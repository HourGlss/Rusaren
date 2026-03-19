use super::*;

use super::sessions::{
    allocate_connection_id, disconnect_bound_session, handle_binary_message, reject_client_session,
    reject_peer_session,
};

mod transport;

use transport::{
    accept_webrtc_offer, add_remote_ice_candidate, create_signaling_transport,
    install_webrtc_callbacks,
};

pub(super) async fn handle_signaling_socket(state: DevServerState, socket: WebSocket) {
    let (mut sender, mut receiver) = socket.split();
    let (signal_tx, mut signal_rx) = mpsc::unbounded_channel::<ServerSignalMessage>();
    let writer = tokio::spawn(async move {
        while let Some(message) = signal_rx.recv().await {
            let text = match serde_json::to_string(&message) {
                Ok(text) => text,
                Err(error) => {
                    error!(%error, "failed to serialize WebRTC signaling message");
                    break;
                }
            };

            if sender.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    let connection_id = allocate_connection_id(&state.next_connection_id);
    info!(
        connection_id = connection_id.get(),
        "WebRTC signaling session opened"
    );

    let Ok(transport) = create_signaling_transport(&state, connection_id, &signal_tx).await else {
        drop(signal_tx);
        let _ = writer.await;
        return;
    };

    install_webrtc_callbacks(
        &state,
        connection_id,
        &transport.peer,
        &transport.binding_state,
        &transport.negotiation_state,
        transport.outbound.clone(),
        signal_tx.clone(),
    );

    let _ = signal_tx.send(ServerSignalMessage::Hello {
        protocol_version: PROTOCOL_VERSION,
        ice_servers: transport.ice_servers.clone(),
        channels: SignalingChannelMap::default(),
    });

    while let Some(message_result) = receiver.next().await {
        let Ok(message) = message_result else {
            warn!(
                connection_id = connection_id.get(),
                "WebRTC signaling websocket ended with an error"
            );
            break;
        };

        let keep_open = process_signaling_websocket_message(
            &state,
            connection_id,
            &signal_tx,
            &transport.peer,
            &transport.negotiation_state,
            message,
        )
        .await;

        if !keep_open {
            break;
        }
    }

    disconnect_bound_session(&state, connection_id, &transport.binding_state, "WebRTC").await;
    let _ = transport.peer.close().await;
    drop(signal_tx);
    let _ = writer.await;
    info!(
        connection_id = connection_id.get(),
        "WebRTC signaling session closed"
    );
}

/// Runs the legacy raw websocket gameplay adapter used by local tests and fallback flows.
pub(super) async fn handle_websocket_dev_socket(state: DevServerState, socket: WebSocket) {
    let (mut sender, mut receiver) = socket.split();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let outbound = ClientOutbound::WebSocket {
        outbound: outbound_tx.clone(),
    };
    let connection_id = allocate_connection_id(&state.next_connection_id);
    let writer = tokio::spawn(async move {
        while let Some(packet) = outbound_rx.recv().await {
            if sender.send(Message::Binary(packet.into())).await.is_err() {
                break;
            }
        }
    });

    let mut guard = NetworkSessionGuard::new();
    let mut bound_player = None;
    info!(
        connection_id = connection_id.get(),
        "websocket dev session opened"
    );

    while let Some(message_result) = receiver.next().await {
        let Ok(message) = message_result else {
            warn!(
                connection_id = connection_id.get(),
                "websocket dev stream ended with an error"
            );
            break;
        };

        match message {
            Message::Binary(bytes) => {
                let keep_open = handle_binary_message(
                    &state,
                    connection_id,
                    &outbound,
                    &mut guard,
                    &mut bound_player,
                    bytes.to_vec(),
                )
                .await;
                if !keep_open {
                    break;
                }
            }
            Message::Text(_) => {
                reject_client_session(
                    &outbound,
                    state.observability.as_ref(),
                    "text websocket messages are not accepted",
                );
                break;
            }
            Message::Close(_) => break,
            Message::Ping(_) | Message::Pong(_) => {}
        }
    }

    if let Some(player_id) = bound_player {
        if let Some(observability) = &state.observability {
            observability.record_websocket_disconnect();
        }
        let _ = state
            .ingress_tx
            .send(IngressEvent::Disconnect { connection_id });
        info!(
            connection_id = connection_id.get(),
            player_id = player_id.get(),
            "websocket dev session disconnected after binding"
        );
    }

    drop(outbound_tx);
    let _ = writer.await;
    info!(
        connection_id = connection_id.get(),
        "websocket dev session closed"
    );
}

/// Dispatches one websocket frame received on `/ws`.
async fn process_signaling_websocket_message(
    state: &DevServerState,
    connection_id: ConnectionId,
    signal_tx: &mpsc::UnboundedSender<ServerSignalMessage>,
    peer: &Arc<RTCPeerConnection>,
    negotiation_state: &Arc<Mutex<WebRtcNegotiationState>>,
    message: Message,
) -> bool {
    match message {
        Message::Text(text) => {
            handle_signaling_message(
                state,
                connection_id,
                signal_tx,
                peer,
                negotiation_state,
                text.as_str(),
            )
            .await
        }
        Message::Binary(_) => {
            let _ = signal_tx.send(ServerSignalMessage::Error {
                message: String::from("binary websocket messages are not accepted on /ws"),
            });
            false
        }
        Message::Close(_) => false,
        Message::Ping(_) | Message::Pong(_) => true,
    }
}

/// Handles one validated client signaling message against a live peer connection.
async fn handle_signaling_message(
    _state: &DevServerState,
    connection_id: ConnectionId,
    signal_tx: &mpsc::UnboundedSender<ServerSignalMessage>,
    peer: &Arc<RTCPeerConnection>,
    negotiation_state: &Arc<Mutex<WebRtcNegotiationState>>,
    message_text: &str,
) -> bool {
    let message = match decode_client_signal_message(message_text) {
        Ok(message) => message,
        Err(message) => {
            let _ = signal_tx.send(ServerSignalMessage::Error { message });
            let _ = peer.close().await;
            return false;
        }
    };

    match message {
        ClientSignalMessage::SessionDescription { description } => {
            accept_webrtc_offer(
                connection_id,
                signal_tx,
                peer,
                negotiation_state,
                description,
            )
            .await
        }
        ClientSignalMessage::IceCandidate { candidate } => {
            add_remote_ice_candidate(signal_tx, peer, negotiation_state, candidate).await
        }
        ClientSignalMessage::Bye => false,
    }
}
