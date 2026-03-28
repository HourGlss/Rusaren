use std::collections::{BTreeMap, VecDeque};
use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::Query;
use axum::extract::State;
use axum::http::{header, HeaderMap, Request, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, get_service};
use axum::Json;
use axum::Router;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64_URL_SAFE_NO_PAD;
use base64::Engine as _;
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use game_content::GameContent;
use game_domain::PlayerId;
use game_net::{
    ChannelId, NetworkSessionGuard, PacketHeader, PacketKind, ServerControlEvent,
    MAX_INGRESS_PACKET_BYTES, PROTOCOL_VERSION,
};
use game_sim::COMBAT_FRAME_MS;
use getrandom::fill as getrandom_fill;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::task::JoinHandle;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing::{debug, error, info, info_span, warn};
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::APIBuilder;
use webrtc::data_channel::data_channel_init::RTCDataChannelInit;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::data_channel::data_channel_state::RTCDataChannelState;
use webrtc::data_channel::RTCDataChannel;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::RTCPeerConnection;

use crate::observability::{classify_http_path, ServerObservability};
use crate::webrtc::{
    decode_client_signal_message, ClientSignalMessage, ServerSignalMessage, SignalingChannelMap,
    SignalingIceCandidate, SignalingSessionDescription, WebRtcRuntimeConfig,
    CONTROL_DATA_CHANNEL_ID, INPUT_DATA_CHANNEL_ID, MAX_SIGNAL_MESSAGE_BYTES,
    SNAPSHOT_DATA_CHANNEL_ID,
};
use crate::{AppTransport, ConnectionId, ServerApp};

const SESSION_BOOTSTRAP_TOKEN_BYTES: usize = 24;
const SESSION_BOOTSTRAP_TOKEN_TTL: Duration = Duration::from_secs(30);
const MAX_SESSION_BOOTSTRAP_TOKEN_BYTES: usize = 96;

#[derive(Clone)]
struct DevServerState {
    runtime: Arc<Mutex<RuntimeState>>,
    ingress_tx: mpsc::UnboundedSender<IngressEvent>,
    web_client_root: PathBuf,
    observability: Option<ServerObservability>,
    next_connection_id: Arc<AtomicU64>,
    bootstrap_tokens: Arc<Mutex<SessionBootstrapTokenRegistry>>,
    webrtc: WebRtcRuntimeConfig,
    admin_auth: Option<AdminAuthConfig>,
}

struct RuntimeState {
    app: ServerApp,
    transport: RealtimeTransport,
    observability: Option<ServerObservability>,
}

impl RuntimeState {
    /// Drains any currently queued transport packets into the app core.
    fn pump_transport(&mut self) {
        let Self { app, transport, .. } = self;
        app.pump_transport(transport);
    }

    /// Advances application time without exposing the transport internals to callers.
    fn advance_millis(&mut self, delta_ms: u16) {
        let Self { app, transport, .. } = self;
        app.advance_millis(transport, delta_ms);
    }

    /// Disconnects one bound connection from the app core.
    fn disconnect_connection(&mut self, connection_id: ConnectionId) {
        let Self { app, transport, .. } = self;
        let _ = app.disconnect_connection(transport, connection_id);
    }
}

#[derive(Clone)]
enum ClientOutbound {
    WebSocket {
        outbound: mpsc::UnboundedSender<Vec<u8>>,
    },
    WebRtc {
        control: mpsc::UnboundedSender<Vec<u8>>,
        snapshot: mpsc::UnboundedSender<Vec<u8>>,
    },
}

impl ClientOutbound {
    /// Routes one encoded server packet to the correct transport-specific sink.
    fn send_packet(&self, packet: Vec<u8>) {
        match self {
            Self::WebSocket { outbound } => {
                let _ = outbound.send(packet);
            }
            Self::WebRtc { control, snapshot } => match PacketHeader::decode(&packet) {
                Ok((header, _)) => match header.channel_id {
                    ChannelId::Control => {
                        let _ = control.send(packet);
                    }
                    ChannelId::Snapshot => {
                        let _ = snapshot.send(packet);
                    }
                    ChannelId::Input => {
                        warn!("server attempted to send an input-channel packet to a client");
                    }
                },
                Err(error) => {
                    warn!(%error, "server attempted to send a malformed packet to a client");
                }
            },
        }
    }

