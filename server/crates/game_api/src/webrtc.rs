use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha1::Sha1;
use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

use crate::ConnectionId;

type HmacSha1 = Hmac<Sha1>;

/// Negotiated id for the reliable ordered control data channel.
pub const CONTROL_DATA_CHANNEL_ID: u16 = 0;
/// Negotiated id for the unreliable player-input data channel.
pub const INPUT_DATA_CHANNEL_ID: u16 = 1;
/// Negotiated id for the unreliable snapshot data channel.
pub const SNAPSHOT_DATA_CHANNEL_ID: u16 = 2;
/// Maximum accepted size for one signaling websocket message.
pub const MAX_SIGNAL_MESSAGE_BYTES: usize = 128 * 1024;
const MAX_SIGNAL_SDP_BYTES: usize = 96 * 1024;
const MAX_SIGNAL_CANDIDATE_BYTES: usize = 4096;
const MAX_SIGNAL_MID_BYTES: usize = 64;
const DEFAULT_TURN_TTL_SECS: u64 = 3600;

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
    fn validate(&self) -> Result<(), String> {
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

/// Fixed mapping between semantic channel names and negotiated data-channel ids.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignalingChannelMap {
    /// Reliable ordered control channel id.
    pub control: u16,
    /// Unreliable player-input channel id.
    pub input: u16,
    /// Unreliable snapshot channel id.
    pub snapshot: u16,
}

impl Default for SignalingChannelMap {
    fn default() -> Self {
        Self {
            control: CONTROL_DATA_CHANNEL_ID,
            input: INPUT_DATA_CHANNEL_ID,
            snapshot: SNAPSHOT_DATA_CHANNEL_ID,
        }
    }
}

/// JSON-serializable `WebRTC` offer or answer description.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignalingSessionDescription {
    /// SDP type, limited to `offer` or `answer`.
    #[serde(rename = "type")]
    pub sdp_type: String,
    /// Raw SDP payload.
    pub sdp: String,
}

impl SignalingSessionDescription {
    /// Validates description type and size limits.
    pub fn validate(&self) -> Result<(), String> {
        let sdp_type = self.sdp_type.trim();
        if sdp_type != "offer" && sdp_type != "answer" {
            return Err(format!(
                "unsupported signaling session description type '{sdp_type}'"
            ));
        }
        if self.sdp.is_empty() {
            return Err(String::from("session description SDP must not be empty"));
        }
        if self.sdp.len() > MAX_SIGNAL_SDP_BYTES {
            return Err(format!(
                "session description SDP length {} exceeds maximum {}",
                self.sdp.len(),
                MAX_SIGNAL_SDP_BYTES
            ));
        }

        Ok(())
    }

    /// Validates that the description is a client-originated offer.
    pub fn validate_as_offer(&self) -> Result<(), String> {
        self.validate()?;
        if self.sdp_type.trim() != "offer" {
            return Err(String::from(
                "clients may only submit offer session descriptions",
            ));
        }
        Ok(())
    }

    /// Converts the signaling payload into an `RTCSessionDescription`.
    pub fn to_rtc_description(&self) -> Result<RTCSessionDescription, String> {
        self.validate()?;
        match self.sdp_type.trim() {
            "offer" => RTCSessionDescription::offer(self.sdp.clone())
                .map_err(|error| format!("failed to parse offer session description: {error}")),
            "answer" => RTCSessionDescription::answer(self.sdp.clone())
                .map_err(|error| format!("failed to parse answer session description: {error}")),
            _ => Err(format!(
                "unsupported signaling session description type '{}'",
                self.sdp_type
            )),
        }
    }

    /// Converts an `RTCSessionDescription` into the signaling wire format.
    #[must_use]
    pub fn from_rtc_description(description: &RTCSessionDescription) -> Self {
        Self {
            sdp_type: description.sdp_type.to_string(),
            sdp: description.sdp.clone(),
        }
    }
}

/// JSON-serializable ICE candidate exchanged over the signaling websocket.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignalingIceCandidate {
    /// Raw candidate line.
    pub candidate: String,
    /// Optional media id for the candidate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sdp_mid: Option<String>,
    /// Optional media-line index for the candidate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sdp_mline_index: Option<u16>,
}

