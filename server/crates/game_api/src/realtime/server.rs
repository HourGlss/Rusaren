use super::{
    classify_http_path, debug, get, get_service, header, info, info_span, io, mpsc, oneshot, warn,
    Arc, AtomicU64, DevServerHandle, DevServerOptions, DevServerState, Duration, GameContent,
    HeaderMap, Html, IngressEvent, Instant, IntoResponse, Json, Mutex, Path, PathBuf, Query,
    RealtimeTransport, Request, Response, Router, RuntimeState, ServeDir, ServerApp,
    SessionBootstrapQuery, SessionBootstrapResponse, SessionBootstrapTokenRegistry, SocketAddr,
    State, StatusCode, TcpListener, TraceLayer, WebSocketUpgrade, MAX_INGRESS_PACKET_BYTES,
    MAX_SIGNAL_MESSAGE_BYTES, SESSION_BOOTSTRAP_TOKEN_TTL,
};
use std::fmt::Write as _;

use super::signaling::{handle_signaling_socket, handle_websocket_dev_socket};

impl DevServerHandle {
    /// Returns the socket address the server bound to.
    #[must_use]
    pub const fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Shuts the server down and waits for its owned tasks to exit.
    pub async fn shutdown(mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }

        let _ = self.server_task.await;
        self.ingress_task.abort();
        self.tick_task.abort();
    }
}

/// Spawns the realtime dev server with default options.
pub async fn spawn_dev_server(listener: TcpListener) -> io::Result<DevServerHandle> {
    spawn_dev_server_with_options(listener, DevServerOptions::default()).await
}

/// Spawns the realtime dev server with explicit runtime options.
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
        app: ServerApp::new_persistent_with_content_and_log(
            content,
            options.record_store_path,
            options.combat_log_path,
        )
        .map_err(io::Error::other)?,
        transport: RealtimeTransport::new(),
        observability: options.observability.clone(),
    }));
    let state = DevServerState {
        runtime: runtime.clone(),
        ingress_tx: ingress_tx.clone(),
        web_client_root: options.web_client_root.clone(),
        observability: options.observability,
        next_connection_id,
        bootstrap_tokens: Arc::new(Mutex::new(SessionBootstrapTokenRegistry::new(
            SESSION_BOOTSTRAP_TOKEN_TTL,
        ))),
        webrtc: options.webrtc,
        admin_auth: options.admin_auth,
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

/// Returns the default persistent record-store path used for local runs.
pub(super) fn default_record_store_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("var")
        .join("player_records.tsv")
}

/// Returns the default persistent combat-log path used for local runs.
pub(super) fn default_combat_log_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("var")
        .join("combat_events.sqlite")
}

/// Returns the default runtime content root used for local runs.
pub(super) fn default_content_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("content")
}

/// Returns the default exported web-client root served at `/`.
pub(super) fn default_web_client_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("static")
        .join("webclient")
}

/// Builds the HTTP router for health, metrics, static assets, and realtime transports.
fn build_router(state: DevServerState) -> Router {
    let static_assets = get_service(ServeDir::new(state.web_client_root.clone()));

    Router::new()
        .route("/", get(web_client_index))
        .route("/healthz", get(healthcheck))
        .route("/metrics", get(metrics_export))
        .route("/adminz", get(admin_dashboard))
        .route("/session/bootstrap", get(session_bootstrap))
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

/// Pumps transport ingress events into the application core.
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

/// Advances the simulation clock on a fixed real-time interval.
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

/// Responds to the liveness probe.
async fn healthcheck(State(state): State<DevServerState>) -> &'static str {
    if let Some(observability) = &state.observability {
        observability.record_http_request(crate::HttpRouteLabel::Healthz);
    }
    "ok"
}

/// Serves the exported Godot web client root page.
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

/// Serves Prometheus metrics when observability is enabled.
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

/// Serves a password-protected read-only admin dashboard for operators.
async fn admin_dashboard(State(state): State<DevServerState>, headers: HeaderMap) -> Response {
    if let Some(observability) = &state.observability {
        observability.record_http_request(crate::HttpRouteLabel::Admin);
    }

    let Some(admin_auth) = &state.admin_auth else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !admin_auth.is_authorized(&headers) {
        return unauthorized_admin_response();
    }

    let runtime = state.runtime.lock().await;
    let metrics = state
        .observability
        .as_ref()
        .map(crate::ServerObservability::render_prometheus);
    let body = render_admin_dashboard(&runtime, state.observability.as_ref(), metrics.as_deref());
    Html(body).into_response()
}

