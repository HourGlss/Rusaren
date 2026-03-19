use super::*;

/// Creates the `WebRTC` transport state for one newly connected signaling client.
pub(super) async fn create_signaling_transport(
    state: &DevServerState,
    connection_id: ConnectionId,
    signal_tx: &mpsc::UnboundedSender<ServerSignalMessage>,
) -> Result<SignalingTransport, ()> {
    let ice_servers = match state
        .webrtc
        .ice_servers_for_connection(connection_id, SystemTime::now())
    {
        Ok(ice_servers) => ice_servers,
        Err(message) => {
            let _ = signal_tx.send(ServerSignalMessage::Error { message });
            return Err(());
        }
    };

    let peer = match create_peer_connection(&ice_servers).await {
        Ok(peer) => Arc::new(peer),
        Err(error) => {
            let _ = signal_tx.send(ServerSignalMessage::Error {
                message: format!("failed to create the WebRTC peer connection: {error}"),
            });
            return Err(());
        }
    };

    let binding_state = Arc::new(Mutex::new(BindingState::default()));
    let negotiation_state = Arc::new(Mutex::new(WebRtcNegotiationState::default()));
    let outbound = match create_webrtc_outbound(
        state,
        connection_id,
        Arc::clone(&peer),
        Arc::clone(&binding_state),
    )
    .await
    {
        Ok(outbound) => outbound,
        Err(message) => {
            let _ = signal_tx.send(ServerSignalMessage::Error { message });
            let _ = peer.close().await;
            return Err(());
        }
    };

    Ok(SignalingTransport {
        peer,
        binding_state,
        negotiation_state,
        outbound,
        ice_servers,
    })
}

/// Accepts the client's SDP offer and emits the server answer.
pub(super) async fn accept_webrtc_offer(
    connection_id: ConnectionId,
    signal_tx: &mpsc::UnboundedSender<ServerSignalMessage>,
    peer: &Arc<RTCPeerConnection>,
    negotiation_state: &Arc<Mutex<WebRtcNegotiationState>>,
    description: SignalingSessionDescription,
) -> bool {
    let mut negotiation = negotiation_state.lock().await;
    if negotiation.offer_seen {
        let _ = signal_tx.send(ServerSignalMessage::Error {
            message: String::from("a WebRTC offer has already been processed"),
        });
        drop(negotiation);
        let _ = peer.close().await;
        return false;
    }

    let remote_description = match description.to_rtc_description() {
        Ok(remote_description) => remote_description,
        Err(message) => {
            let _ = signal_tx.send(ServerSignalMessage::Error { message });
            drop(negotiation);
            let _ = peer.close().await;
            return false;
        }
    };

    if let Err(error) = peer.set_remote_description(remote_description).await {
        let _ = signal_tx.send(ServerSignalMessage::Error {
            message: format!("failed to apply the remote offer: {error}"),
        });
        drop(negotiation);
        let _ = peer.close().await;
        return false;
    }

    let answer = match peer.create_answer(None).await {
        Ok(answer) => answer,
        Err(error) => {
            let _ = signal_tx.send(ServerSignalMessage::Error {
                message: format!("failed to create the WebRTC answer: {error}"),
            });
            drop(negotiation);
            let _ = peer.close().await;
            return false;
        }
    };
    if let Err(error) = peer.set_local_description(answer.clone()).await {
        let _ = signal_tx.send(ServerSignalMessage::Error {
            message: format!("failed to apply the local answer: {error}"),
        });
        drop(negotiation);
        let _ = peer.close().await;
        return false;
    }

    let answer_description = peer.local_description().await.unwrap_or(answer);
    negotiation.offer_seen = true;
    let _ = signal_tx.send(ServerSignalMessage::SessionDescription {
        description: SignalingSessionDescription::from_rtc_description(&answer_description),
    });
    negotiation.answer_sent = true;

    for candidate in negotiation.pending_local_candidates.drain(..) {
        let _ = signal_tx.send(ServerSignalMessage::IceCandidate { candidate });
    }

    info!(
        connection_id = connection_id.get(),
        "WebRTC offer accepted and answer sent"
    );
    true
}

