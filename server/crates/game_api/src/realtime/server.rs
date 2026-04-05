use super::{
    classify_http_path, debug, get, get_service, header, info, info_span, io, mpsc, oneshot, warn,
    Arc, AtomicU64, DevServerHandle, DevServerOptions, DevServerState, Duration, GameContent,
    HeaderMap, Html, IngressEvent, Instant, IntoResponse, IpAddr, Json, Mutex, Path, PathBuf,
    Query, RealtimeTransport, Request, Response, Router, RuntimeState, ServeDir, ServerApp,
    SessionBootstrapQuery, SessionBootstrapResponse, SessionBootstrapTokenRegistry, SocketAddr,
    State, StatusCode, TcpListener, TraceLayer, WebSocketUpgrade, MAX_INGRESS_PACKET_BYTES,
    MAX_SIGNAL_MESSAGE_BYTES, SESSION_BOOTSTRAP_RATE_LIMIT_MAX_REQUESTS,
    SESSION_BOOTSTRAP_RATE_LIMIT_WINDOW, SESSION_BOOTSTRAP_TOKEN_TTL,
};
use axum::extract::ConnectInfo;
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
        bootstrap_rate_limiter: Arc::new(Mutex::new(super::SessionBootstrapRateLimiter::new(
            SESSION_BOOTSTRAP_RATE_LIMIT_WINDOW,
            SESSION_BOOTSTRAP_RATE_LIMIT_MAX_REQUESTS,
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
        let server = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async {
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

#[derive(Debug, Default, serde::Deserialize)]
struct AdminDashboardQuery {
    format: Option<String>,
    match_id: Option<u32>,
    match_limit: Option<usize>,
    event_limit: Option<usize>,
}

const DEFAULT_ADMIN_MATCH_LIMIT: usize = 8;
const DEFAULT_ADMIN_EVENT_LIMIT: usize = 48;
const MAX_ADMIN_MATCH_LIMIT: usize = 25;
const MAX_ADMIN_EVENT_LIMIT: usize = 200;

/// Serves a password-protected read-only admin dashboard for operators.
async fn admin_dashboard(
    State(state): State<DevServerState>,
    Query(query): Query<AdminDashboardQuery>,
    headers: HeaderMap,
) -> Response {
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
    let stats = collect_admin_dashboard_stats(&runtime, state.observability.as_ref());
    let app_diagnostics = runtime.app.diagnostics_snapshot();
    let match_limit = clamp_admin_limit(
        query.match_limit,
        DEFAULT_ADMIN_MATCH_LIMIT,
        MAX_ADMIN_MATCH_LIMIT,
    );
    let event_limit = clamp_admin_limit(
        query.event_limit,
        DEFAULT_ADMIN_EVENT_LIMIT,
        MAX_ADMIN_EVENT_LIMIT,
    );
    let recent_matches = match runtime.app.recent_combat_log_matches(match_limit) {
        Ok(matches) => matches,
        Err(error) => return admin_dashboard_failure_response(&error.to_string()),
    };
    let selected_match_log =
        match build_selected_match_log_view(&runtime, &recent_matches, query.match_id, event_limit)
        {
            Ok(view) => view,
            Err(error) => return admin_dashboard_failure_response(&error.to_string()),
        };
    let metrics = state
        .observability
        .as_ref()
        .map(crate::ServerObservability::render_prometheus);
    if matches!(query.format.as_deref(), Some("json")) {
        let response = AdminDiagnosticsResponse {
            generated_unix_ms: std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .map(|duration: std::time::Duration| {
                    u64::try_from(duration.as_millis().min(u128::from(u64::MAX)))
                        .unwrap_or(u64::MAX)
                })
                .unwrap_or(0),
            runtime: stats.clone(),
            app_diagnostics: app_diagnostics.clone(),
            recent_matches,
            selected_match_log,
            prometheus_text: metrics.clone(),
        };
        return Json(response).into_response();
    }
    let body = render_admin_dashboard(
        &stats,
        &app_diagnostics,
        &recent_matches,
        selected_match_log.as_ref(),
        metrics.as_deref(),
    );
    Html(body).into_response()
}

/// Issues a short-lived one-time token required by websocket signaling upgrades.
async fn session_bootstrap(
    State(state): State<DevServerState>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Json<SessionBootstrapResponse>, Response> {
    if let Some(observability) = &state.observability {
        observability.record_http_request(crate::HttpRouteLabel::SessionBootstrap);
    }

    let client_ip = effective_client_ip(&headers, peer_addr.ip());
    let now = Instant::now();
    if let Err(retry_after) = state
        .bootstrap_rate_limiter
        .lock()
        .await
        .check_and_record(client_ip, now)
    {
        if let Some(observability) = &state.observability {
            observability.record_diagnostic(
                "session_bootstrap",
                None,
                None,
                format!(
                    "rate limited bootstrap request from {client_ip}; retry after {}ms",
                    retry_after.as_millis()
                ),
            );
        }
        let retry_after_seconds = retry_after.as_secs().max(1).to_string();
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, retry_after_seconds)],
            Json(serde_json::json!({
                "error": "session bootstrap is temporarily rate limited",
                "retry_after_ms": u64::try_from(retry_after.as_millis()).unwrap_or(u64::MAX),
            })),
        )
            .into_response());
    }

    let token = {
        let mut tokens = state.bootstrap_tokens.lock().await;
        match tokens.mint(now) {
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

fn effective_client_ip(headers: &HeaderMap, peer_ip: IpAddr) -> IpAddr {
    for header_name in ["x-forwarded-for", "x-real-ip"] {
        let Some(raw_value) = headers.get(header_name) else {
            continue;
        };
        let Ok(raw_value) = raw_value.to_str() else {
            continue;
        };
        for candidate in raw_value.split(',') {
            if let Ok(parsed) = candidate.trim().parse::<IpAddr>() {
                return parsed;
            }
        }
    }
    peer_ip
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
            "<p>Suggested command: <code>python3 -m rusaren_ops export-web-client</code></p>",
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

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
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
    recent_errors: Vec<crate::observability::RecentDiagnosticEvent>,
    recent_diagnostics: Vec<crate::observability::RecentDiagnosticEvent>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
struct AdminMatchLogView {
    summary: crate::CombatLogMatchSummary,
    entries: Vec<crate::CombatLogEntry>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
struct AdminDiagnosticsResponse {
    generated_unix_ms: u64,
    runtime: AdminDashboardStats,
    app_diagnostics: crate::app::ServerAppDiagnosticsSnapshot,
    recent_matches: Vec<crate::CombatLogMatchSummary>,
    selected_match_log: Option<AdminMatchLogView>,
    prometheus_text: Option<String>,
}

fn collect_admin_dashboard_stats(
    runtime: &RuntimeState,
    observability: Option<&crate::ServerObservability>,
) -> AdminDashboardStats {
    let uptime = observability
        .map(crate::ServerObservability::uptime)
        .unwrap_or_default();
    let recent_diagnostics = observability
        .map(crate::ServerObservability::recent_diagnostics)
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
        recent_errors: collect_recent_error_events(&recent_diagnostics),
        recent_diagnostics,
    }
}

fn clamp_admin_limit(value: Option<usize>, default_value: usize, max_value: usize) -> usize {
    value.unwrap_or(default_value).clamp(1, max_value)
}

fn collect_recent_error_events(
    recent_diagnostics: &[crate::observability::RecentDiagnosticEvent],
) -> Vec<crate::observability::RecentDiagnosticEvent> {
    recent_diagnostics
        .iter()
        .filter(|event| diagnostic_is_error(event))
        .cloned()
        .collect()
}

fn diagnostic_is_error(event: &crate::observability::RecentDiagnosticEvent) -> bool {
    matches!(
        event.category,
        "session_reject" | "websocket_upgrade" | "webrtc" | "signaling"
    ) || {
        let detail = event.detail.to_ascii_lowercase();
        [
            "error",
            "failed",
            "reject",
            "unauthorized",
            "timeout",
            "invalid",
            "denied",
        ]
        .iter()
        .any(|needle| detail.contains(needle))
    }
}

fn build_selected_match_log_view(
    runtime: &RuntimeState,
    recent_matches: &[crate::CombatLogMatchSummary],
    requested_match_id: Option<u32>,
    event_limit: usize,
) -> Result<Option<AdminMatchLogView>, crate::CombatLogStoreError> {
    let selected_summary = requested_match_id
        .and_then(|match_id| {
            recent_matches
                .iter()
                .find(|summary| summary.match_id == match_id)
        })
        .or_else(|| recent_matches.first());
    let Some(summary) = selected_summary.cloned() else {
        return Ok(None);
    };
    let Ok(match_id) = game_domain::MatchId::new(summary.match_id) else {
        return Ok(None);
    };
    let entries = runtime
        .app
        .combat_log_entries_limit(match_id, event_limit)?;
    Ok(Some(AdminMatchLogView { summary, entries }))
}

fn admin_dashboard_failure_response(message: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Html(format!(
            "<!doctype html><html><body><h1>Rusaren admin dashboard failed</h1><p>{}</p></body></html>",
            html_escape(message)
        )),
    )
        .into_response()
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

fn render_recent_errors_block(
    recent_errors: &[crate::observability::RecentDiagnosticEvent],
) -> String {
    if recent_errors.is_empty() {
        return String::from("<h2>Recent Errors</h2><p>No recent errors captured.</p>");
    }

    let mut rows = String::new();
    for event in recent_errors.iter().rev() {
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
            "<h2>Recent Errors</h2><table>",
            "<tr><th>Elapsed s</th><th>Category</th><th>Connection</th><th>Player</th><th>Detail</th></tr>",
            "{}",
            "</table>"
        ),
        rows
    )
}

fn render_recent_match_logs_block(
    recent_matches: &[crate::CombatLogMatchSummary],
    selected_match_log: Option<&AdminMatchLogView>,
) -> String {
    if recent_matches.is_empty() {
        return String::from(
            "<h2>Recent Match Logs</h2><p>No durable combat-log matches captured yet.</p>",
        );
    }

    let selected_match_id = selected_match_log.map(|view| view.summary.match_id);
    let mut match_rows = String::new();
    for summary in recent_matches {
        let selected_label = if Some(summary.match_id) == selected_match_id {
            " (selected)"
        } else {
            ""
        };
        let _ = write!(
            match_rows,
            concat!(
                "<tr><td><a href=\"/adminz?match_id={match_id}\">{match_id}</a>{selected}</td>",
                "<td>{event_count}</td><td>{last_round}</td><td>{last_phase}</td><td>{last_frame}</td><td>{last_kind}</td></tr>"
            ),
            match_id = summary.match_id,
            selected = selected_label,
            event_count = summary.event_count,
            last_round = summary.last_round,
            last_phase = html_escape(phase_label_for_dashboard(summary.last_phase)),
            last_frame = summary.last_frame_index,
            last_kind = html_escape(&summary.last_event_kind),
        );
    }

    let selected_block = selected_match_log.map_or_else(
        || String::from("<p>No recent match log selected.</p>"),
        render_selected_match_log_block,
    );

    format!(
        concat!(
            "<h2>Recent Match Logs</h2>",
            "<table><tr><th>Match</th><th>Events</th><th>Last round</th><th>Last phase</th><th>Last frame</th><th>Last event</th></tr>{}</table>",
            "{}"
        ),
        match_rows,
        selected_block,
    )
}

fn render_selected_match_log_block(match_log: &AdminMatchLogView) -> String {
    let mut rows = String::new();
    for entry in &match_log.entries {
        let event_json = serde_json::to_string(&entry.event).unwrap_or_else(|_| {
            String::from("{\"error\":\"failed to serialize combat log event\"}")
        });
        let _ = write!(
            rows,
            concat!(
                "<tr><td>{sequence}</td><td>{round}</td><td>{phase}</td><td>{frame}</td><td>{kind}</td><td><code>{payload}</code></td></tr>"
            ),
            sequence = entry.sequence.unwrap_or_default(),
            round = entry.round,
            phase = html_escape(phase_label_for_dashboard(entry.phase)),
            frame = entry.frame_index,
            kind = html_escape(entry.event.kind()),
            payload = html_escape(&event_json),
        );
    }

    format!(
        concat!(
            "<h3>Match {}</h3>",
            "<p>Showing {} most recent durable combat events for round {} through phase {}.</p>",
            "<table><tr><th>Seq</th><th>Round</th><th>Phase</th><th>Frame</th><th>Kind</th><th>Payload</th></tr>{}</table>"
        ),
        match_log.summary.match_id,
        match_log.entries.len(),
        match_log.summary.last_round,
        html_escape(phase_label_for_dashboard(match_log.summary.last_phase)),
        rows,
    )
}

fn render_app_diagnostics_block(
    app_diagnostics: &crate::app::ServerAppDiagnosticsSnapshot,
) -> String {
    format!(
        concat!(
            "<h2>App Diagnostics</h2><table>",
            "<tr><th>Control events sent</th><td>{}</td></tr>",
            "<tr><th>Full snapshots sent</th><td>{}</td></tr>",
            "<tr><th>Delta snapshots sent</th><td>{}</td></tr>",
            "<tr><th>Effect batches sent</th><td>{}</td></tr>",
            "<tr><th>Combat text batches sent</th><td>{}</td></tr>",
            "<tr><th>Peak player count</th><td>{}</td></tr>",
            "<tr><th>Peak projectile count</th><td>{}</td></tr>",
            "<tr><th>Peak deployable count</th><td>{}</td></tr>",
            "<tr><th>Peak visible tile count</th><td>{}</td></tr>",
            "</table>",
            "<h2>Combat Log Store</h2><table>",
            "<tr><th>Path</th><td>{}</td></tr>",
            "<tr><th>File bytes</th><td>{}</td></tr>",
            "<tr><th>Event count</th><td>{}</td></tr>",
            "<tr><th>Append p95 ms</th><td>{:.3}</td></tr>",
            "<tr><th>Append p99 ms</th><td>{:.3}</td></tr>",
            "<tr><th>Query p95 ms</th><td>{:.3}</td></tr>",
            "<tr><th>Query p99 ms</th><td>{:.3}</td></tr>",
            "</table>"
        ),
        app_diagnostics.app.control_events.sent_packets,
        app_diagnostics.app.full_snapshots.sent_packets,
        app_diagnostics.app.delta_snapshots.sent_packets,
        app_diagnostics.app.effect_batches.sent_packets,
        app_diagnostics.app.combat_text_batches.sent_packets,
        app_diagnostics.app.peak_player_count,
        app_diagnostics.app.peak_projectile_count,
        app_diagnostics.app.peak_deployable_count,
        app_diagnostics.app.peak_visible_tile_count,
        html_escape(
            app_diagnostics
                .combat_log
                .path
                .as_deref()
                .unwrap_or("in-memory"),
        ),
        app_diagnostics
            .combat_log
            .file_bytes
            .map_or_else(|| String::from("-"), |value| value.to_string()),
        app_diagnostics.combat_log.event_count,
        app_diagnostics.combat_log.append.p95_ms,
        app_diagnostics.combat_log.append.p99_ms,
        app_diagnostics.combat_log.query.p95_ms,
        app_diagnostics.combat_log.query.p99_ms,
    )
}

fn phase_label_for_dashboard(phase: crate::CombatLogPhase) -> &'static str {
    match phase {
        crate::CombatLogPhase::SkillPick => "skill_pick",
        crate::CombatLogPhase::PreCombat => "pre_combat",
        crate::CombatLogPhase::Combat => "combat",
        crate::CombatLogPhase::MatchEnd => "match_end",
    }
}

fn render_admin_dashboard(
    stats: &AdminDashboardStats,
    app_diagnostics: &crate::app::ServerAppDiagnosticsSnapshot,
    recent_matches: &[crate::CombatLogMatchSummary],
    selected_match_log: Option<&AdminMatchLogView>,
    metrics: Option<&str>,
) -> String {
    let metrics_block = render_metrics_block(metrics);
    let errors_block = render_recent_errors_block(&stats.recent_errors);
    let match_logs_block = render_recent_match_logs_block(recent_matches, selected_match_log);
    let app_diagnostics_block = render_app_diagnostics_block(app_diagnostics);
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
            "</table>{}{}{}{}{}</body></html>"
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
        app_diagnostics_block,
        errors_block,
        match_logs_block,
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