/// Issues a short-lived one-time token required by websocket signaling upgrades.
async fn session_bootstrap(
    State(state): State<DevServerState>,
) -> Result<Json<SessionBootstrapResponse>, Response> {
    if let Some(observability) = &state.observability {
        observability.record_http_request(crate::HttpRouteLabel::SessionBootstrap);
    }

    let token = {
        let mut tokens = state.bootstrap_tokens.lock().await;
        match tokens.mint(Instant::now()) {
            Ok(token) => token,
            Err(message) => {
                if let Some(observability) = &state.observability {
                    observability.record_diagnostic(
                        "session_bootstrap",
                        None,
                        None,
                        format!("failed to mint bootstrap token: {message}"),
                    );
                }
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": message })),
                )
                    .into_response());
            }
        }
    };

    if let Some(observability) = &state.observability {
        observability.record_diagnostic(
            "session_bootstrap",
            None,
            None,
            "issued one-time websocket bootstrap token",
        );
    }

    Ok(Json(SessionBootstrapResponse {
        token,
        expires_in_ms: u64::try_from(SESSION_BOOTSTRAP_TOKEN_TTL.as_millis()).unwrap_or(u64::MAX),
    }))
}

/// Renders a small HTML page that explains how to build the missing web client.
fn render_missing_web_client_page(web_client_root: &Path) -> String {
    format!(
        concat!(
            "<!doctype html><html><head><meta charset=\"utf-8\">",
            "<title>Rusaren web client not built</title></head><body>",
            "<h1>Rusaren web client is not built yet.</h1>",
            "<p>Build the Godot Web export into the server static root, then reload this page.</p>",
            "<p>Expected export root: <code>{}</code></p>",
            "<p>Suggested command: <code>bash server/scripts/export-web-client.sh</code></p>",
            "</body></html>"
        ),
        web_client_root.display()
    )
}

fn unauthorized_admin_response() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Basic realm=\"Rusaren Admin\"")],
        Html(String::from(
            "<!doctype html><html><body><h1>Rusaren admin authentication required</h1></body></html>",
        )),
    )
        .into_response()
}

struct AdminDashboardStats {
    connected_players: usize,
    bound_connections: usize,
    central_lobby_players: usize,
    active_lobbies: usize,
    active_matches: usize,
    active_sessions: usize,
    uptime_seconds: f64,
    tick_iterations: u64,
    tick_last_ms: f64,
    tick_max_ms: f64,
    ingress_accepted: u64,
    ingress_rejected: u64,
    websocket_bound: u64,
    websocket_disconnects: u64,
    websocket_rejections: u64,
    websocket_active: u64,
    recent_diagnostics: Vec<crate::observability::RecentDiagnosticEvent>,
}

fn collect_admin_dashboard_stats(
    runtime: &RuntimeState,
    observability: Option<&crate::ServerObservability>,
) -> AdminDashboardStats {
    let uptime = observability
        .map(crate::ServerObservability::uptime)
        .unwrap_or_default();
    AdminDashboardStats {
        connected_players: runtime.app.connected_player_count(),
        bound_connections: runtime.app.bound_connection_count(),
        central_lobby_players: runtime.app.central_lobby_player_count(),
        active_lobbies: runtime.app.active_lobby_count(),
        active_matches: runtime.app.active_match_count(),
        active_sessions: runtime.transport.outgoing.len(),
        uptime_seconds: uptime.as_secs_f64(),
        tick_iterations: observability.map_or(0, crate::ServerObservability::tick_iterations),
        tick_last_ms: observability
            .map(crate::ServerObservability::tick_duration_last)
            .unwrap_or_default()
            .as_secs_f64()
            * 1000.0,
        tick_max_ms: observability
            .map(crate::ServerObservability::tick_duration_max)
            .unwrap_or_default()
            .as_secs_f64()
            * 1000.0,
        ingress_accepted: observability.map_or(
            0,
            crate::ServerObservability::ingress_packets_accepted_total,
        ),
        ingress_rejected: observability.map_or(
            0,
            crate::ServerObservability::ingress_packets_rejected_total,
        ),
        websocket_bound: observability.map_or(
            0,
            crate::ServerObservability::websocket_sessions_bound_total,
        ),
        websocket_disconnects: observability
            .map_or(0, crate::ServerObservability::websocket_disconnects_total),
        websocket_rejections: observability
            .map_or(0, crate::ServerObservability::websocket_rejections_total),
        websocket_active: observability
            .map_or(0, crate::ServerObservability::websocket_sessions_active),
        recent_diagnostics: observability
            .map(crate::ServerObservability::recent_diagnostics)
            .unwrap_or_default(),
    }
}

