use std::collections::{BTreeMap, VecDeque};
use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{header, Request, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, get_service};
use axum::Router;
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use game_content::GameContent;
use game_domain::PlayerId;
use game_net::{
    ChannelId, NetworkSessionGuard, PacketHeader, PacketKind, ServerControlEvent,
    MAX_INGRESS_PACKET_BYTES, PROTOCOL_VERSION,
};
use game_sim::COMBAT_FRAME_MS;
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

#[derive(Clone)]
struct DevServerState {
    ingress_tx: mpsc::UnboundedSender<IngressEvent>,
    web_client_root: PathBuf,
    observability: Option<ServerObservability>,
    next_connection_id: Arc<AtomicU64>,
    webrtc: WebRtcRuntimeConfig,
}

struct RuntimeState {
    app: ServerApp,
    transport: RealtimeTransport,
    observability: Option<ServerObservability>,
}

impl RuntimeState {
    fn pump_transport(&mut self) {
        let Self { app, transport, .. } = self;
        app.pump_transport(transport);
    }

    fn advance_millis(&mut self, delta_ms: u16) {
        let Self { app, transport, .. } = self;
        app.advance_millis(transport, delta_ms);
    }

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
    fn new() -> Self {
        Self {
            incoming: VecDeque::new(),
            outgoing: BTreeMap::new(),
        }
    }

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

    fn unregister_client(&mut self, connection_id: ConnectionId) {
        self.outgoing.remove(&connection_id);
    }

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

pub struct DevServerHandle {
    local_addr: SocketAddr,
    shutdown_tx: Option<oneshot::Sender<()>>,
    server_task: JoinHandle<()>,
    ingress_task: JoinHandle<()>,
    tick_task: JoinHandle<()>,
}

#[derive(Clone, Debug)]
pub struct DevServerOptions {
    pub tick_interval: Duration,
    pub simulation_step_ms: u16,
    pub record_store_path: PathBuf,
    pub content_root: PathBuf,
    pub web_client_root: PathBuf,
    pub observability: Option<ServerObservability>,
    pub webrtc: WebRtcRuntimeConfig,
}

impl Default for DevServerOptions {
    fn default() -> Self {
        Self {
            tick_interval: Duration::from_millis(u64::from(COMBAT_FRAME_MS)),
            simulation_step_ms: COMBAT_FRAME_MS,
            record_store_path: default_record_store_path(),
            content_root: default_content_root(),
            web_client_root: default_web_client_root(),
            observability: Some(ServerObservability::new(env!("CARGO_PKG_VERSION"))),
            webrtc: WebRtcRuntimeConfig::default(),
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

struct SignalingTransport {
    peer: Arc<RTCPeerConnection>,
    binding_state: Arc<Mutex<BindingState>>,
    negotiation_state: Arc<Mutex<WebRtcNegotiationState>>,
    outbound: ClientOutbound,
    ice_servers: Vec<crate::WebRtcIceServerConfig>,
}

impl DevServerHandle {
    #[must_use]
    pub const fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    pub async fn shutdown(mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }

        let _ = self.server_task.await;
        self.ingress_task.abort();
        self.tick_task.abort();
    }
}

pub async fn spawn_dev_server(listener: TcpListener) -> io::Result<DevServerHandle> {
    spawn_dev_server_with_options(listener, DevServerOptions::default()).await
}

pub async fn spawn_dev_server_with_options(
    listener: TcpListener,
    options: DevServerOptions,
) -> io::Result<DevServerHandle> {
    if options.tick_interval.is_zero() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "tick interval must be greater than zero",
        ));
    }
    if options.simulation_step_ms == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "simulation step must be greater than zero",
        ));
    }
    options
        .webrtc
        .validate()
        .map_err(|message| io::Error::new(io::ErrorKind::InvalidInput, message))?;

    let local_addr = listener.local_addr()?;
    let content = GameContent::load_from_root(&options.content_root).map_err(io::Error::other)?;
    let (ingress_tx, ingress_rx) = mpsc::unbounded_channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let next_connection_id = Arc::new(AtomicU64::new(1));
    let runtime = Arc::new(Mutex::new(RuntimeState {
        app: ServerApp::new_persistent_with_content(content, options.record_store_path)
            .map_err(io::Error::other)?,
        transport: RealtimeTransport::new(),
        observability: options.observability.clone(),
    }));
    let state = DevServerState {
        ingress_tx: ingress_tx.clone(),
        web_client_root: options.web_client_root.clone(),
        observability: options.observability,
        next_connection_id,
        webrtc: options.webrtc,
    };

    let ingress_task = tokio::spawn(run_ingress_loop(runtime.clone(), ingress_rx));
    let tick_task = tokio::spawn(run_tick_loop(
        runtime.clone(),
        options.tick_interval,
        options.simulation_step_ms,
    ));

    let app = build_router(state);

    let server_task = tokio::spawn(async move {
        let server = axum::serve(listener, app).with_graceful_shutdown(async {
            let _ = shutdown_rx.await;
        });
        let _ = server.await;
    });

    Ok(DevServerHandle {
        local_addr,
        shutdown_tx: Some(shutdown_tx),
        server_task,
        ingress_task,
        tick_task,
    })
}

