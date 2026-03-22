use std::env;
use std::path::PathBuf;
use std::time::Duration;

use game_api::{AdminAuthConfig, WebRtcRuntimeConfig};
use game_sim::COMBAT_FRAME_MS;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ServerConfig {
    pub bind_address: String,
    pub record_store_path: PathBuf,
    pub content_root: PathBuf,
    pub web_client_root: PathBuf,
    pub tick_interval: Duration,
    pub webrtc: WebRtcRuntimeConfig,
    pub admin_auth: Option<AdminAuthConfig>,
}

impl ServerConfig {
    pub(crate) fn from_env() -> Result<Self, String> {
        Ok(Self {
            bind_address: env::var("RARENA_BIND")
                .unwrap_or_else(|_| String::from("127.0.0.1:3000")),
            record_store_path: env::var_os("RARENA_RECORD_STORE_PATH")
                .map_or_else(default_record_store_path, PathBuf::from),
            content_root: env::var_os("RARENA_CONTENT_ROOT")
                .map_or_else(default_content_root, PathBuf::from),
            web_client_root: env::var_os("RARENA_WEB_CLIENT_ROOT")
                .map_or_else(default_web_client_root, PathBuf::from),
            tick_interval: parse_tick_interval(env::var("RARENA_TICK_INTERVAL_MS").ok()),
            webrtc: parse_webrtc_config_from_env()?,
            admin_auth: parse_admin_auth_from_env()?,
        })
    }
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

pub(crate) fn parse_tick_interval(raw: Option<String>) -> Duration {
    raw.and_then(|value| value.parse::<u64>().ok())
        .filter(|millis| *millis > 0)
        .map_or_else(
            || Duration::from_millis(u64::from(COMBAT_FRAME_MS)),
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