fn render_metrics_block(metrics: Option<&str>) -> String {
    metrics.map_or_else(
        || String::from("<p>Observability is disabled.</p>"),
        |metrics| {
            format!(
                "<details><summary>Prometheus Snapshot</summary><pre>{}</pre></details>",
                html_escape(metrics)
            )
        },
    )
}

fn format_elapsed_seconds(elapsed_ms: u64) -> String {
    format!("{}.{:03}", elapsed_ms / 1000, elapsed_ms % 1000)
}

fn render_recent_diagnostics_block(
    recent_diagnostics: &[crate::observability::RecentDiagnosticEvent],
) -> String {
    if recent_diagnostics.is_empty() {
        return String::from("<p>No recent diagnostics captured.</p>");
    }

    let mut rows = String::new();
    for event in recent_diagnostics.iter().rev() {
        let connection_text = event
            .connection_id
            .map_or_else(|| String::from("-"), |value| value.to_string());
        let player_text = event
            .player_id
            .map_or_else(|| String::from("-"), |value| value.to_string());
        let _ = write!(
            rows,
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            format_elapsed_seconds(event.elapsed_ms),
            html_escape(event.category),
            html_escape(&connection_text),
            html_escape(&player_text),
            html_escape(&event.detail),
        );
    }

    format!(
        concat!(
            "<h2>Recent Diagnostics</h2><table>",
            "<tr><th>Elapsed s</th><th>Category</th><th>Connection</th><th>Player</th><th>Detail</th></tr>",
            "{}",
            "</table>"
        ),
        rows
    )
}