fn default_record_store_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("var")
        .join("player_records.tsv")
}

fn default_content_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("content")
}

fn default_web_client_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("static")
        .join("webclient")
}

fn build_router(state: DevServerState) -> Router {
    let static_assets = get_service(ServeDir::new(state.web_client_root.clone()));

    Router::new()
        .route("/", get(web_client_index))
        .route("/healthz", get(healthcheck))
        .route("/metrics", get(metrics_export))
        .route("/ws", get(signaling_upgrade))
        .route("/ws-dev", get(websocket_dev_upgrade))
        .fallback_service(static_assets)
        .layer(
            TraceLayer::new_for_http().make_span_with(|request: &Request<_>| {
                let route = classify_http_path(request.uri().path());
                info_span!(
                    "http_request",
                    method = %request.method(),
                    route = route.as_str(),
                    path = %request.uri().path()
                )
            }),
        )
        .with_state(state)
}

async fn run_ingress_loop(
    runtime: Arc<Mutex<RuntimeState>>,
    mut ingress_rx: mpsc::UnboundedReceiver<IngressEvent>,
) {
    while let Some(event) = ingress_rx.recv().await {
        let mut runtime = runtime.lock().await;
        match event {
            IngressEvent::Connect {
                connection_id,
                outbound,
                packet,
                ack,
            } => {
                if let Err(message) = runtime
                    .transport
                    .register_client(connection_id, outbound.clone())
                {
                    if let Some(observability) = &runtime.observability {
                        observability.record_ingress_packet(false);
                    }
                    warn!(
                        connection_id = connection_id.get(),
                        %message,
                        "realtime ingress rejected duplicate connect"
                    );
                    outbound.send_error(message);
                    let _ = ack.send(Err(message.to_string()));
                    continue;
                }

                if let Some(observability) = &runtime.observability {
                    observability.record_ingress_packet(true);
                }
                debug!(
                    connection_id = connection_id.get(),
                    "realtime ingress accepted connect packet"
                );
                runtime.transport.enqueue(connection_id, packet);
                runtime.pump_transport();
                if let Some(player_id) = runtime.app.player_id_for_connection(connection_id) {
                    let _ = ack.send(Ok(player_id));
                } else {
                    runtime.transport.unregister_client(connection_id);
                    let _ = ack.send(Err(String::from(
                        "server did not bind the connection after connect",
                    )));
                }
            }
            IngressEvent::Packet {
                connection_id,
                packet,
            } => {
                if let Some(observability) = &runtime.observability {
                    observability.record_ingress_packet(true);
                }
                debug!(
                    connection_id = connection_id.get(),
                    "realtime ingress accepted packet"
                );
                runtime.transport.enqueue(connection_id, packet);
                runtime.pump_transport();
            }
            IngressEvent::Disconnect { connection_id } => {
                info!(
                    connection_id = connection_id.get(),
                    "realtime ingress disconnected bound session"
                );
                runtime.disconnect_connection(connection_id);
                runtime.transport.unregister_client(connection_id);
            }
        }
    }
}

async fn run_tick_loop(
    runtime: Arc<Mutex<RuntimeState>>,
    tick_interval: Duration,
    simulation_step_ms: u16,
) {
    let mut interval = tokio::time::interval(tick_interval);
    loop {
        interval.tick().await;
        let mut runtime = runtime.lock().await;
        let started_at = Instant::now();
        runtime.advance_millis(simulation_step_ms);
        let elapsed = started_at.elapsed();
        if let Some(observability) = &runtime.observability {
            observability.record_tick(elapsed);
        }
        if elapsed > tick_interval {
            warn!(
                tick_duration_ms = elapsed.as_secs_f64() * 1000.0,
                configured_tick_ms = tick_interval.as_secs_f64() * 1000.0,
                "simulation tick exceeded the configured interval"
            );
        }
    }
}

async fn healthcheck(State(state): State<DevServerState>) -> &'static str {
    if let Some(observability) = &state.observability {
        observability.record_http_request(crate::HttpRouteLabel::Healthz);
    }
    "ok"
}