impl SignalingIceCandidate {
    /// Validates size and field constraints for one ICE candidate.
    pub fn validate(&self) -> Result<(), String> {
        if self.candidate.trim().is_empty() {
            return Err(String::from("ICE candidate must not be empty"));
        }
        if self.candidate.len() > MAX_SIGNAL_CANDIDATE_BYTES {
            return Err(format!(
                "ICE candidate length {} exceeds maximum {}",
                self.candidate.len(),
                MAX_SIGNAL_CANDIDATE_BYTES
            ));
        }

        if let Some(sdp_mid) = &self.sdp_mid {
            if sdp_mid.len() > MAX_SIGNAL_MID_BYTES {
                return Err(format!(
                    "ICE candidate sdp_mid length {} exceeds maximum {}",
                    sdp_mid.len(),
                    MAX_SIGNAL_MID_BYTES
                ));
            }
        }

        Ok(())
    }

    /// Converts the signaling form into the `webrtc` crate type.
    pub fn to_rtc_candidate_init(&self) -> Result<RTCIceCandidateInit, String> {
        self.validate()?;
        Ok(RTCIceCandidateInit {
            candidate: self.candidate.clone(),
            sdp_mid: self.sdp_mid.clone(),
            sdp_mline_index: self.sdp_mline_index,
            username_fragment: None,
        })
    }

    /// Converts the `webrtc` crate candidate type into the signaling wire format.
    #[must_use]
    pub fn from_rtc_candidate_init(candidate: &RTCIceCandidateInit) -> Self {
        Self {
            candidate: candidate.candidate.clone(),
            sdp_mid: candidate.sdp_mid.clone(),
            sdp_mline_index: candidate.sdp_mline_index,
        }
    }
}

/// Client-to-server signaling messages accepted on `/ws`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClientSignalMessage {
    /// Submits the client's SDP offer.
    SessionDescription {
        /// The offer payload.
        description: SignalingSessionDescription,
    },
    /// Submits one remote ICE candidate.
    IceCandidate {
        /// The candidate payload.
        candidate: SignalingIceCandidate,
    },
    /// Requests an orderly signaling shutdown.
    Bye,
}

impl ClientSignalMessage {
    /// Validates the message according to the client signaling contract.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::SessionDescription { description } => description.validate_as_offer(),
            Self::IceCandidate { candidate } => candidate.validate(),
            Self::Bye => Ok(()),
        }
    }
}

/// Server-to-client signaling messages sent over `/ws`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ServerSignalMessage {
    /// Announces protocol details before the `WebRTC` offer is processed.
    Hello {
        /// Current network protocol version.
        protocol_version: u8,
        /// ICE servers to use for this connection.
        ice_servers: Vec<WebRtcIceServerConfig>,
        /// Negotiated channel ids to open in the browser.
        channels: SignalingChannelMap,
    },
    /// Sends the server's SDP answer.
    SessionDescription {
        /// The answer payload.
        description: SignalingSessionDescription,
    },
    /// Sends one local ICE candidate to the client.
    IceCandidate {
        /// The candidate payload.
        candidate: SignalingIceCandidate,
    },
    /// Reports a signaling failure to the client.
    Error {
        /// Human-readable error message safe to show in the shell.
        message: String,
    },
}

/// Parses and validates one client signaling message from websocket text.
///
/// VERIFIED MODEL: `server/verus/webrtc_signaling_model.rs` mirrors the message-order
/// contract enforced by the runtime signaling flow that uses this decoder. The proof
/// model is not a direct proof over these production types, so runtime tests remain
/// mandatory for the actual websocket and peer-connection integration.
pub fn decode_client_signal_message(text: &str) -> Result<ClientSignalMessage, String> {
    if text.is_empty() {
        return Err(String::from("signaling message must not be empty"));
    }
    if text.len() > MAX_SIGNAL_MESSAGE_BYTES {
        return Err(format!(
            "signaling message length {} exceeds maximum {}",
            text.len(),
            MAX_SIGNAL_MESSAGE_BYTES
        ));
    }

    let value = serde_json::from_str::<Value>(text)
        .map_err(|error| format!("invalid signaling JSON: {error}"))?;
    validate_client_signal_json_shape(&value)?;
    let message = serde_json::from_value::<ClientSignalMessage>(value)
        .map_err(|error| format!("invalid signaling JSON: {error}"))?;
    message.validate()?;
    Ok(message)
}

