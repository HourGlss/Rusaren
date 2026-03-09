use std::collections::{BTreeMap, VecDeque};
use std::io;
use std::net::SocketAddr;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{header, Request, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, get_service};
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use game_content::GameContent;
use game_domain::PlayerId;
use game_net::{NetworkSessionGuard, ServerControlEvent};
use game_sim::COMBAT_FRAME_MS;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::task::JoinHandle;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing::{debug, error, info, info_span, warn};

use crate::observability::{classify_http_path, ServerObservability};
use crate::{AppTransport, ConnectionId, ServerApp};

#[derive(Clone)]
struct DevServerState {
    ingress_tx: mpsc::UnboundedSender<IngressEvent>,
    web_client_root: PathBuf,
    observability: Option<ServerObservability>,
    next_connection_id: Arc<AtomicU64>,
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

struct RealtimeTransport {
    incoming: VecDeque<(ConnectionId, Vec<u8>)>,
    outgoing: BTreeMap<ConnectionId, mpsc::UnboundedSender<Vec<u8>>>,
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
        outbound: mpsc::UnboundedSender<Vec<u8>>,
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
            let _ = outbound.send(packet);
        }
    }
}

enum IngressEvent {
    Connect {
        connection_id: ConnectionId,
        outbound: mpsc::UnboundedSender<Vec<u8>>,
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
        }
    }
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
        .route("/ws", get(websocket_upgrade))
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
                    send_direct_error(&outbound, message);
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
                    "realtime ingress disconnected websocket session"
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

async fn websocket_upgrade(
    ws: WebSocketUpgrade,
    State(state): State<DevServerState>,
) -> impl IntoResponse {
    if let Some(observability) = &state.observability {
        observability.record_http_request(crate::HttpRouteLabel::WebSocket);
        observability.record_websocket_upgrade_attempt();
    }
    ws.max_message_size(game_net::MAX_INGRESS_PACKET_BYTES)
        .max_frame_size(game_net::MAX_INGRESS_PACKET_BYTES)
        .on_upgrade(move |socket| handle_socket(state, socket))
}

async fn handle_socket(state: DevServerState, socket: WebSocket) {
    let (mut sender, mut receiver) = socket.split();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<Vec<u8>>();
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
    info!("websocket session opened");

    while let Some(message_result) = receiver.next().await {
        let Ok(message) = message_result else {
            warn!("websocket stream ended with an error");
            break;
        };

        match message {
            Message::Binary(bytes) => {
                let keep_open = handle_binary_message(
                    &state,
                    connection_id,
                    &outbound_tx,
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
                reject_socket(
                    &outbound_tx,
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
            "websocket session disconnected after binding"
        );
    }

    drop(outbound_tx);
    let _ = writer.await;
    info!("websocket session closed");
}

async fn handle_binary_message(
    state: &DevServerState,
    connection_id: ConnectionId,
    outbound_tx: &mpsc::UnboundedSender<Vec<u8>>,
    guard: &mut NetworkSessionGuard,
    bound_player: &mut Option<PlayerId>,
    packet: Vec<u8>,
) -> bool {
    if let Err(error) = guard.accept_packet(&packet) {
        reject_socket(
            outbound_tx,
            state.observability.as_ref(),
            &error.to_string(),
        );
        return false;
    }

    if bound_player.is_none() {
        return bind_initial_player(
            state,
            connection_id,
            outbound_tx,
            guard,
            bound_player,
            packet,
        )
        .await;
    }

    forward_bound_packet(state, connection_id, packet)
}

async fn bind_initial_player(
    state: &DevServerState,
    connection_id: ConnectionId,
    outbound_tx: &mpsc::UnboundedSender<Vec<u8>>,
    guard: &mut NetworkSessionGuard,
    bound_player: &mut Option<PlayerId>,
    packet: Vec<u8>,
) -> bool {
    let (ack_tx, ack_rx) = oneshot::channel();
    if state
        .ingress_tx
        .send(IngressEvent::Connect {
            connection_id,
            outbound: outbound_tx.clone(),
            packet,
            ack: ack_tx,
        })
        .is_err()
    {
        reject_socket(
            outbound_tx,
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
                "websocket session bound to player"
            );
            *bound_player = Some(player_id);
            true
        }
        Ok(Err(message)) => {
            reject_socket(outbound_tx, state.observability.as_ref(), &message);
            false
        }
        Err(_) => {
            reject_socket(
                outbound_tx,
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

fn allocate_connection_id(next_connection_id: &AtomicU64) -> ConnectionId {
    let raw = next_connection_id.fetch_add(1, Ordering::Relaxed);
    match ConnectionId::new(raw) {
        Ok(connection_id) => connection_id,
        Err(error) => panic!("generated connection id should be valid: {error}"),
    }
}

fn send_direct_error(outbound: &mpsc::UnboundedSender<Vec<u8>>, message: &str) {
    if let Ok(packet) = (ServerControlEvent::Error {
        message: message.to_string(),
    })
    .encode_packet(0, 0)
    {
        let _ = outbound.send(packet);
    }
}

fn reject_socket(
    outbound: &mpsc::UnboundedSender<Vec<u8>>,
    observability: Option<&ServerObservability>,
    message: &str,
) {
    if let Some(observability) = observability {
        observability.record_websocket_rejection();
        observability.record_ingress_packet(false);
    }
    warn!(%message, "rejecting websocket session");
    send_direct_error(outbound, message);
}