async fn web_client_index(State(state): State<DevServerState>) -> Response {
    if let Some(observability) = &state.observability {
        observability.record_http_request(crate::HttpRouteLabel::Root);
    }
    let index_path = state.web_client_root.join("index.html");
    match tokio::fs::read_to_string(&index_path).await {
        Ok(contents) => Html(contents).into_response(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => (
            StatusCode::SERVICE_UNAVAILABLE,
            Html(render_missing_web_client_page(&state.web_client_root)),
        )
            .into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Html(format!(
                "<!doctype html><html><body><h1>Rusaren web client load failed</h1><p>{error}</p></body></html>"
            )),
        )
            .into_response(),
    }
}

async fn metrics_export(State(state): State<DevServerState>) -> Response {
    if let Some(observability) = &state.observability {
        observability.record_http_request(crate::HttpRouteLabel::Metrics);
        return (
            StatusCode::OK,
            [(
                header::CONTENT_TYPE,
                "text/plain; version=0.0.4; charset=utf-8",
            )],
            observability.render_prometheus(),
        )
            .into_response();
    }

    (
        StatusCode::SERVICE_UNAVAILABLE,
        Html(String::from(
            "<!doctype html><html><body><h1>Rusaren metrics are disabled</h1><p>Start the server with observability enabled to expose Prometheus metrics.</p></body></html>",
        )),
    )
        .into_response()
}

fn render_missing_web_client_page(web_client_root: &Path) -> String {
    format!(
        concat!(
            "<!doctype html><html><head><meta charset=\"utf-8\">",
            "<title>Rusaren web client not built</title></head><body>",
            "<h1>Rusaren web client is not built yet.</h1>",
            "<p>Build the Godot Web export into the server static root, then reload this page.</p>",
            "<p>Expected export root: <code>{}</code></p>",
            "<p>Suggested command: <code>powershell -NoProfile -ExecutionPolicy Bypass -File server/scripts/export-web-client.ps1</code></p>",
            "</body></html>"
        ),
        web_client_root.display()
    )
}

async fn signaling_upgrade(
    ws: WebSocketUpgrade,
    State(state): State<DevServerState>,
) -> impl IntoResponse {
    if let Some(observability) = &state.observability {
        observability.record_http_request(crate::HttpRouteLabel::WebSocket);
        observability.record_websocket_upgrade_attempt();
    }
    ws.max_message_size(MAX_SIGNAL_MESSAGE_BYTES)
        .max_frame_size(MAX_SIGNAL_MESSAGE_BYTES)
        .on_upgrade(move |socket| handle_signaling_socket(state, socket))
}

async fn websocket_dev_upgrade(
    ws: WebSocketUpgrade,
    State(state): State<DevServerState>,
) -> impl IntoResponse {
    if let Some(observability) = &state.observability {
        observability.record_http_request(crate::HttpRouteLabel::WebSocket);
        observability.record_websocket_upgrade_attempt();
    }
    ws.max_message_size(MAX_INGRESS_PACKET_BYTES)
        .max_frame_size(MAX_INGRESS_PACKET_BYTES)
        .on_upgrade(move |socket| handle_websocket_dev_socket(state, socket))
}

async fn handle_signaling_socket(state: DevServerState, socket: WebSocket) {
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

async fn handle_websocket_dev_socket(state: DevServerState, socket: WebSocket) {
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

async fn create_signaling_transport(
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

async fn accept_webrtc_offer(
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

async fn add_remote_ice_candidate(
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

fn install_webrtc_callbacks(
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

const fn control_channel_init() -> RTCDataChannelInit {
    RTCDataChannelInit {
        ordered: Some(true),
        max_packet_life_time: None,
        max_retransmits: None,
        protocol: None,
        negotiated: Some(CONTROL_DATA_CHANNEL_ID),
    }
}

const fn unreliable_channel_init(id: u16) -> RTCDataChannelInit {
    RTCDataChannelInit {
        ordered: Some(false),
        max_packet_life_time: None,
        max_retransmits: Some(0),
        protocol: None,
        negotiated: Some(id),
    }
}

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

async fn handle_binary_message(
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

async fn disconnect_bound_session(
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

fn allocate_connection_id(next_connection_id: &AtomicU64) -> ConnectionId {
    let raw = next_connection_id.fetch_add(1, Ordering::Relaxed);
    match ConnectionId::new(raw) {
        Ok(connection_id) => connection_id,
        Err(error) => panic!("generated connection id should be valid: {error}"),
    }
}

fn reject_client_session(
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

async fn reject_peer_session(
    outbound: &ClientOutbound,
    observability: Option<&ServerObservability>,
    peer: &Arc<RTCPeerConnection>,
    message: &str,
) {
    reject_client_session(outbound, observability, message);
    let _ = peer.close().await;
}