/// Rejects obvious shape mismatches before deserializing into the signaling enum.
fn validate_client_signal_json_shape(value: &Value) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or_else(|| String::from("invalid signaling JSON: root value must be an object"))?;
    let message_type = object
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| String::from("invalid signaling JSON: missing string field 'type'"))?;

    let allowed_keys: &[&str] = match message_type {
        "session_description" => &["type", "description"],
        "ice_candidate" => &["type", "candidate"],
        "bye" => &["type"],
        _ => return Ok(()),
    };

    for key in object.keys() {
        if !allowed_keys.iter().any(|allowed| key == allowed) {
            return Err(format!(
                "invalid signaling JSON: unexpected field '{key}' for message type '{message_type}'"
            ));
        }
    }

    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with_turn() -> WebRtcRuntimeConfig {
        WebRtcRuntimeConfig {
            stun_urls: vec![String::from("stun:turn.example.com:3478")],
            turn_urls: vec![
                String::from("turn:turn.example.com:3478?transport=udp"),
                String::from("turn:turn.example.com:3478?transport=tcp"),
            ],
            turn_shared_secret: Some(String::from("shared-secret")),
            turn_ttl: Duration::from_secs(600),
        }
    }

    #[test]
    fn signaling_decoder_rejects_answers_from_clients_and_unknown_fields() {
        let answer =
            r#"{"type":"session_description","description":{"type":"answer","sdp":"v=0"}}"#;
        assert_eq!(
            decode_client_signal_message(answer),
            Err(String::from(
                "clients may only submit offer session descriptions"
            ))
        );

        let unknown_field = r#"{"type":"bye","extra":true}"#;
        let result = decode_client_signal_message(unknown_field);
        assert!(matches!(
            result,
            Err(message) if message.starts_with("invalid signaling JSON:")
        ));
    }

    #[test]
    fn signaling_decoder_accepts_offers_and_candidates() {
        let offer =
            r#"{"type":"session_description","description":{"type":"offer","sdp":"v=0\r\n"}}"#;
        assert!(matches!(
            decode_client_signal_message(offer),
            Ok(ClientSignalMessage::SessionDescription { .. })
        ));

        let candidate = r#"{"type":"ice_candidate","candidate":{"candidate":"candidate:0 1 UDP 2122252543 127.0.0.1 5000 typ host","sdp_mid":"0","sdp_mline_index":0}}"#;
        assert!(matches!(
            decode_client_signal_message(candidate),
            Ok(ClientSignalMessage::IceCandidate { .. })
        ));
    }

    #[test]
    fn runtime_config_generates_ephemeral_turn_credentials() {
        let connection_id = ConnectionId::new(7).expect("valid connection id");
        let ice_servers = config_with_turn()
            .ice_servers_for_connection(connection_id, UNIX_EPOCH + Duration::from_secs(1_000))
            .expect("ICE servers should build");

        assert_eq!(ice_servers.len(), 2);
        assert_eq!(
            ice_servers[0].urls,
            vec![String::from("stun:turn.example.com:3478")]
        );
        assert_eq!(ice_servers[1].urls.len(), 2);
        assert!(ice_servers[1].username.starts_with("1600:conn-7"));
        assert!(!ice_servers[1].credential.is_empty());
    }

    #[test]
    fn runtime_config_requires_secret_for_turn_urls() {
        let config = WebRtcRuntimeConfig {
            stun_urls: Vec::new(),
            turn_urls: vec![String::from("turn:turn.example.com:3478?transport=udp")],
            turn_shared_secret: None,
            turn_ttl: Duration::from_secs(300),
        };

        assert_eq!(
            config.validate(),
            Err(String::from(
                "TURN URLs require RARENA_WEBRTC_TURN_SECRET to be configured"
            ))
        );
    }
}
