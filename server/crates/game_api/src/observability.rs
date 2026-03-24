use std::collections::VecDeque;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const MAX_RECENT_DIAGNOSTICS: usize = 128;

/// One recent low-volume diagnostic event captured for operator inspection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentDiagnosticEvent {
    /// Milliseconds elapsed since the server process started.
    pub elapsed_ms: u64,
    /// Stable category name for the event source.
    pub category: &'static str,
    /// Connection id when one transport session is involved.
    pub connection_id: Option<u64>,
    /// Player id when the transport was already bound.
    pub player_id: Option<u64>,
    /// Human-readable diagnostic detail.
    pub detail: String,
}

/// Low-cardinality labels for the HTTP surface exposed by the server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpRouteLabel {
    /// The root page that serves the exported Godot client.
    Root,
    /// The basic liveness probe.
    Healthz,
    /// The Prometheus metrics endpoint.
    Metrics,
    /// The password-protected operator dashboard.
    Admin,
    /// One-time bootstrap token minting for websocket signaling upgrades.
    SessionBootstrap,
    /// Either the signaling websocket or the websocket dev adapter.
    WebSocket,
    /// Any other static asset path served from the web client root.
    StaticAsset,
}

impl HttpRouteLabel {
    /// Returns the Prometheus-safe string form for the label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Root => "root",
            Self::Healthz => "healthz",
            Self::Metrics => "metrics",
            Self::Admin => "admin",
            Self::SessionBootstrap => "session_bootstrap",
            Self::WebSocket => "ws",
            Self::StaticAsset => "static_asset",
        }
    }
}

/// Maps an incoming HTTP request path into a low-cardinality metrics label.
#[must_use]
pub fn classify_http_path(path: &str) -> HttpRouteLabel {
    match path {
        "/" => HttpRouteLabel::Root,
        "/healthz" => HttpRouteLabel::Healthz,
        "/metrics" => HttpRouteLabel::Metrics,
        "/adminz" => HttpRouteLabel::Admin,
        "/session/bootstrap" => HttpRouteLabel::SessionBootstrap,
        "/ws" | "/ws-dev" => HttpRouteLabel::WebSocket,
        _ => HttpRouteLabel::StaticAsset,
    }
}

#[derive(Debug)]
struct ServerObservabilityInner {
    version: String,
    started_at: Instant,
    recent_diagnostics: Mutex<VecDeque<RecentDiagnosticEvent>>,
    http_root_requests: AtomicU64,
    http_healthz_requests: AtomicU64,
    http_metrics_requests: AtomicU64,
    http_admin_requests: AtomicU64,
    http_session_bootstrap_requests: AtomicU64,
    http_websocket_requests: AtomicU64,
    http_static_asset_requests: AtomicU64,
    websocket_upgrade_attempts: AtomicU64,
    websocket_sessions_bound: AtomicU64,
    websocket_sessions_active: AtomicU64,
    websocket_disconnects: AtomicU64,
    websocket_rejections: AtomicU64,
    ingress_packets_accepted: AtomicU64,
    ingress_packets_rejected: AtomicU64,
    tick_iterations: AtomicU64,
    tick_duration_last_micros: AtomicU64,
    tick_duration_max_micros: AtomicU64,
}

impl ServerObservabilityInner {
    fn new(version: String) -> Self {
        Self {
            version,
            started_at: Instant::now(),
            recent_diagnostics: Mutex::new(VecDeque::with_capacity(MAX_RECENT_DIAGNOSTICS)),
            http_root_requests: AtomicU64::new(0),
            http_healthz_requests: AtomicU64::new(0),
            http_metrics_requests: AtomicU64::new(0),
            http_admin_requests: AtomicU64::new(0),
            http_session_bootstrap_requests: AtomicU64::new(0),
            http_websocket_requests: AtomicU64::new(0),
            http_static_asset_requests: AtomicU64::new(0),
            websocket_upgrade_attempts: AtomicU64::new(0),
            websocket_sessions_bound: AtomicU64::new(0),
            websocket_sessions_active: AtomicU64::new(0),
            websocket_disconnects: AtomicU64::new(0),
            websocket_rejections: AtomicU64::new(0),
            ingress_packets_accepted: AtomicU64::new(0),
            ingress_packets_rejected: AtomicU64::new(0),
            tick_iterations: AtomicU64::new(0),
            tick_duration_last_micros: AtomicU64::new(0),
            tick_duration_max_micros: AtomicU64::new(0),
        }
    }
}

