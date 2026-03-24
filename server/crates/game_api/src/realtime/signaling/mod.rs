use super::*;

use super::sessions::{
    allocate_connection_id, disconnect_bound_session, handle_binary_message, reject_client_session,
    reject_peer_session,
};

mod transport;

#[cfg(test)]
pub(crate) use transport::validate_webrtc_packet_channel;
use transport::{
    accept_webrtc_offer, add_remote_ice_candidate, create_signaling_transport,
    install_webrtc_callbacks,
};

fn spawn_signaling_writer(
    mut sender: futures_util::stream::SplitSink<WebSocket, Message>,
    mut signal_rx: mpsc::UnboundedReceiver<ServerSignalMessage>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
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
    })
}

pub(super) async fn handle_signaling_socket(state: DevServerState, socket: WebSocket) {
    let (sender, mut receiver) = socket.split();
    let (signal_tx, signal_rx) = mpsc::unbounded_channel::<ServerSignalMessage>();
    let writer = spawn_signaling_writer(sender, signal_rx);

    let connection_id = allocate_connection_id(&state.next_connection_id);
    info!(
        connection_id = connection_id.get(),
        "WebRTC signaling session opened"
    );
    if let Some(observability) = &state.observability {
        observability.record_diagnostic(
            "signaling",
            Some(connection_id.get()),
            None,
            "WebRTC signaling websocket opened",
        );
    }

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
    if let Some(observability) = &state.observability {
        observability.record_diagnostic(
            "signaling",
            Some(connection_id.get()),
            None,
            "sent signaling hello with ICE server configuration",
        );
    }

    let close_reason = run_signaling_socket_loop(
        &state,
        connection_id,
        &signal_tx,
        &transport.peer,
        &transport.negotiation_state,
        &mut receiver,
    )
    .await;

    disconnect_bound_session(
        &state,
        connection_id,
        &transport.binding_state,
        "WebRTC",
        &close_reason,
    )
    .await;
    let _ = transport.peer.close().await;
    drop(signal_tx);
    let _ = writer.await;
    info!(
        connection_id = connection_id.get(),
        "WebRTC signaling session closed"
    );
    if let Some(observability) = &state.observability {
        observability.record_diagnostic(
            "signaling",
            Some(connection_id.get()),
            None,
            format!("WebRTC signaling websocket closed: {close_reason}"),
        );
    }
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
    if let Some(observability) = &state.observability {
        observability.record_diagnostic(
            "ws_dev",
            Some(connection_id.get()),
            None,
            "legacy websocket gameplay session opened",
        );
    }

    let close_reason = run_websocket_dev_loop(
        &state,
        connection_id,
        &outbound,
        &mut guard,
        &mut bound_player,
        &mut receiver,
    )
    .await;

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
            reason = %close_reason,
            "websocket dev session disconnected after binding"
        );
    }

    drop(outbound_tx);
    let _ = writer.await;
    info!(
        connection_id = connection_id.get(),
        "websocket dev session closed"
    );
    if let Some(observability) = &state.observability {
        observability.record_diagnostic(
            "ws_dev",
            Some(connection_id.get()),
            bound_player.map(|player_id: PlayerId| u64::from(player_id.get())),
            format!("legacy websocket gameplay session closed: {close_reason}"),
        );
    }
}

async fn run_signaling_socket_loop(
    state: &DevServerState,
    connection_id: ConnectionId,
    signal_tx: &mpsc::UnboundedSender<ServerSignalMessage>,
    peer: &Arc<RTCPeerConnection>,
    negotiation_state: &Arc<Mutex<WebRtcNegotiationState>>,
    receiver: &mut futures_util::stream::SplitStream<WebSocket>,
) -> String {
    let mut close_reason = String::from("signaling websocket loop ended");
    while let Some(message_result) = receiver.next().await {
        let message = match message_result {
            Ok(message) => message,
            Err(error) => {
                warn!(
                    connection_id = connection_id.get(),
                    %error,
                    "WebRTC signaling websocket ended with an error"
                );
                if let Some(observability) = &state.observability {
                    observability.record_diagnostic(
                        "signaling",
                        Some(connection_id.get()),
                        None,
                        format!("signaling websocket ended with an error: {error}"),
                    );
                }
                return format!("signaling websocket read error: {error}");
            }
        };

        let keep_open = process_signaling_websocket_message(
            state,
            connection_id,
            signal_tx,
            peer,
            negotiation_state,
            message,
        )
        .await;

        if !keep_open {
            close_reason = String::from("signaling message handler closed the session");
            break;
        }
    }
    close_reason
}

