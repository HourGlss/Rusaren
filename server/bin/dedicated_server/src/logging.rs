use std::env;

use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LogFormat {
    Pretty,
    Json,
}

impl LogFormat {
    pub(crate) fn parse(raw: &str) -> Result<Self, String> {
        if raw.eq_ignore_ascii_case("pretty") {
            return Ok(Self::Pretty);
        }
        if raw.eq_ignore_ascii_case("json") {
            return Ok(Self::Json);
        }

        Err(format!(
            "unsupported RARENA_LOG_FORMAT '{raw}'; expected 'pretty' or 'json'"
        ))
    }
}

pub(crate) fn parse_log_format_from_env() -> Result<LogFormat, String> {
    env::var("RARENA_LOG_FORMAT")
        .map_or_else(|_| Ok(LogFormat::Pretty), |raw| LogFormat::parse(&raw))
}

pub(crate) fn init_tracing(log_format: LogFormat) -> Result<(), String> {
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info,axum=info,tower_http=info"))
        .map_err(|error| format!("failed to configure tracing filter: {error}"))?;

    match log_format {
        LogFormat::Pretty => tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer().compact())
            .try_init()
            .map_err(|error| format!("failed to initialize tracing: {error}")),
        LogFormat::Json => tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer().json())
            .try_init()
            .map_err(|error| format!("failed to initialize tracing: {error}")),
    }
}
