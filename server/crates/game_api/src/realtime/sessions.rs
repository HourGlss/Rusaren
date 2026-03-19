use super::{
    error, info, oneshot, warn, Arc, AtomicU64, BindingState, ClientOutbound, ConnectionId,
    DevServerState, IngressEvent, Mutex, NetworkSessionGuard, Ordering, PlayerId,
    RTCPeerConnection, ServerObservability,
};

pub(super) async fn handle_binary_message(
    state: &DevServerState,
    connection_id: ConnectionId,
    outbound: &ClientOutbound,
    guard: &mut NetworkSessionGuard,
    bound_player: &mut Option<PlayerId>,
    packet: Vec<u8>,
) -> bool {
    if let Err(error) = guard.accept_packet(&packet) {
        reject_client_session(outbound, state.observability.as_ref(), &error.to_string());
        return false;
    }

    if bound_player.is_none() {
        return bind_initial_player(state, connection_id, outbound, guard, bound_player, packet)
            .await;
    }

    forward_bound_packet(state, connection_id, packet)
}

/// Processes the first connect packet that binds a transport session to a player.
async fn bind_initial_player(
    state: &DevServerState,
    connection_id: ConnectionId,
    outbound: &ClientOutbound,
    guard: &mut NetworkSessionGuard,
    bound_player: &mut Option<PlayerId>,
    packet: Vec<u8>,
) -> bool {
    let (ack_tx, ack_rx) = oneshot::channel();
    if state
        .ingress_tx
        .send(IngressEvent::Connect {
            connection_id,
            outbound: outbound.clone(),
            packet,
            ack: ack_tx,
        })
        .is_err()
    {
        reject_client_session(
            outbound,
            state.observability.as_ref(),
            "server is shutting down",
        );
        return false;
    }

    match ack_rx.await {
        Ok(Ok(player_id)) => {
            guard.mark_bound();
            if let Some(observability) = &state.observability {
                observability.record_websocket_session_bound();
            }
            info!(
                connection_id = connection_id.get(),
                player_id = player_id.get(),
                "session bound to player"
            );
            *bound_player = Some(player_id);
            true
        }
        Ok(Err(message)) => {
            reject_client_session(outbound, state.observability.as_ref(), &message);
            false
        }
        Err(_) => {
            reject_client_session(
                outbound,
                state.observability.as_ref(),
                "server did not accept the connect request",
            );
            false
        }
    }
}

/// Forwards a bound packet into the application ingress channel.
fn forward_bound_packet(
    state: &DevServerState,
    connection_id: ConnectionId,
    packet: Vec<u8>,
) -> bool {
    if state
        .ingress_tx
        .send(IngressEvent::Packet {
            connection_id,
            packet,
        })
        .is_ok()
    {
        return true;
    }

    if let Some(observability) = &state.observability {
        observability.record_websocket_rejection();
        observability.record_ingress_packet(false);
    }
    error!(
        connection_id = connection_id.get(),
        "realtime ingress channel closed while forwarding packet"
    );
    false
}

/// Disconnects a bound session exactly once and notifies the app layer.
pub(super) async fn disconnect_bound_session(
    state: &DevServerState,
    connection_id: ConnectionId,
    binding_state: &Arc<Mutex<BindingState>>,
    transport_name: &'static str,
) {
    let mut binding = binding_state.lock().await;
    if binding.disconnected {
        return;
    }
    binding.disconnected = true;

    let Some(player_id) = binding.bound_player else {
        return;
    };

    if let Some(observability) = &state.observability {
        observability.record_websocket_disconnect();
    }
    let _ = state
        .ingress_tx
        .send(IngressEvent::Disconnect { connection_id });
    info!(
        connection_id = connection_id.get(),
        player_id = player_id.get(),
        transport = transport_name,
        "bound realtime session disconnected"
    );
}

/// Allocates the next non-zero transport connection id.
pub(super) fn allocate_connection_id(next_connection_id: &AtomicU64) -> ConnectionId {
    let raw = next_connection_id.fetch_add(1, Ordering::Relaxed);
    match ConnectionId::new(raw) {
        Ok(connection_id) => connection_id,
        Err(error) => panic!("generated connection id should be valid: {error}"),
    }
}

/// Sends a protocol-level rejection to a client and records observability counters.
pub(super) fn reject_client_session(
    outbound: &ClientOutbound,
    observability: Option<&ServerObservability>,
    message: &str,
) {
    if let Some(observability) = observability {
        observability.record_websocket_rejection();
        observability.record_ingress_packet(false);
    }
    warn!(%message, "rejecting realtime client session");
    outbound.send_error(message);
}

/// Rejects a `WebRTC` session and closes the peer connection.
pub(super) async fn reject_peer_session(
    outbound: &ClientOutbound,
    observability: Option<&ServerObservability>,
    peer: &Arc<RTCPeerConnection>,
    message: &str,
) {
    reject_client_session(outbound, observability, message);
    let _ = peer.close().await;
}