    /// Sends a control-plane error packet if it can be encoded successfully.
    fn send_error(&self, message: &str) {
        if let Ok(packet) = (ServerControlEvent::Error {
            message: message.to_string(),
        })
        .encode_packet(0, 0)
        {
            self.send_packet(packet);
        }
    }
}

struct RealtimeTransport {
    incoming: VecDeque<(ConnectionId, Vec<u8>)>,
    outgoing: BTreeMap<ConnectionId, ClientOutbound>,
}

impl RealtimeTransport {
    /// Creates an empty realtime transport.
    fn new() -> Self {
        Self {
            incoming: VecDeque::new(),
            outgoing: BTreeMap::new(),
        }
    }

    /// Registers the outbound path for one connection.
    fn register_client(
        &mut self,
        connection_id: ConnectionId,
        outbound: ClientOutbound,
    ) -> Result<(), &'static str> {
        if self.outgoing.contains_key(&connection_id) {
            return Err("connection is already registered");
        }

        self.outgoing.insert(connection_id, outbound);
        Ok(())
    }

    /// Removes the outbound path for one connection.
    fn unregister_client(&mut self, connection_id: ConnectionId) {
        self.outgoing.remove(&connection_id);
    }

    /// Queues one inbound client packet for later processing.
    fn enqueue(&mut self, connection_id: ConnectionId, packet: Vec<u8>) {
        self.incoming.push_back((connection_id, packet));
    }
}

impl AppTransport for RealtimeTransport {
    fn recv_from_client(&mut self) -> Option<(ConnectionId, Vec<u8>)> {
        self.incoming.pop_front()
    }

    fn send_to_client(&mut self, connection_id: ConnectionId, packet: Vec<u8>) {
        if let Some(outbound) = self.outgoing.get(&connection_id) {
            outbound.send_packet(packet);
        }
    }
}

enum IngressEvent {
    Connect {
        connection_id: ConnectionId,
        outbound: ClientOutbound,
        packet: Vec<u8>,
        ack: oneshot::Sender<Result<PlayerId, String>>,
    },
    Packet {
        connection_id: ConnectionId,
        packet: Vec<u8>,
    },
    Disconnect {
        connection_id: ConnectionId,
    },
}

/// Running dev-server handle used by tests and local launch scripts.
pub struct DevServerHandle {
    local_addr: SocketAddr,
    shutdown_tx: Option<oneshot::Sender<()>>,
    server_task: JoinHandle<()>,
    ingress_task: JoinHandle<()>,
    tick_task: JoinHandle<()>,
}

/// Password-protected admin surface configuration for the hosted backend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminAuthConfig {
    username: String,
    password: String,
}

impl AdminAuthConfig {
    /// Creates validated basic-auth credentials for the read-only admin surface.
    pub fn new(username: impl Into<String>, password: impl Into<String>) -> Result<Self, String> {
        let username = username.into().trim().to_string();
        let password = password.into();
        if username.is_empty() {
            return Err(String::from("admin username must not be blank"));
        }
        if password.trim().is_empty() {
            return Err(String::from("admin password must not be blank"));
        }

        Ok(Self { username, password })
    }

    /// Returns the configured admin username.
    #[must_use]
    pub fn username(&self) -> &str {
        &self.username
    }

    fn is_authorized(&self, headers: &HeaderMap) -> bool {
        let Some(raw_header) = headers.get(header::AUTHORIZATION) else {
            return false;
        };
        let Ok(raw_header) = raw_header.to_str() else {
            return false;
        };
        let Some(encoded) = raw_header.strip_prefix("Basic ") else {
            return false;
        };
        let Ok(decoded) = BASE64_STANDARD.decode(encoded) else {
            return false;
        };
        let Ok(decoded) = String::from_utf8(decoded) else {
            return false;
        };

        decoded == format!("{}:{}", self.username, self.password)
    }
}

