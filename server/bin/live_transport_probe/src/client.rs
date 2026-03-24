use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use game_api::{
    ClientSignalMessage, ServerSignalMessage, SignalingIceCandidate, SignalingSessionDescription,
    WebRtcIceServerConfig,
};
use game_domain::PlayerName;
use game_net::{ClientControlCommand, ServerControlEvent, ValidatedInputFrame};
use reqwest::Client as HttpClient;
use serde::Deserialize;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as SocketMessage;
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::APIBuilder;
use webrtc::data_channel::data_channel_init::RTCDataChannelInit;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::data_channel::RTCDataChannel;
use webrtc::ice_transport::ice_candidate::RTCIceCandidate;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::RTCPeerConnection;

use crate::{ProbeError, ProbeResult};

type SignalStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
type SignalReader = futures_util::stream::SplitStream<SignalStream>;
type SignalWriter = futures_util::stream::SplitSink<SignalStream, SocketMessage>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientRuntimeMessage {
    ServerEvent(ServerControlEvent),
    Notice { category: String, detail: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingInput {
    pub move_x: i16,
    pub move_y: i16,
    pub aim_x: i16,
    pub aim_y: i16,
    pub buttons: u16,
    pub ability_or_context: u16,
}

pub struct LiveClient {
    signal_tx: mpsc::UnboundedSender<ClientSignalMessage>,
    message_rx: mpsc::UnboundedReceiver<ClientRuntimeMessage>,
    peer: Arc<RTCPeerConnection>,
    control: Arc<RTCDataChannel>,
    input: Arc<RTCDataChannel>,
    next_control_seq: u32,
    next_input_seq: u32,
}

#[derive(Debug, Deserialize)]
struct BootstrapResponse {
    token: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct HelloMessage {
    ice_servers: Vec<WebRtcIceServerConfig>,
}

struct ClientChannels {
    control: Arc<RTCDataChannel>,
    input: Arc<RTCDataChannel>,
}

impl LiveClient {
    pub async fn connect(origin: &str, raw_name: &str, timeout: Duration) -> ProbeResult<Self> {
        let (signal_writer, mut signal_reader) = open_signaling_stream(origin).await?;
        let hello = wait_for_server_hello(timeout, &mut signal_reader).await?;
        let peer = Arc::new(create_client_peer(&hello.ice_servers).await?);
        let (signal_tx, signal_rx) = mpsc::unbounded_channel::<ClientSignalMessage>();
        let (message_tx, message_rx) = mpsc::unbounded_channel::<ClientRuntimeMessage>();
        let (control_open_tx, control_open_rx) = oneshot::channel::<()>();

        spawn_signal_writer(signal_writer, signal_rx);
        let channels = create_runtime_channels(&peer, &message_tx, Some(control_open_tx)).await?;
        install_client_ice_callback(&peer, &signal_tx, &message_tx);
        spawn_signal_reader(signal_reader, Arc::clone(&peer), message_tx);
        submit_local_offer(&peer, &signal_tx).await?;
        await_control_channel_open(timeout, control_open_rx).await?;

        let mut client = Self {
            signal_tx,
            message_rx,
            peer,
            control: channels.control,
            input: channels.input,
            next_control_seq: 1,
            next_input_seq: 1,
        };
        client.send_connect_command(raw_name).await?;
        Ok(client)
    }

    async fn send_connect_command(&mut self, raw_name: &str) -> ProbeResult<()> {
        self.send_command(ClientControlCommand::Connect {
            player_name: PlayerName::new(raw_name)
                .map_err(|error| ProbeError::new(error.to_string()))?,
        })
        .await
    }

    pub async fn send_command(&mut self, command: ClientControlCommand) -> ProbeResult<()> {
        let packet = command
            .encode_packet(self.next_control_seq, 0)
            .map_err(|error| ProbeError::new(format!("control packet encode failed: {error}")))?;
        self.control
            .send(&Bytes::from(packet))
            .await
            .map_err(|error| ProbeError::new(format!("control packet send failed: {error}")))?;
        self.next_control_seq += 1;
        Ok(())
    }

    pub async fn send_input_action(&mut self, action: PendingInput) -> ProbeResult<u32> {
        let sequence = self.next_input_seq;
        let frame = ValidatedInputFrame::new(
            sequence,
            action.move_x,
            action.move_y,
            action.aim_x,
            action.aim_y,
            action.buttons,
            action.ability_or_context,
        )
        .map_err(|error| ProbeError::new(format!("input frame build failed: {error}")))?;
        let packet = frame
            .encode_packet(sequence, sequence)
            .map_err(|error| ProbeError::new(format!("input frame encode failed: {error}")))?;
        self.input
            .send(&Bytes::from(packet))
            .await
            .map_err(|error| ProbeError::new(format!("input frame send failed: {error}")))?;
        self.next_input_seq += 1;
        Ok(sequence)
    }

    pub async fn recv_message_timeout(
        &mut self,
        timeout: Duration,
    ) -> ProbeResult<Option<ClientRuntimeMessage>> {
        match tokio::time::timeout(timeout, self.message_rx.recv()).await {
            Ok(Some(message)) => Ok(Some(message)),
            Ok(None) => Err(ProbeError::new(
                "client runtime channel closed unexpectedly",
            )),
            Err(_) => Ok(None),
        }
    }

    pub fn try_recv_message(&mut self) -> Option<ClientRuntimeMessage> {
        self.message_rx.try_recv().ok()
    }

    pub async fn close(self) {
        let _ = self.signal_tx.send(ClientSignalMessage::Bye);
        let _ = self.peer.close().await;
    }
}

async fn open_signaling_stream(origin: &str) -> ProbeResult<(SignalWriter, SignalReader)> {
    let signal_url = bootstrap_signal_url(origin).await?;
    let (stream, _) = connect_async(&signal_url)
        .await
        .map_err(|error| ProbeError::new(format!("signaling websocket connect failed: {error}")))?;
    Ok(stream.split())
}

async fn wait_for_server_hello(
    timeout: Duration,
    signal_reader: &mut SignalReader,
) -> ProbeResult<HelloMessage> {
    tokio::time::timeout(timeout, recv_hello(signal_reader))
        .await
        .map_err(|_| ProbeError::new("timed out waiting for server hello"))?
}

fn spawn_signal_writer(
    mut signal_writer: SignalWriter,
    mut signal_rx: mpsc::UnboundedReceiver<ClientSignalMessage>,
) {
    tokio::spawn(async move {
        while let Some(message) = signal_rx.recv().await {
            let Ok(text) = serde_json::to_string(&message) else {
                break;
            };
            if signal_writer
                .send(SocketMessage::Text(text.into()))
                .await
                .is_err()
            {
                break;
            }
        }
    });
}

async fn create_runtime_channels(
    peer: &Arc<RTCPeerConnection>,
    message_tx: &mpsc::UnboundedSender<ClientRuntimeMessage>,
    control_open_tx: Option<oneshot::Sender<()>>,
) -> ProbeResult<ClientChannels> {
    let control = create_client_channel(Arc::clone(peer), "control", control_init()).await?;
    let input = create_client_channel(Arc::clone(peer), "input", unreliable_init(1)).await?;
    let snapshot = create_client_channel(Arc::clone(peer), "snapshot", unreliable_init(2)).await?;

    install_event_channel(&control, "control", message_tx.clone(), control_open_tx);
    install_event_channel(&snapshot, "snapshot", message_tx.clone(), None);
    install_ignore_channel(&input);
    install_peer_callbacks(peer, message_tx.clone());

    Ok(ClientChannels { control, input })
}

fn install_client_ice_callback(
    peer: &Arc<RTCPeerConnection>,
    signal_tx: &mpsc::UnboundedSender<ClientSignalMessage>,
    message_tx: &mpsc::UnboundedSender<ClientRuntimeMessage>,
) {
    let signal_tx = signal_tx.clone();
    let message_tx = message_tx.clone();
    peer.on_ice_candidate(Box::new(move |candidate: Option<RTCIceCandidate>| {
        let signal_tx = signal_tx.clone();
        let message_tx = message_tx.clone();
        Box::pin(async move {
            let Some(candidate) = candidate else {
                return;
            };
            let candidate_init = match candidate.to_json() {
                Ok(candidate_init) => candidate_init,
                Err(error) => {
                    let _ = message_tx.send(ClientRuntimeMessage::Notice {
                        category: String::from("local_ice_candidate_serialize_error"),
                        detail: error.to_string(),
                    });
                    return;
                }
            };
            let _ = signal_tx.send(ClientSignalMessage::IceCandidate {
                candidate: SignalingIceCandidate::from_rtc_candidate_init(&candidate_init),
            });
        })
    }));
}

fn spawn_signal_reader(
    mut signal_reader: SignalReader,
    peer: Arc<RTCPeerConnection>,
    message_tx: mpsc::UnboundedSender<ClientRuntimeMessage>,
) {
    tokio::spawn(async move {
        while let Some(message_result) = signal_reader.next().await {
            match message_result {
                Ok(SocketMessage::Text(text)) => {
                    handle_signal_text(&peer, &message_tx, &text).await;
                }
                Ok(SocketMessage::Close(frame)) => {
                    let _ = message_tx.send(ClientRuntimeMessage::Notice {
                        category: String::from("signal_closed"),
                        detail: frame.map_or_else(
                            || String::from("websocket closed without a frame"),
                            |value| value.reason.to_string(),
                        ),
                    });
                    break;
                }
                Ok(SocketMessage::Binary(_)) => {
                    let _ = message_tx.send(ClientRuntimeMessage::Notice {
                        category: String::from("signal_protocol_error"),
                        detail: String::from("server sent a binary signaling frame"),
                    });
                }
                Ok(SocketMessage::Ping(_) | SocketMessage::Pong(_) | SocketMessage::Frame(_)) => {}
                Err(error) => {
                    let _ = message_tx.send(ClientRuntimeMessage::Notice {
                        category: String::from("signal_read_error"),
                        detail: error.to_string(),
                    });
                    break;
                }
            }
        }
    });
}

async fn handle_signal_text(
    peer: &Arc<RTCPeerConnection>,
    message_tx: &mpsc::UnboundedSender<ClientRuntimeMessage>,
    text: &str,
) {
    let signal_message = match serde_json::from_str::<ServerSignalMessage>(text) {
        Ok(message) => message,
        Err(error) => {
            let _ = message_tx.send(ClientRuntimeMessage::Notice {
                category: String::from("signal_decode_error"),
                detail: error.to_string(),
            });
            return;
        }
    };
    if let Err(error) = apply_server_signal(peer, message_tx, signal_message).await {
        let _ = message_tx.send(ClientRuntimeMessage::Notice {
            category: String::from("signal_apply_error"),
            detail: error.to_string(),
        });
    }
}

async fn submit_local_offer(
    peer: &Arc<RTCPeerConnection>,
    signal_tx: &mpsc::UnboundedSender<ClientSignalMessage>,
) -> ProbeResult<()> {
    let offer = peer
        .create_offer(None)
        .await
        .map_err(|error| ProbeError::new(format!("webrtc offer creation failed: {error}")))?;
    peer.set_local_description(offer.clone())
        .await
        .map_err(|error| ProbeError::new(format!("setting local description failed: {error}")))?;
    let local_offer = peer.local_description().await.unwrap_or(offer);
    signal_tx
        .send(ClientSignalMessage::SessionDescription {
            description: SignalingSessionDescription::from_rtc_description(&local_offer),
        })
        .map_err(|_| ProbeError::new("failed to queue the local offer"))?;
    Ok(())
}

async fn await_control_channel_open(
    timeout: Duration,
    control_open_rx: oneshot::Receiver<()>,
) -> ProbeResult<()> {
    tokio::time::timeout(timeout, control_open_rx)
        .await
        .map_err(|_| ProbeError::new("timed out waiting for the control data channel to open"))?
        .map_err(|_| ProbeError::new("control data channel open notification failed"))?;
    Ok(())
}

async fn bootstrap_signal_url(origin: &str) -> ProbeResult<String> {
    let http_origin = http_origin(origin)?;
    let websocket_origin = websocket_origin(origin)?;
    let client = HttpClient::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| ProbeError::new(format!("http client build failed: {error}")))?;
    let response = client
        .get(format!("{http_origin}/session/bootstrap"))
        .send()
        .await
        .map_err(|error| ProbeError::new(format!("bootstrap request failed: {error}")))?;
    if !response.status().is_success() {
        return Err(ProbeError::new(format!(
            "bootstrap request returned {}",
            response.status()
        )));
    }
    let payload = response
        .json::<BootstrapResponse>()
        .await
        .map_err(|error| ProbeError::new(format!("bootstrap response decode failed: {error}")))?;
    Ok(format!("{websocket_origin}/ws?token={}", payload.token))
}

fn http_origin(origin: &str) -> ProbeResult<String> {
    normalize_origin(origin, true)
}

fn websocket_origin(origin: &str) -> ProbeResult<String> {
    normalize_origin(origin, false)
}

fn normalize_origin(origin: &str, to_http: bool) -> ProbeResult<String> {
    let trimmed = origin.trim().trim_end_matches('/');
    let transformed = if let Some(rest) = trimmed.strip_prefix("https://") {
        if to_http {
            format!("https://{rest}")
        } else {
            format!("wss://{rest}")
        }
    } else if let Some(rest) = trimmed.strip_prefix("http://") {
        if to_http {
            format!("http://{rest}")
        } else {
            format!("ws://{rest}")
        }
    } else if let Some(rest) = trimmed.strip_prefix("wss://") {
        if to_http {
            format!("https://{rest}")
        } else {
            format!("wss://{rest}")
        }
    } else if let Some(rest) = trimmed.strip_prefix("ws://") {
        if to_http {
            format!("http://{rest}")
        } else {
            format!("ws://{rest}")
        }
    } else {
        return Err(ProbeError::new(format!(
            "origin must start with http://, https://, ws://, or wss://; got {origin:?}"
        )));
    };

    if transformed.contains("/session/bootstrap") || transformed.contains("/ws?token=") {
        return Err(ProbeError::new(format!(
            "origin should be a host root, not a transport path: {origin:?}"
        )));
    }

    Ok(transformed)
}

async fn recv_hello(signal_reader: &mut SignalReader) -> ProbeResult<HelloMessage> {
    while let Some(message_result) = signal_reader.next().await {
        match message_result
            .map_err(|error| ProbeError::new(format!("signaling hello read failed: {error}")))?
        {
            SocketMessage::Text(text) => {
                if let Some(hello) = decode_hello_message(&text)? {
                    return Ok(hello);
                }
            }
            SocketMessage::Binary(_) => {
                return Err(ProbeError::new(
                    "server hello arrived as a binary websocket frame",
                ));
            }
            SocketMessage::Close(_) => {
                return Err(ProbeError::new("signaling websocket closed before hello"));
            }
            SocketMessage::Ping(_) | SocketMessage::Pong(_) | SocketMessage::Frame(_) => {}
        }
    }

    Err(ProbeError::new(
        "signaling websocket ended before the server hello arrived",
    ))
}

fn decode_hello_message(text: &str) -> ProbeResult<Option<HelloMessage>> {
    let message = serde_json::from_str::<ServerSignalMessage>(text)
        .map_err(|error| ProbeError::new(format!("server hello decode failed: {error}")))?;
    let ServerSignalMessage::Hello {
        protocol_version,
        ice_servers,
        channels,
    } = message
    else {
        return Ok(None);
    };

    if protocol_version != game_net::PROTOCOL_VERSION {
        return Err(ProbeError::new(format!(
            "protocol version mismatch: expected {}, got {protocol_version}",
            game_net::PROTOCOL_VERSION
        )));
    }
    if channels.control != 0 || channels.input != 1 || channels.snapshot != 2 {
        return Err(ProbeError::new(format!(
            "unexpected signaling channel map: control={} input={} snapshot={}",
            channels.control, channels.input, channels.snapshot
        )));
    }

    Ok(Some(HelloMessage { ice_servers }))
}

async fn create_client_peer(
    ice_servers: &[WebRtcIceServerConfig],
) -> ProbeResult<RTCPeerConnection> {
    let mut media_engine = MediaEngine::default();
    media_engine
        .register_default_codecs()
        .map_err(|error| ProbeError::new(format!("codec registration failed: {error}")))?;
    let api = APIBuilder::new().with_media_engine(media_engine).build();
    api.new_peer_connection(RTCConfiguration {
        ice_servers: ice_servers
            .iter()
            .map(WebRtcIceServerConfig::to_rtc_ice_server)
            .collect(),
        ..Default::default()
    })
    .await
    .map_err(|error| ProbeError::new(format!("peer connection creation failed: {error}")))
}

async fn create_client_channel(
    peer: Arc<RTCPeerConnection>,
    label: &str,
    init: RTCDataChannelInit,
) -> ProbeResult<Arc<RTCDataChannel>> {
    peer.create_data_channel(label, Some(init))
        .await
        .map_err(|error| {
            ProbeError::new(format!("data channel {label:?} creation failed: {error}"))
        })
}

fn control_init() -> RTCDataChannelInit {
    RTCDataChannelInit {
        ordered: Some(true),
        negotiated: Some(0),
        ..Default::default()
    }
}

fn unreliable_init(id: u16) -> RTCDataChannelInit {
    RTCDataChannelInit {
        ordered: Some(false),
        max_retransmits: Some(0),
        negotiated: Some(id),
        ..Default::default()
    }
}

fn install_event_channel(
    channel: &Arc<RTCDataChannel>,
    label: &'static str,
    message_tx: mpsc::UnboundedSender<ClientRuntimeMessage>,
    control_open_tx: Option<oneshot::Sender<()>>,
) {
    let channel = Arc::clone(channel);
    if let Some(control_open_tx) = control_open_tx {
        let control_open_tx = Arc::new(tokio::sync::Mutex::new(Some(control_open_tx)));
        let message_tx_for_open = message_tx.clone();
        channel.on_open(Box::new(move || {
            let control_open_tx = Arc::clone(&control_open_tx);
            let message_tx = message_tx_for_open.clone();
            Box::pin(async move {
                let _ = message_tx.send(ClientRuntimeMessage::Notice {
                    category: format!("data_channel_open_{label}"),
                    detail: format!("{label} data channel opened"),
                });
                let mut control_open_tx = control_open_tx.lock().await;
                if let Some(control_open_tx) = control_open_tx.take() {
                    let _ = control_open_tx.send(());
                }
            })
        }));
    }

    channel.on_message(Box::new(move |message: DataChannelMessage| {
        let message_tx = message_tx.clone();
        Box::pin(async move {
            if message.is_string {
                let _ = message_tx.send(ClientRuntimeMessage::Notice {
                    category: format!("data_channel_protocol_error_{label}"),
                    detail: format!("{label} channel delivered a string frame"),
                });
                return;
            }
            match ServerControlEvent::decode_packet(&message.data) {
                Ok((_, event)) => {
                    let _ = message_tx.send(ClientRuntimeMessage::ServerEvent(event));
                }
                Err(error) => {
                    let _ = message_tx.send(ClientRuntimeMessage::Notice {
                        category: format!("data_channel_decode_error_{label}"),
                        detail: error.to_string(),
                    });
                }
            }
        })
    }));
}

fn install_ignore_channel(channel: &Arc<RTCDataChannel>) {
    let channel = Arc::clone(channel);
    channel.on_message(Box::new(|_message: DataChannelMessage| Box::pin(async {})));
}

fn install_peer_callbacks(
    peer: &Arc<RTCPeerConnection>,
    message_tx: mpsc::UnboundedSender<ClientRuntimeMessage>,
) {
    peer.on_peer_connection_state_change(Box::new(move |state| {
        let message_tx = message_tx.clone();
        Box::pin(async move {
            let category = match state {
                RTCPeerConnectionState::Connected => "peer_state_connected",
                RTCPeerConnectionState::Disconnected => "peer_state_disconnected",
                RTCPeerConnectionState::Failed => "peer_state_failed",
                RTCPeerConnectionState::Closed => "peer_state_closed",
                _ => "peer_state_changed",
            };
            let _ = message_tx.send(ClientRuntimeMessage::Notice {
                category: String::from(category),
                detail: state.to_string(),
            });
        })
    }));
}

async fn apply_server_signal(
    peer: &Arc<RTCPeerConnection>,
    message_tx: &mpsc::UnboundedSender<ClientRuntimeMessage>,
    signal_message: ServerSignalMessage,
) -> ProbeResult<()> {
    match signal_message {
        ServerSignalMessage::Hello { .. } => {
            return Err(ProbeError::new(
                "received a duplicate signaling hello after initialization",
            ));
        }
        ServerSignalMessage::SessionDescription { description } => {
            if description.sdp_type != "answer" {
                return Err(ProbeError::new(format!(
                    "unexpected remote SDP type {}",
                    description.sdp_type
                )));
            }
            let remote_description = description
                .to_rtc_description()
                .map_err(|error| ProbeError::new(format!("remote answer parse failed: {error}")))?;
            peer.set_remote_description(remote_description)
                .await
                .map_err(|error| ProbeError::new(format!("remote answer apply failed: {error}")))?;
        }
        ServerSignalMessage::IceCandidate { candidate } => {
            let candidate_init = candidate.to_rtc_candidate_init().map_err(|error| {
                ProbeError::new(format!("remote ICE candidate parse failed: {error}"))
            })?;
            peer.add_ice_candidate(candidate_init)
                .await
                .map_err(|error| {
                    ProbeError::new(format!("remote ICE candidate apply failed: {error}"))
                })?;
        }
        ServerSignalMessage::Error { message } => {
            let _ = message_tx.send(ClientRuntimeMessage::ServerEvent(
                ServerControlEvent::Error { message },
            ));
        }
    }
    Ok(())
}