/// Thread-safe in-process metrics registry for the hosted server surface.
#[derive(Clone, Debug)]
pub struct ServerObservability {
    inner: Arc<ServerObservabilityInner>,
}

impl ServerObservability {
    /// Creates a new observability registry for one server process.
    #[must_use]
    pub fn new(version: impl Into<String>) -> Self {
        Self {
            inner: Arc::new(ServerObservabilityInner::new(version.into())),
        }
    }

    /// Records one HTTP request against the supplied route label.
    pub fn record_http_request(&self, route: HttpRouteLabel) {
        let counter = match route {
            HttpRouteLabel::Root => &self.inner.http_root_requests,
            HttpRouteLabel::Healthz => &self.inner.http_healthz_requests,
            HttpRouteLabel::Metrics => &self.inner.http_metrics_requests,
            HttpRouteLabel::Admin => &self.inner.http_admin_requests,
            HttpRouteLabel::SessionBootstrap => &self.inner.http_session_bootstrap_requests,
            HttpRouteLabel::WebSocket => &self.inner.http_websocket_requests,
            HttpRouteLabel::StaticAsset => &self.inner.http_static_asset_requests,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    /// Records an attempted websocket upgrade.
    pub fn record_websocket_upgrade_attempt(&self) {
        self.inner
            .websocket_upgrade_attempts
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Records a successfully bound realtime session.
    pub fn record_websocket_session_bound(&self) {
        self.inner
            .websocket_sessions_bound
            .fetch_add(1, Ordering::Relaxed);
        self.inner
            .websocket_sessions_active
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Records a websocket disconnect and decrements the active-session gauge.
    pub fn record_websocket_disconnect(&self) {
        self.inner
            .websocket_disconnects
            .fetch_add(1, Ordering::Relaxed);
        decrement_clamped(&self.inner.websocket_sessions_active);
    }

    /// Records a rejected realtime session or packet.
    pub fn record_websocket_rejection(&self) {
        self.inner
            .websocket_rejections
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Records whether an ingress packet was accepted or rejected.
    pub fn record_ingress_packet(&self, accepted: bool) {
        let counter = if accepted {
            &self.inner.ingress_packets_accepted
        } else {
            &self.inner.ingress_packets_rejected
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    /// Records the elapsed time for one simulation tick.
    pub fn record_tick(&self, duration: Duration) {
        let capped_micros = duration.as_micros().min(u128::from(u64::MAX));
        let micros = u64::try_from(capped_micros).unwrap_or(u64::MAX);
        self.inner.tick_iterations.fetch_add(1, Ordering::Relaxed);
        self.inner
            .tick_duration_last_micros
            .store(micros, Ordering::Relaxed);
        update_max(&self.inner.tick_duration_max_micros, micros);
    }

    /// Records one recent diagnostic event for the operator dashboard.
    pub fn record_diagnostic(
        &self,
        category: &'static str,
        connection_id: Option<u64>,
        player_id: Option<u64>,
        detail: impl Into<String>,
    ) {
        let elapsed_ms = u64::try_from(
            self.inner
                .started_at
                .elapsed()
                .as_millis()
                .min(u128::from(u64::MAX)),
        )
        .unwrap_or(u64::MAX);
        let event = RecentDiagnosticEvent {
            elapsed_ms,
            category,
            connection_id,
            player_id,
            detail: detail.into(),
        };

        let Ok(mut diagnostics) = self.inner.recent_diagnostics.lock() else {
            return;
        };
        diagnostics.push_back(event);
        while diagnostics.len() > MAX_RECENT_DIAGNOSTICS {
            diagnostics.pop_front();
        }
    }

    /// Renders the current metrics snapshot in Prometheus text format.
    #[must_use]
    pub fn render_prometheus(&self) -> String {
        let mut output = String::new();
        write_http_metrics(&mut output, &self.inner);
        write_websocket_metrics(&mut output, &self.inner);
        write_ingress_metrics(&mut output, &self.inner);
        write_tick_metrics(&mut output, &self.inner);
        write_uptime_metric(&mut output, self.inner.started_at.elapsed());
        write_build_info_metric(&mut output, &self.inner.version);
        output
    }

    /// Returns the process uptime tracked by the observability registry.
    #[must_use]
    pub fn uptime(&self) -> Duration {
        self.inner.started_at.elapsed()
    }

    /// Returns the active websocket-session gauge.
    #[must_use]
    pub fn websocket_sessions_active(&self) -> u64 {
        load_counter(&self.inner.websocket_sessions_active)
    }

    /// Returns the total number of bound websocket sessions.
    #[must_use]
    pub fn websocket_sessions_bound_total(&self) -> u64 {
        load_counter(&self.inner.websocket_sessions_bound)
    }

    /// Returns the total number of websocket disconnects.
    #[must_use]
    pub fn websocket_disconnects_total(&self) -> u64 {
        load_counter(&self.inner.websocket_disconnects)
    }

    /// Returns the total number of websocket rejections.
    #[must_use]
    pub fn websocket_rejections_total(&self) -> u64 {
        load_counter(&self.inner.websocket_rejections)
    }

    /// Returns accepted ingress packet count.
    #[must_use]
    pub fn ingress_packets_accepted_total(&self) -> u64 {
        load_counter(&self.inner.ingress_packets_accepted)
    }

    /// Returns rejected ingress packet count.
    #[must_use]
    pub fn ingress_packets_rejected_total(&self) -> u64 {
        load_counter(&self.inner.ingress_packets_rejected)
    }

    /// Returns the number of recorded simulation ticks.
    #[must_use]
    pub fn tick_iterations(&self) -> u64 {
        load_counter(&self.inner.tick_iterations)
    }

    /// Returns the most recently observed tick duration.
    #[must_use]
    pub fn tick_duration_last(&self) -> Duration {
        Duration::from_micros(load_counter(&self.inner.tick_duration_last_micros))
    }

    /// Returns the maximum observed tick duration.
    #[must_use]
    pub fn tick_duration_max(&self) -> Duration {
        Duration::from_micros(load_counter(&self.inner.tick_duration_max_micros))
    }

    /// Returns the recent diagnostic trail captured for the operator dashboard.
    #[must_use]
    pub fn recent_diagnostics(&self) -> Vec<RecentDiagnosticEvent> {
        self.inner
            .recent_diagnostics
            .lock()
            .map(|events| events.iter().cloned().collect())
            .unwrap_or_default()
    }
}

fn decrement_clamped(counter: &AtomicU64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
        Some(current.saturating_sub(1))
    });
}

fn update_max(counter: &AtomicU64, candidate: u64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
        Some(current.max(candidate))
    });
}

fn load_counter(counter: &AtomicU64) -> u64 {
    counter.load(Ordering::Relaxed)
}

fn write_http_metrics(output: &mut String, inner: &ServerObservabilityInner) {
    append_counter_group(
        output,
        "rarena_http_requests_total",
        "Total HTTP requests by low-cardinality route label.",
        &[
            ("route", "root", load_counter(&inner.http_root_requests)),
            (
                "route",
                "healthz",
                load_counter(&inner.http_healthz_requests),
            ),
            (
                "route",
                "metrics",
                load_counter(&inner.http_metrics_requests),
            ),
            ("route", "admin", load_counter(&inner.http_admin_requests)),
            (
                "route",
                "session_bootstrap",
                load_counter(&inner.http_session_bootstrap_requests),
            ),
            ("route", "ws", load_counter(&inner.http_websocket_requests)),
            (
                "route",
                "static_asset",
                load_counter(&inner.http_static_asset_requests),
            ),
        ],
    );
}

fn write_websocket_metrics(output: &mut String, inner: &ServerObservabilityInner) {
    append_simple_counter(
        output,
        "rarena_websocket_upgrade_attempts_total",
        "Total websocket upgrade attempts.",
        load_counter(&inner.websocket_upgrade_attempts),
    );
    append_simple_counter(
        output,
        "rarena_websocket_sessions_bound_total",
        "Total websocket sessions that successfully bound to a player id.",
        load_counter(&inner.websocket_sessions_bound),
    );
    append_integer_gauge(
        output,
        "rarena_websocket_sessions_active",
        "Currently active websocket sessions bound to players.",
        load_counter(&inner.websocket_sessions_active),
    );
    append_simple_counter(
        output,
        "rarena_websocket_disconnects_total",
        "Total websocket disconnects after a session bound to a player.",
        load_counter(&inner.websocket_disconnects),
    );
    append_simple_counter(
        output,
        "rarena_websocket_rejections_total",
        "Total websocket protocol or session rejections.",
        load_counter(&inner.websocket_rejections),
    );
}

fn write_ingress_metrics(output: &mut String, inner: &ServerObservabilityInner) {
    append_counter_group(
        output,
        "rarena_ingress_packets_total",
        "Total packets observed at the websocket ingress boundary.",
        &[
            (
                "result",
                "accepted",
                load_counter(&inner.ingress_packets_accepted),
            ),
            (
                "result",
                "rejected",
                load_counter(&inner.ingress_packets_rejected),
            ),
        ],
    );
}

fn write_tick_metrics(output: &mut String, inner: &ServerObservabilityInner) {
    append_simple_counter(
        output,
        "rarena_tick_iterations_total",
        "Total simulation tick iterations completed by the runtime.",
        load_counter(&inner.tick_iterations),
    );
    append_preformatted_gauge(
        output,
        "rarena_tick_duration_last_seconds",
        "Duration of the most recent simulation tick in seconds.",
        &format_seconds_from_micros(load_counter(&inner.tick_duration_last_micros)),
    );
    append_preformatted_gauge(
        output,
        "rarena_tick_duration_max_seconds",
        "Maximum observed simulation tick duration in seconds.",
        &format_seconds_from_micros(load_counter(&inner.tick_duration_max_micros)),
    );
}

fn write_uptime_metric(output: &mut String, uptime: Duration) {
    append_preformatted_gauge(
        output,
        "rarena_uptime_seconds",
        "Server uptime in seconds since this process started.",
        &format_duration_seconds(uptime),
    );
}

fn write_build_info_metric(output: &mut String, version: &str) {
    let escaped_version = escape_prometheus_label(version);
    let _ = writeln!(
        output,
        "# HELP rarena_build_info Build metadata for the current Rusaren server process."
    );
    let _ = writeln!(output, "# TYPE rarena_build_info gauge");
    let _ = writeln!(
        output,
        "rarena_build_info{{version=\"{escaped_version}\"}} 1"
    );
}

fn format_seconds_from_micros(micros: u64) -> String {
    format!("{}.{:06}", micros / 1_000_000, micros % 1_000_000)
}

fn format_duration_seconds(duration: Duration) -> String {
    format!("{}.{:06}", duration.as_secs(), duration.subsec_micros())
}

fn append_simple_counter(output: &mut String, metric: &str, help: &str, value: u64) {
    let _ = writeln!(output, "# HELP {metric} {help}");
    let _ = writeln!(output, "# TYPE {metric} counter");
    let _ = writeln!(output, "{metric} {value}");
}

fn append_integer_gauge(output: &mut String, metric: &str, help: &str, value: u64) {
    let _ = writeln!(output, "# HELP {metric} {help}");
    let _ = writeln!(output, "# TYPE {metric} gauge");
    let _ = writeln!(output, "{metric} {value}");
}

fn append_preformatted_gauge(output: &mut String, metric: &str, help: &str, value: &str) {
    let _ = writeln!(output, "# HELP {metric} {help}");
    let _ = writeln!(output, "# TYPE {metric} gauge");
    let _ = writeln!(output, "{metric} {value}");
}

fn append_counter_group(output: &mut String, metric: &str, help: &str, rows: &[(&str, &str, u64)]) {
    let _ = writeln!(output, "# HELP {metric} {help}");
    let _ = writeln!(output, "# TYPE {metric} counter");
    for (label_key, label_value, value) in rows {
        let safe_value = escape_prometheus_label(label_value);
        let _ = writeln!(output, "{metric}{{{label_key}=\"{safe_value}\"}} {value}");
    }
}

fn escape_prometheus_label(value: &str) -> String {
    value
        .replace('\\', r"\\")
        .replace('"', "\\\"")
        .replace('\n', r"\n")
}

#[cfg(test)]
mod tests;
