#![allow(clippy::expect_used, clippy::too_many_lines)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use game_api::{
    spawn_dev_server_with_options, ClientSignalMessage, DevServerOptions, ServerSignalMessage,
    SignalingIceCandidate, SignalingSessionDescription, WebRtcIceServerConfig, WebRtcRuntimeConfig,
};
use game_domain::{PlayerName, ReadyState, SkillTree, TeamSide};
use game_net::{ClientControlCommand, ServerControlEvent, ValidatedInputFrame, BUTTON_CAST};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as ClientMessage;
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::APIBuilder;
use webrtc::data_channel::data_channel_init::RTCDataChannelInit;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::data_channel::RTCDataChannel;
use webrtc::ice_transport::ice_candidate::RTCIceCandidate;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::RTCPeerConnection;

type SignalStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

const COMBAT_FRAME_MS: u16 = 100;

#[path = "realtime_webrtc/client.rs"]
mod client;
#[path = "realtime_webrtc/session.rs"]
mod session;
#[path = "realtime_webrtc/support.rs"]
mod support;

use client::{slot_one_cast_input, WebRtcClient};
use support::{bootstrap_signal_url, start_server_fast};