/// Runtime options for the websocket/WebRTC dev server.
#[derive(Clone, Debug)]
pub struct DevServerOptions {
    /// Real wall-clock interval between server ticks.
    pub tick_interval: Duration,
    /// Simulated milliseconds advanced per tick.
    pub simulation_step_ms: u16,
    /// Path to the persistent player-record store.
    pub record_store_path: PathBuf,
    /// Path to the persistent server-authored combat log.
    pub combat_log_path: PathBuf,
    /// Root directory that holds runtime-authored content.
    pub content_root: PathBuf,
    /// Root directory that holds the exported web client.
    pub web_client_root: PathBuf,
    /// Optional Prometheus-style observability registry.
    pub observability: Option<ServerObservability>,
    /// STUN/TURN runtime configuration for `WebRTC` clients.
    pub webrtc: WebRtcRuntimeConfig,
    /// Optional basic-auth protection for the private read-only admin surface.
    pub admin_auth: Option<AdminAuthConfig>,
}

impl Default for DevServerOptions {
    fn default() -> Self {
        Self {
            tick_interval: Duration::from_millis(u64::from(COMBAT_FRAME_MS)),
            simulation_step_ms: COMBAT_FRAME_MS,
            record_store_path: server::default_record_store_path(),
            combat_log_path: server::default_combat_log_path(),
            content_root: server::default_content_root(),
            web_client_root: server::default_web_client_root(),
            observability: Some(ServerObservability::new(env!("CARGO_PKG_VERSION"))),
            webrtc: WebRtcRuntimeConfig::default(),
            admin_auth: None,
        }
    }
}

#[derive(Default)]
struct BindingState {
    guard: NetworkSessionGuard,
    bound_player: Option<PlayerId>,
    disconnected: bool,
}

#[derive(Default)]
struct WebRtcNegotiationState {
    offer_seen: bool,
    answer_sent: bool,
    pending_local_candidates: Vec<SignalingIceCandidate>,
}

#[derive(Debug)]
struct SessionBootstrapTokenRegistry {
    issued: BTreeMap<String, Instant>,
    ttl: Duration,
}

impl SessionBootstrapTokenRegistry {
    fn new(ttl: Duration) -> Self {
        Self {
            issued: BTreeMap::new(),
            ttl,
        }
    }

    fn mint(&mut self, now: Instant) -> Result<String, String> {
        self.prune_expired(now);

        let mut bytes = [0_u8; SESSION_BOOTSTRAP_TOKEN_BYTES];
        getrandom_fill(&mut bytes)
            .map_err(|error| format!("failed to generate a session bootstrap token: {error}"))?;
        let token = BASE64_URL_SAFE_NO_PAD.encode(bytes);
        self.issued.insert(token.clone(), now + self.ttl);
        Ok(token)
    }

    fn consume(&mut self, token: &str, now: Instant) -> Result<(), &'static str> {
        self.prune_expired(now);

        if !is_valid_session_bootstrap_token_shape(token) {
            return Err("session bootstrap token format is invalid");
        }

        let Some(expires_at) = self.issued.remove(token) else {
            return Err("session bootstrap token is missing or already consumed");
        };
        if now > expires_at {
            return Err("session bootstrap token has expired");
        }

        Ok(())
    }

    fn prune_expired(&mut self, now: Instant) {
        self.issued.retain(|_, expires_at| *expires_at >= now);
    }
}

#[derive(Debug, Default, Deserialize)]
struct SessionBootstrapQuery {
    token: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SessionBootstrapResponse {
    token: String,
    expires_in_ms: u64,
}

fn is_valid_session_bootstrap_token_shape(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= MAX_SESSION_BOOTSTRAP_TOKEN_BYTES
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

struct SignalingTransport {
    peer: Arc<RTCPeerConnection>,
    binding_state: Arc<Mutex<BindingState>>,
    negotiation_state: Arc<Mutex<WebRtcNegotiationState>>,
    outbound: ClientOutbound,
    ice_servers: Vec<crate::WebRtcIceServerConfig>,
}

mod server;
mod sessions;
mod signaling;

pub use server::{spawn_dev_server, spawn_dev_server_with_options};

#[cfg(test)]
mod tests;
