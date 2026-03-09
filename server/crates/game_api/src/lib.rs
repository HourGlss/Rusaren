//! HTTP and service-facing orchestration for the arena server.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![cfg_attr(test, allow(clippy::expect_used))]

mod app;
mod observability;
mod realtime;
mod records;
mod transport;
mod webrtc;

pub use app::{AppError, ServerApp};
pub use observability::{classify_http_path, HttpRouteLabel, ServerObservability};
pub use realtime::{
    spawn_dev_server, spawn_dev_server_with_options, DevServerHandle, DevServerOptions,
};
pub use records::{
    canonicalize_record_store_contents, PlayerRecordStore, RecordStoreError, MAX_RECORD_STORE_BYTES,
};
pub use transport::{AppTransport, ConnectionId, HeadlessClient, InMemoryTransport};
pub use webrtc::{
    decode_client_signal_message, ClientSignalMessage, ServerSignalMessage, SignalingChannelMap,
    SignalingIceCandidate, SignalingSessionDescription, WebRtcIceServerConfig, WebRtcRuntimeConfig,
    CONTROL_DATA_CHANNEL_ID, INPUT_DATA_CHANNEL_ID, MAX_SIGNAL_MESSAGE_BYTES,
    SNAPSHOT_DATA_CHANNEL_ID,
};