fn render_admin_dashboard(
    runtime: &RuntimeState,
    observability: Option<&crate::ServerObservability>,
    metrics: Option<&str>,
) -> String {
    let stats = collect_admin_dashboard_stats(runtime, observability);
    let metrics_block = render_metrics_block(metrics);
    let diagnostics_block = render_recent_diagnostics_block(&stats.recent_diagnostics);

    format!(
        concat!(
            "<!doctype html><html><head><meta charset=\"utf-8\">",
            "<title>Rusaren Admin</title>",
            "<style>body{{font-family:Consolas,monospace;max-width:1100px;margin:2rem auto;padding:0 1rem;}}",
            "table{{border-collapse:collapse;width:100%;margin-bottom:1.5rem;}}",
            "th,td{{border:1px solid #bbb;padding:.5rem;text-align:left;vertical-align:top;}}",
            "h1,h2{{margin-bottom:.5rem;}}pre{{white-space:pre-wrap;word-break:break-word;}}</style>",
            "</head><body><h1>Rusaren Admin Dashboard</h1>",
            "<p>Read-only operator view for the live backend.</p>",
            "<h2>Runtime</h2><table>",
            "<tr><th>Connected players</th><td>{}</td></tr>",
            "<tr><th>Bound connections</th><td>{}</td></tr>",
            "<tr><th>Active realtime sessions</th><td>{}</td></tr>",
            "<tr><th>Central lobby players</th><td>{}</td></tr>",
            "<tr><th>Active lobbies</th><td>{}</td></tr>",
            "<tr><th>Active matches</th><td>{}</td></tr>",
            "</table>",
            "<h2>Tick Health</h2><table>",
            "<tr><th>Uptime seconds</th><td>{:.3}</td></tr>",
            "<tr><th>Tick iterations</th><td>{}</td></tr>",
            "<tr><th>Last tick ms</th><td>{:.3}</td></tr>",
            "<tr><th>Max tick ms</th><td>{:.3}</td></tr>",
            "</table>",
            "<h2>Transport</h2><table>",
            "<tr><th>Ingress accepted</th><td>{}</td></tr>",
            "<tr><th>Ingress rejected</th><td>{}</td></tr>",
            "<tr><th>Websocket sessions bound</th><td>{}</td></tr>",
            "<tr><th>Websocket sessions active</th><td>{}</td></tr>",
            "<tr><th>Websocket disconnects</th><td>{}</td></tr>",
            "<tr><th>Websocket rejections</th><td>{}</td></tr>",
            "</table>{}{}</body></html>"
        ),
        stats.connected_players,
        stats.bound_connections,
        stats.active_sessions,
        stats.central_lobby_players,
        stats.active_lobbies,
        stats.active_matches,
        stats.uptime_seconds,
        stats.tick_iterations,
        stats.tick_last_ms,
        stats.tick_max_ms,
        stats.ingress_accepted,
        stats.ingress_rejected,
        stats.websocket_bound,
        stats.websocket_active,
        stats.websocket_disconnects,
        stats.websocket_rejections,
        diagnostics_block,
        metrics_block,
    )
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Upgrades `/ws` into the websocket signaling channel used for `WebRTC` setup.
async fn signaling_upgrade(
    ws: WebSocketUpgrade,
    Query(query): Query<SessionBootstrapQuery>,
    State(state): State<DevServerState>,
) -> Response {
    if let Some(observability) = &state.observability {
        observability.record_http_request(crate::HttpRouteLabel::WebSocket);
    }

    if let Err(response) = authorize_websocket_upgrade(&state, query.token.as_deref()).await {
        return response;
    }

    if let Some(observability) = &state.observability {
        observability.record_websocket_upgrade_attempt();
    }
    ws.max_message_size(MAX_SIGNAL_MESSAGE_BYTES)
        .max_frame_size(MAX_SIGNAL_MESSAGE_BYTES)
        .on_upgrade(move |socket| handle_signaling_socket(state, socket))
        .into_response()
}

/// Upgrades `/ws-dev` into the legacy raw websocket gameplay adapter.
async fn websocket_dev_upgrade(
    ws: WebSocketUpgrade,
    Query(query): Query<SessionBootstrapQuery>,
    State(state): State<DevServerState>,
) -> Response {
    if let Some(observability) = &state.observability {
        observability.record_http_request(crate::HttpRouteLabel::WebSocket);
    }

    if let Err(response) = authorize_websocket_upgrade(&state, query.token.as_deref()).await {
        return response;
    }

    if let Some(observability) = &state.observability {
        observability.record_websocket_upgrade_attempt();
    }
    ws.max_message_size(MAX_INGRESS_PACKET_BYTES)
        .max_frame_size(MAX_INGRESS_PACKET_BYTES)
        .on_upgrade(move |socket| handle_websocket_dev_socket(state, socket))
        .into_response()
}

/// Validates and consumes the short-lived bootstrap token required for websocket upgrades.
///
/// VERIFIED MODEL: `server/verus/session_bootstrap_model.rs` mirrors the one-time-use
/// token invariant enforced here. The production registry still relies on runtime tests
/// for actual timing, storage, and HTTP upgrade behavior.
async fn authorize_websocket_upgrade(
    state: &DevServerState,
    token: Option<&str>,
) -> Result<(), Response> {
    let Some(token) = token else {
        if let Some(observability) = &state.observability {
            observability.record_websocket_rejection();
            observability.record_diagnostic(
                "websocket_upgrade",
                None,
                None,
                "rejected websocket upgrade because the bootstrap token was missing",
            );
        }
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "missing required session bootstrap token"
            })),
        )
            .into_response());
    };

    let consume_result = {
        let mut tokens = state.bootstrap_tokens.lock().await;
        tokens.consume(token, Instant::now())
    };
    if let Err(message) = consume_result {
        if let Some(observability) = &state.observability {
            observability.record_websocket_rejection();
            observability.record_diagnostic(
                "websocket_upgrade",
                None,
                None,
                format!("rejected websocket upgrade: {message}"),
            );
        }
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": message })),
        )
            .into_response());
    }

    if let Some(observability) = &state.observability {
        observability.record_diagnostic(
            "websocket_upgrade",
            None,
            None,
            "authorized websocket upgrade with a valid bootstrap token",
        );
    }

    Ok(())
}