/// Adds one remote ICE candidate after an offer has been accepted.
pub(super) async fn add_remote_ice_candidate(
    signal_tx: &mpsc::UnboundedSender<ServerSignalMessage>,
    peer: &Arc<RTCPeerConnection>,
    negotiation_state: &Arc<Mutex<WebRtcNegotiationState>>,
    candidate: SignalingIceCandidate,
) -> bool {
    let negotiation = negotiation_state.lock().await;
    if !negotiation.offer_seen {
        let _ = signal_tx.send(ServerSignalMessage::Error {
            message: String::from("ICE candidates are not accepted before an offer"),
        });
        drop(negotiation);
        let _ = peer.close().await;
        return false;
    }
    drop(negotiation);

    let candidate_init = match candidate.to_rtc_candidate_init() {
        Ok(candidate_init) => candidate_init,
        Err(message) => {
            let _ = signal_tx.send(ServerSignalMessage::Error { message });
            let _ = peer.close().await;
            return false;
        }
    };

    if let Err(error) = peer.add_ice_candidate(candidate_init).await {
        let _ = signal_tx.send(ServerSignalMessage::Error {
            message: format!("failed to add the remote ICE candidate: {error}"),
        });
        let _ = peer.close().await;
        return false;
    }
    true
}

/// Creates negotiated `WebRTC` data channels and the server-side outbound handles for them.
async fn create_webrtc_outbound(
    state: &DevServerState,
    connection_id: ConnectionId,
    peer: Arc<RTCPeerConnection>,
    binding_state: Arc<Mutex<BindingState>>,
) -> Result<ClientOutbound, String> {
    let (control_tx, control_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let (snapshot_tx, snapshot_rx) = mpsc::unbounded_channel::<Vec<u8>>();

    let control_channel = peer
        .create_data_channel("control", Some(control_channel_init()))
        .await
        .map_err(|error| format!("failed to create the control data channel: {error}"))?;
    let input_channel = peer
        .create_data_channel(
            "input",
            Some(unreliable_channel_init(INPUT_DATA_CHANNEL_ID)),
        )
        .await
        .map_err(|error| format!("failed to create the input data channel: {error}"))?;
    let snapshot_channel = peer
        .create_data_channel(
            "snapshot",
            Some(unreliable_channel_init(SNAPSHOT_DATA_CHANNEL_ID)),
        )
        .await
        .map_err(|error| format!("failed to create the snapshot data channel: {error}"))?;

    tokio::spawn(run_webrtc_channel_writer(
        "control",
        Arc::clone(&control_channel),
        control_rx,
    ));
    tokio::spawn(run_webrtc_channel_writer(
        "snapshot",
        Arc::clone(&snapshot_channel),
        snapshot_rx,
    ));

    let outbound = ClientOutbound::WebRtc {
        control: control_tx.clone(),
        snapshot: snapshot_tx.clone(),
    };

    install_webrtc_message_handler(
        state.clone(),
        connection_id,
        &peer,
        &binding_state,
        outbound.clone(),
        &control_channel,
        ChannelId::Control,
    );
    install_webrtc_message_handler(
        state.clone(),
        connection_id,
        &peer,
        &binding_state,
        outbound.clone(),
        &input_channel,
        ChannelId::Input,
    );
    install_snapshot_rejection_handler(state.clone(), &peer, outbound.clone(), &snapshot_channel);

    Ok(outbound)
}

/// Installs peer-state and ICE-candidate callbacks on a negotiated peer connection.
pub(super) fn install_webrtc_callbacks(
    state: &DevServerState,
    connection_id: ConnectionId,
    peer: &Arc<RTCPeerConnection>,
    binding_state: &Arc<Mutex<BindingState>>,
    negotiation_state: &Arc<Mutex<WebRtcNegotiationState>>,
    outbound: ClientOutbound,
    signal_tx: mpsc::UnboundedSender<ServerSignalMessage>,
) {
    let state_for_disconnect = state.clone();
    let binding_for_disconnect = Arc::clone(binding_state);
    let outbound_for_errors = outbound;
    let peer_for_state = Arc::clone(peer);
    peer.on_peer_connection_state_change(Box::new(move |peer_state| {
        let state = state_for_disconnect.clone();
        let binding_state = Arc::clone(&binding_for_disconnect);
        let outbound = outbound_for_errors.clone();
        let peer = Arc::clone(&peer_for_state);
        Box::pin(async move {
            info!(
                connection_id = connection_id.get(),
                peer_state = %peer_state,
                "WebRTC peer connection state changed"
            );
            if peer_state == RTCPeerConnectionState::Failed {
                outbound.send_error("the WebRTC transport failed");
                let _ = peer.close().await;
            }
            if matches!(
                peer_state,
                RTCPeerConnectionState::Disconnected
                    | RTCPeerConnectionState::Failed
                    | RTCPeerConnectionState::Closed
            ) {
                disconnect_bound_session(&state, connection_id, &binding_state, "WebRTC").await;
            }
        })
    }));

    let negotiation_for_candidates = Arc::clone(negotiation_state);
    peer.on_ice_candidate(Box::new(move |candidate| {
        let signal_tx = signal_tx.clone();
        let negotiation_state = Arc::clone(&negotiation_for_candidates);
        Box::pin(async move {
            let Some(candidate) = candidate else {
                return;
            };

            let candidate_init = match candidate.to_json() {
                Ok(candidate_init) => candidate_init,
                Err(error) => {
                    warn!(%error, "failed to serialize local ICE candidate");
                    return;
                }
            };
            let candidate = SignalingIceCandidate::from_rtc_candidate_init(&candidate_init);
            let mut negotiation = negotiation_state.lock().await;
            if negotiation.answer_sent {
                let _ = signal_tx.send(ServerSignalMessage::IceCandidate { candidate });
            } else {
                negotiation.pending_local_candidates.push(candidate);
            }
        })
    }));
}

/// Installs one packet handler for an inbound `WebRTC` data channel.
fn install_webrtc_message_handler(
    state: DevServerState,
    connection_id: ConnectionId,
    peer: &Arc<RTCPeerConnection>,
    binding_state: &Arc<Mutex<BindingState>>,
    outbound: ClientOutbound,
    data_channel: &Arc<RTCDataChannel>,
    expected_channel: ChannelId,
) {
    let peer = Arc::clone(peer);
    let binding_state = Arc::clone(binding_state);
    data_channel.on_message(Box::new(move |message: DataChannelMessage| {
        let state = state.clone();
        let peer = Arc::clone(&peer);
        let binding_state = Arc::clone(&binding_state);
        let outbound = outbound.clone();
        Box::pin(async move {
            if message.is_string {
                reject_peer_session(
                    &outbound,
                    state.observability.as_ref(),
                    &peer,
                    "text data-channel messages are not accepted",
                )
                .await;
                disconnect_bound_session(&state, connection_id, &binding_state, "WebRTC").await;
                return;
            }

            let packet = message.data.to_vec();
            let keep_open = handle_webrtc_binary_message(
                &state,
                connection_id,
                &peer,
                &outbound,
                &binding_state,
                expected_channel,
                packet,
            )
            .await;
            if !keep_open {
                disconnect_bound_session(&state, connection_id, &binding_state, "WebRTC").await;
            }
        })
    }));
}

/// Rejects any attempt by a client to send payloads on the snapshot channel.
fn install_snapshot_rejection_handler(
    state: DevServerState,
    peer: &Arc<RTCPeerConnection>,
    outbound: ClientOutbound,
    data_channel: &Arc<RTCDataChannel>,
) {
    let peer = Arc::clone(peer);
    data_channel.on_message(Box::new(move |_message: DataChannelMessage| {
        let state = state.clone();
        let peer = Arc::clone(&peer);
        let outbound = outbound.clone();
        Box::pin(async move {
            reject_peer_session(
                &outbound,
                state.observability.as_ref(),
                &peer,
                "clients must not send packets on the snapshot data channel",
            )
            .await;
        })
    }));
}

/// Writes queued packets onto one `WebRTC` data channel once it is open.
async fn run_webrtc_channel_writer(
    label: &'static str,
    data_channel: Arc<RTCDataChannel>,
    mut outbound_rx: mpsc::UnboundedReceiver<Vec<u8>>,
) {
    loop {
        match data_channel.ready_state() {
            RTCDataChannelState::Open => break,
            RTCDataChannelState::Closing | RTCDataChannelState::Closed => return,
            _ => tokio::time::sleep(Duration::from_millis(10)).await,
        }
    }

    while let Some(packet) = outbound_rx.recv().await {
        if data_channel.ready_state() != RTCDataChannelState::Open {
            break;
        }
        if let Err(error) = data_channel.send(&Bytes::from(packet)).await {
            warn!(channel = label, %error, "failed to send a WebRTC packet");
            break;
        }
    }
}

/// Creates a `webrtc` peer connection configured with the supplied ICE servers.
async fn create_peer_connection(
    ice_servers: &[crate::WebRtcIceServerConfig],
) -> Result<RTCPeerConnection, webrtc::Error> {
    let mut media_engine = MediaEngine::default();
    media_engine.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(media_engine).build();
    let configuration = RTCConfiguration {
        ice_servers: ice_servers
            .iter()
            .map(crate::WebRtcIceServerConfig::to_rtc_ice_server)
            .collect(),
        ..Default::default()
    };
    api.new_peer_connection(configuration).await
}

/// Returns the negotiated settings for the reliable ordered control channel.
const fn control_channel_init() -> RTCDataChannelInit {
    RTCDataChannelInit {
        ordered: Some(true),
        max_packet_life_time: None,
        max_retransmits: None,
        protocol: None,
        negotiated: Some(CONTROL_DATA_CHANNEL_ID),
    }
}

/// Returns the negotiated settings for the unreliable gameplay channels.
const fn unreliable_channel_init(id: u16) -> RTCDataChannelInit {
    RTCDataChannelInit {
        ordered: Some(false),
        max_packet_life_time: None,
        max_retransmits: Some(0),
        protocol: None,
        negotiated: Some(id),
    }
}

/// Validates and dispatches one inbound binary data-channel payload.
async fn handle_webrtc_binary_message(
    state: &DevServerState,
    connection_id: ConnectionId,
    peer: &Arc<RTCPeerConnection>,
    outbound: &ClientOutbound,
    binding_state: &Arc<Mutex<BindingState>>,
    expected_channel: ChannelId,
    packet: Vec<u8>,
) -> bool {
    if let Err(message) = validate_webrtc_packet_channel(&packet, expected_channel) {
        reject_peer_session(outbound, state.observability.as_ref(), peer, &message).await;
        return false;
    }

    let mut binding = binding_state.lock().await;
    let BindingState {
        guard,
        bound_player,
        ..
    } = &mut *binding;
    let keep_open =
        handle_binary_message(state, connection_id, outbound, guard, bound_player, packet).await;
    if !keep_open {
        let _ = peer.close().await;
    }
    keep_open
}

/// Verifies that a packet arrived on the correct negotiated data channel.
fn validate_webrtc_packet_channel(
    packet: &[u8],
    expected_channel: ChannelId,
) -> Result<(), String> {
    let (header, _) = PacketHeader::decode(packet).map_err(|error| error.to_string())?;
    if header.channel_id != expected_channel {
        return Err(format!(
            "packet header channel {:?} does not match the {:?} data channel",
            header.channel_id, expected_channel
        ));
    }

    match expected_channel {
        ChannelId::Control if header.packet_kind != PacketKind::ControlCommand => Err(format!(
            "the control data channel only accepts ControlCommand packets, received {:?}",
            header.packet_kind
        )),
        ChannelId::Input if header.packet_kind != PacketKind::InputFrame => Err(format!(
            "the input data channel only accepts InputFrame packets, received {:?}",
            header.packet_kind
        )),
        ChannelId::Snapshot => Err(String::from(
            "clients must not send packets on the snapshot data channel",
        )),
        _ => Ok(()),
    }
}
