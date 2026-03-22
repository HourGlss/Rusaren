use super::*;

/// JSON-serializable ICE server configuration sent to the browser client.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebRtcIceServerConfig {
    /// `stun:` or `turn:` URLs exposed to the browser.
    pub urls: Vec<String>,
    /// Ephemeral TURN username, or blank for STUN-only servers.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub username: String,
    /// Ephemeral TURN credential, or blank for STUN-only servers.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub credential: String,
}

impl WebRtcIceServerConfig {
    /// Converts the serialized config into the `webrtc` crate type.
    #[must_use]
    pub fn to_rtc_ice_server(&self) -> RTCIceServer {
        RTCIceServer {
            urls: self.urls.clone(),
            username: self.username.clone(),
            credential: self.credential.clone(),
        }
    }

    /// Validates that the config is safe to send to the client and feed into `WebRTC`.
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.urls.is_empty() {
            return Err(String::from(
                "ICE server configuration requires at least one URL",
            ));
        }

        for url in &self.urls {
            let trimmed = url.trim();
            if trimmed.is_empty() {
                return Err(String::from("ICE server URLs must not be blank"));
            }
            if trimmed.len() > MAX_SIGNAL_CANDIDATE_BYTES {
                return Err(format!(
                    "ICE server URL length {} exceeds maximum {}",
                    trimmed.len(),
                    MAX_SIGNAL_CANDIDATE_BYTES
                ));
            }
        }

        Ok(())
    }
}

/// Runtime configuration for STUN/TURN integration on the server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebRtcRuntimeConfig {
    /// STUN URLs exposed to the client for direct-path discovery.
    pub stun_urls: Vec<String>,
    /// TURN URLs exposed to the client for relay fallback.
    pub turn_urls: Vec<String>,
    /// Shared secret used to mint temporary TURN credentials.
    pub turn_shared_secret: Option<String>,
    /// Lifetime of generated TURN credentials.
    pub turn_ttl: Duration,
}

impl Default for WebRtcRuntimeConfig {
    fn default() -> Self {
        Self {
            stun_urls: Vec::new(),
            turn_urls: Vec::new(),
            turn_shared_secret: None,
            turn_ttl: Duration::from_secs(DEFAULT_TURN_TTL_SECS),
        }
    }
}

impl WebRtcRuntimeConfig {
    /// Validates the runtime configuration loaded from the environment.
    pub fn validate(&self) -> Result<(), String> {
        for url in &self.stun_urls {
            if url.trim().is_empty() {
                return Err(String::from("STUN URLs must not contain blank entries"));
            }
        }
        for url in &self.turn_urls {
            if url.trim().is_empty() {
                return Err(String::from("TURN URLs must not contain blank entries"));
            }
        }

        if !self.turn_urls.is_empty()
            && self
                .turn_shared_secret
                .as_ref()
                .is_none_or(|secret| secret.trim().is_empty())
        {
            return Err(String::from(
                "TURN URLs require RARENA_WEBRTC_TURN_SECRET to be configured",
            ));
        }

        if self.turn_ttl.is_zero() {
            return Err(String::from(
                "TURN credential TTL must be greater than zero",
            ));
        }

        Ok(())
    }

    /// Builds the ICE server list for one connection, including ephemeral TURN credentials.
    pub fn ice_servers_for_connection(
        &self,
        connection_id: ConnectionId,
        now: SystemTime,
    ) -> Result<Vec<WebRtcIceServerConfig>, String> {
        self.validate()?;

        let mut servers = Vec::new();
        if !self.stun_urls.is_empty() {
            servers.push(WebRtcIceServerConfig {
                urls: self.stun_urls.clone(),
                username: String::new(),
                credential: String::new(),
            });
        }

        if !self.turn_urls.is_empty() {
            let secret = self
                .turn_shared_secret
                .as_ref()
                .ok_or_else(|| String::from("TURN shared secret is required"))?;
            let (username, credential) =
                generate_turn_credentials(secret, connection_id, now, self.turn_ttl)?;
            servers.push(WebRtcIceServerConfig {
                urls: self.turn_urls.clone(),
                username,
                credential,
            });
        }

        for server in &servers {
            server.validate()?;
        }

        Ok(servers)
    }
}

/// Generates ephemeral TURN credentials from the shared secret and connection id.
fn generate_turn_credentials(
    shared_secret: &str,
    connection_id: ConnectionId,
    now: SystemTime,
    ttl: Duration,
) -> Result<(String, String), String> {
    let now_secs = now
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before the unix epoch: {error}"))?
        .as_secs();
    let expires = now_secs.saturating_add(ttl.as_secs());
    let username = format!("{expires}:conn-{}", connection_id.get());
    let mut mac = HmacSha1::new_from_slice(shared_secret.as_bytes())
        .map_err(|error| format!("invalid TURN shared secret: {error}"))?;
    mac.update(username.as_bytes());
    let credential = BASE64_STANDARD.encode(mac.finalize().into_bytes());
    Ok((username, credential))
}
