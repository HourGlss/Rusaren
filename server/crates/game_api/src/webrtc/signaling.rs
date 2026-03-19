use super::{
    Deserialize, RTCIceCandidateInit, RTCSessionDescription, Serialize, Value,
    WebRtcIceServerConfig, CONTROL_DATA_CHANNEL_ID, INPUT_DATA_CHANNEL_ID,
    MAX_SIGNAL_CANDIDATE_BYTES, MAX_SIGNAL_MESSAGE_BYTES, MAX_SIGNAL_MID_BYTES,
    MAX_SIGNAL_SDP_BYTES, SNAPSHOT_DATA_CHANNEL_ID,
};

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
