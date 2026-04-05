use std::env;
use std::path::PathBuf;
use std::time::Duration;

use game_api::{AdminAuthConfig, WebRtcRuntimeConfig};
use game_content::GameContent;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ServerConfig {
    pub bind_address: String,
    pub record_store_path: PathBuf,
    pub combat_log_path: PathBuf,
    pub content_root: PathBuf,
    pub web_client_root: PathBuf,
    pub tick_interval: Duration,
    pub simulation_step_ms: u16,
    pub webrtc: WebRtcRuntimeConfig,
    pub admin_auth: Option<AdminAuthConfig>,
}

impl ServerConfig {
    pub(crate) fn from_env() -> Result<Self, String> {
        let record_store_path = env::var_os("RARENA_RECORD_STORE_PATH")
            .map_or_else(default_record_store_path, PathBuf::from);
        let content_root =
            env::var_os("RARENA_CONTENT_ROOT").map_or_else(default_content_root, PathBuf::from);
        let simulation_step_ms = GameContent::load_from_root(&content_root)
            .map_err(|error| format!("failed to load content configuration: {error}"))?
            .configuration()
            .simulation
            .combat_frame_ms;
        Ok(Self {
            bind_address: env::var("RARENA_BIND")
                .unwrap_or_else(|_| String::from("127.0.0.1:3000")),
            combat_log_path: env::var_os("RARENA_COMBAT_LOG_PATH").map_or_else(
                || companion_combat_log_path(&record_store_path),
                PathBuf::from,
            ),
            record_store_path,
            content_root,
            web_client_root: env::var_os("RARENA_WEB_CLIENT_ROOT")
                .map_or_else(default_web_client_root, PathBuf::from),
            tick_interval: parse_tick_interval(
                env::var("RARENA_TICK_INTERVAL_MS").ok(),
                simulation_step_ms,
            ),
            simulation_step_ms,
            webrtc: parse_webrtc_config_from_env()?,
            admin_auth: parse_admin_auth_from_env()?,
        })
    }
}

pub(crate) fn companion_combat_log_path(record_store_path: &std::path::Path) -> PathBuf {
    let stem = record_store_path
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .filter(|value| !value.is_empty())
        .unwrap_or("player_records");
    let file_name = format!("{stem}.combat.sqlite");
    record_store_path.parent().map_or_else(
        || PathBuf::from(file_name.clone()),
        |parent| parent.join(file_name.clone()),
    )
}

pub(crate) fn default_record_store_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("var")
        .join("player_records.tsv")
}

pub(crate) fn default_content_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("content")
}

pub(crate) fn default_web_client_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("static")
        .join("webclient")
}

pub(crate) fn parse_tick_interval(raw: Option<String>, default_millis: u16) -> Duration {
    raw.and_then(|value| value.parse::<u64>().ok())
        .filter(|millis| *millis > 0)
        .map_or_else(
            || Duration::from_millis(u64::from(default_millis)),
            Duration::from_millis,
        )
}

pub(crate) fn parse_csv_urls(raw: Option<String>) -> Vec<String> {
    raw.into_iter()
        .flat_map(|value| {
            value
                .split(',')
                .map(str::trim)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|value| !value.is_empty())
        .collect()
}

pub(crate) fn parse_turn_ttl(raw: Option<String>) -> Result<Duration, String> {
    let Some(raw) = raw else {
        return Ok(WebRtcRuntimeConfig::default().turn_ttl);
    };

    let seconds = raw
        .parse::<u64>()
        .map_err(|error| format!("failed to parse RARENA_WEBRTC_TURN_TTL_SECONDS: {error}"))?;
    if seconds == 0 {
        return Err(String::from(
            "RARENA_WEBRTC_TURN_TTL_SECONDS must be greater than zero",
        ));
    }

    Ok(Duration::from_secs(seconds))
}

pub(crate) fn parse_webrtc_config_from_env() -> Result<WebRtcRuntimeConfig, String> {
    Ok(WebRtcRuntimeConfig {
        stun_urls: parse_csv_urls(env::var("RARENA_WEBRTC_STUN_URLS").ok()),
        turn_urls: parse_csv_urls(env::var("RARENA_WEBRTC_TURN_URLS").ok()),
        turn_shared_secret: env::var("RARENA_WEBRTC_TURN_SECRET")
            .ok()
            .map(|secret| secret.trim().to_string())
            .filter(|secret| !secret.is_empty()),
        turn_ttl: parse_turn_ttl(env::var("RARENA_WEBRTC_TURN_TTL_SECONDS").ok())?,
    })
}

pub(crate) fn parse_admin_auth_from_env() -> Result<Option<AdminAuthConfig>, String> {
    let username = env::var("RARENA_ADMIN_USERNAME")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let password = env::var("RARENA_ADMIN_PASSWORD")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    match (username, password) {
        (None, None) => Ok(None),
        (Some(_), None) | (None, Some(_)) => Err(String::from(
            "RARENA_ADMIN_USERNAME and RARENA_ADMIN_PASSWORD must either both be set or both be omitted",
        )),
        (Some(username), Some(password)) => AdminAuthConfig::new(username, password).map(Some),
    }
}
