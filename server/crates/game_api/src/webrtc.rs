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

mod config;
mod signaling;

pub use config::{WebRtcIceServerConfig, WebRtcRuntimeConfig};
pub use signaling::{
    decode_client_signal_message, ClientSignalMessage, ServerSignalMessage, SignalingChannelMap,
    SignalingIceCandidate, SignalingSessionDescription,
};

#[cfg(test)]
mod tests;