async fn run_websocket_dev_loop(
    state: &DevServerState,
    connection_id: ConnectionId,
    outbound: &ClientOutbound,
    guard: &mut NetworkSessionGuard,
    bound_player: &mut Option<PlayerId>,
    receiver: &mut futures_util::stream::SplitStream<WebSocket>,
) -> String {
    let mut close_reason = String::from("websocket dev session loop ended");
    while let Some(message_result) = receiver.next().await {
        let message = match message_result {
            Ok(message) => message,
            Err(error) => {
                warn!(
                    connection_id = connection_id.get(),
                    %error,
                    "websocket dev stream ended with an error"
                );
                if let Some(observability) = &state.observability {
                    observability.record_diagnostic(
                        "ws_dev",
                        Some(connection_id.get()),
                        bound_player.map(|player_id| u64::from(player_id.get())),
                        format!("websocket dev stream ended with an error: {error}"),
                    );
                }
                return format!("websocket dev read error: {error}");
            }
        };

        match message {
            Message::Binary(bytes) => {
                let keep_open = handle_binary_message(
                    state,
                    connection_id,
                    outbound,
                    guard,
                    bound_player,
                    bytes.to_vec(),
                )
                .await;
                if !keep_open {
                    close_reason = String::from("websocket dev packet handler closed the session");
                    break;
                }
            }
            Message::Text(_) => {
                reject_client_session(
                    outbound,
                    state.observability.as_ref(),
                    "text websocket messages are not accepted",
                );
                close_reason = String::from("websocket dev client sent a text frame");
                break;
            }
            Message::Close(frame) => {
                close_reason = match frame {
                    Some(frame) => format!(
                        "websocket dev close frame code={} reason={}",
                        frame.code, frame.reason
                    ),
                    None => String::from("websocket dev close frame without details"),
                };
                if let Some(observability) = &state.observability {
                    observability.record_diagnostic(
                        "ws_dev",
                        Some(connection_id.get()),
                        bound_player.map(|player_id| u64::from(player_id.get())),
                        close_reason.clone(),
                    );
                }
                break;
            }
            Message::Ping(_) | Message::Pong(_) => {}
        }
    }
    close_reason
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
            if let Some(observability) = &state.observability {
                observability.record_diagnostic(
                    "signaling",
                    Some(connection_id.get()),
                    None,
                    "client sent an unexpected binary websocket frame on /ws",
                );
            }
            let _ = signal_tx.send(ServerSignalMessage::Error {
                message: String::from("binary websocket messages are not accepted on /ws"),
            });
            false
        }
        Message::Close(frame) => {
            if let Some(observability) = &state.observability {
                let detail = signaling_close_detail(frame.as_ref());
                observability.record_diagnostic(
                    "signaling",
                    Some(connection_id.get()),
                    None,
                    detail,
                );
            }
            false
        }
        Message::Ping(_) | Message::Pong(_) => true,
    }
}

fn signaling_close_detail(frame: Option<&axum::extract::ws::CloseFrame>) -> String {
    match frame {
        Some(frame) => format!(
            "client closed signaling websocket with code={} reason={}",
            frame.code, frame.reason
        ),
        None => String::from("client closed signaling websocket without details"),
    }
}

/// Handles one validated client signaling message against a live peer connection.
async fn handle_signaling_message(
    state: &DevServerState,
    connection_id: ConnectionId,
    signal_tx: &mpsc::UnboundedSender<ServerSignalMessage>,
    peer: &Arc<RTCPeerConnection>,
    negotiation_state: &Arc<Mutex<WebRtcNegotiationState>>,
    message_text: &str,
) -> bool {
    let message = match decode_client_signal_message(message_text) {
        Ok(message) => message,
        Err(message) => {
            if let Some(observability) = &state.observability {
                observability.record_diagnostic(
                    "signaling",
                    Some(connection_id.get()),
                    None,
                    format!("rejected signaling message: {message}"),
                );
            }
            let _ = signal_tx.send(ServerSignalMessage::Error { message });
            let _ = peer.close().await;
            return false;
        }
    };

    match message {
        ClientSignalMessage::SessionDescription { description } => {
            accept_webrtc_offer(
                state,
                connection_id,
                signal_tx,
                peer,
                negotiation_state,
                description,
            )
            .await
        }
        ClientSignalMessage::IceCandidate { candidate } => {
            add_remote_ice_candidate(
                state,
                connection_id,
                signal_tx,
                peer,
                negotiation_state,
                candidate,
            )
            .await
        }
        ClientSignalMessage::Bye => false,
    }
}
