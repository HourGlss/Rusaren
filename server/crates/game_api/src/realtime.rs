use std::collections::{BTreeMap, VecDeque};
use std::io;
use std::net::SocketAddr;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, get_service};
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use game_domain::PlayerId;
use game_net::{NetworkSessionGuard, ServerControlEvent};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::task::JoinHandle;
use tower_http::services::ServeDir;

use crate::{AppTransport, ServerApp};

#[derive(Clone)]
struct DevServerState {
    ingress_tx: mpsc::UnboundedSender<IngressEvent>,
    web_client_root: PathBuf,
}

struct RuntimeState {
    app: ServerApp,
    transport: RealtimeTransport,
}

impl RuntimeState {
    fn pump_transport(&mut self) {
        let Self { app, transport } = self;
        app.pump_transport(transport);
    }

    fn advance_second(&mut self) {
        let Self { app, transport } = self;
        app.advance_seconds(transport, 1);
    }

    fn disconnect_player(&mut self, player_id: PlayerId) {
        let Self { app, transport } = self;
        let _ = app.disconnect_player(transport, player_id);
    }
}

struct RealtimeTransport {
    incoming: VecDeque<(PlayerId, Vec<u8>)>,
    outgoing: BTreeMap<PlayerId, mpsc::UnboundedSender<Vec<u8>>>,
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
        player_id: PlayerId,
        outbound: mpsc::UnboundedSender<Vec<u8>>,
    ) -> Result<(), &'static str> {
        if self.outgoing.contains_key(&player_id) {
            return Err("player is already connected");
        }

        self.outgoing.insert(player_id, outbound);
        Ok(())
    }

    fn unregister_client(&mut self, player_id: PlayerId) {
        self.outgoing.remove(&player_id);
    }

    fn enqueue(&mut self, player_id: PlayerId, packet: Vec<u8>) {
        self.incoming.push_back((player_id, packet));
    }
}

impl AppTransport for RealtimeTransport {
    fn recv_from_client(&mut self) -> Option<(PlayerId, Vec<u8>)> {
        self.incoming.pop_front()
    }

    fn send_to_client(&mut self, player_id: PlayerId, packet: Vec<u8>) {
        if let Some(outbound) = self.outgoing.get(&player_id) {
            let _ = outbound.send(packet);
        }
    }
}

enum IngressEvent {
    Connect {
        player_id: PlayerId,
        outbound: mpsc::UnboundedSender<Vec<u8>>,
        packet: Vec<u8>,
        ack: oneshot::Sender<Result<(), String>>,
    },
    Packet {
        player_id: PlayerId,
        packet: Vec<u8>,
    },
    Disconnect {
        player_id: PlayerId,
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
    pub record_store_path: PathBuf,
    pub web_client_root: PathBuf,
}

impl Default for DevServerOptions {
    fn default() -> Self {
        Self {
            tick_interval: Duration::from_secs(1),
            record_store_path: default_record_store_path(),
            web_client_root: default_web_client_root(),
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

    let local_addr = listener.local_addr()?;
    let (ingress_tx, ingress_rx) = mpsc::unbounded_channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let runtime = Arc::new(Mutex::new(RuntimeState {
        app: ServerApp::new_persistent(options.record_store_path).map_err(io::Error::other)?,
        transport: RealtimeTransport::new(),
    }));
    let state = DevServerState {
        ingress_tx: ingress_tx.clone(),
        web_client_root: options.web_client_root.clone(),
    };

    let ingress_task = tokio::spawn(run_ingress_loop(runtime.clone(), ingress_rx));
    let tick_task = tokio::spawn(run_tick_loop(runtime.clone(), options.tick_interval));

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
        .route("/ws", get(websocket_upgrade))
        .fallback_service(static_assets)
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
                player_id,
                outbound,
                packet,
                ack,
            } => {
                if let Err(message) = runtime
                    .transport
                    .register_client(player_id, outbound.clone())
                {
                    send_direct_error(&outbound, message);
                    let _ = ack.send(Err(message.to_string()));
                    continue;
                }

                runtime.transport.enqueue(player_id, packet);
                runtime.pump_transport();
                let _ = ack.send(Ok(()));
            }
            IngressEvent::Packet { player_id, packet } => {
                runtime.transport.enqueue(player_id, packet);
                runtime.pump_transport();
            }
            IngressEvent::Disconnect { player_id } => {
                runtime.disconnect_player(player_id);
                runtime.transport.unregister_client(player_id);
            }
        }
    }
}

async fn run_tick_loop(runtime: Arc<Mutex<RuntimeState>>, tick_interval: Duration) {
    let mut interval = tokio::time::interval(tick_interval);
    loop {
        interval.tick().await;
        let mut runtime = runtime.lock().await;
        runtime.advance_second();
    }
}

async fn healthcheck() -> &'static str {
    "ok"
}

async fn web_client_index(State(state): State<DevServerState>) -> Response {
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
    ws.max_message_size(game_net::MAX_INGRESS_PACKET_BYTES)
        .max_frame_size(game_net::MAX_INGRESS_PACKET_BYTES)
        .on_upgrade(move |socket| handle_socket(state, socket))
}

async fn handle_socket(state: DevServerState, socket: WebSocket) {
    let (mut sender, mut receiver) = socket.split();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let writer = tokio::spawn(async move {
        while let Some(packet) = outbound_rx.recv().await {
            if sender.send(Message::Binary(packet.into())).await.is_err() {
                break;
            }
        }
    });

    let mut guard = NetworkSessionGuard::new();
    let mut bound_player = None;

    while let Some(message_result) = receiver.next().await {
        let Ok(message) = message_result else {
            break;
        };

        match message {
            Message::Binary(bytes) => {
                let packet = bytes.to_vec();
                let player_id = match guard.accept_packet(&packet) {
                    Ok(player_id) => player_id,
                    Err(error) => {
                        send_direct_error(&outbound_tx, &error.to_string());
                        break;
                    }
                };

                if bound_player.is_none() {
                    let (ack_tx, ack_rx) = oneshot::channel();
                    if state
                        .ingress_tx
                        .send(IngressEvent::Connect {
                            player_id,
                            outbound: outbound_tx.clone(),
                            packet,
                            ack: ack_tx,
                        })
                        .is_err()
                    {
                        send_direct_error(&outbound_tx, "server is shutting down");
                        break;
                    }

                    match ack_rx.await {
                        Ok(Ok(())) => {
                            bound_player = Some(player_id);
                        }
                        Ok(Err(message)) => {
                            send_direct_error(&outbound_tx, &message);
                            break;
                        }
                        Err(_) => {
                            send_direct_error(
                                &outbound_tx,
                                "server did not accept the connect request",
                            );
                            break;
                        }
                    }
                } else if state
                    .ingress_tx
                    .send(IngressEvent::Packet { player_id, packet })
                    .is_err()
                {
                    break;
                }
            }
            Message::Text(_) => {
                send_direct_error(&outbound_tx, "text websocket messages are not accepted");
                break;
            }
            Message::Close(_) => break,
            Message::Ping(_) | Message::Pong(_) => {}
        }
    }

    if let Some(player_id) = bound_player {
        let _ = state
            .ingress_tx
            .send(IngressEvent::Disconnect { player_id });
    }

    drop(outbound_tx);
    let _ = writer.await;
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
