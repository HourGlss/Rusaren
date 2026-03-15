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
use game_sim::COMBAT_FRAME_MS;
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

struct HelloMessage {
    ice_servers: Vec<WebRtcIceServerConfig>,
}

struct WebRtcClient {
    signal_tx: mpsc::UnboundedSender<ClientSignalMessage>,
    event_rx: mpsc::UnboundedReceiver<ServerControlEvent>,
    peer: Arc<RTCPeerConnection>,
    control: Arc<RTCDataChannel>,
    input: Arc<RTCDataChannel>,
    next_control_seq: u32,
    next_input_seq: u32,
}

impl WebRtcClient {
    async fn connect(base_url: &str, player_name_raw: &str) -> Self {
        let signal_url = bootstrap_signal_url(base_url).await;
        let (stream, _) = connect_async(&signal_url)
            .await
            .expect("signaling websocket should connect");
        let (mut signal_writer, mut signal_reader) = stream.split();
        let hello = recv_hello(&mut signal_reader).await;

        let peer = Arc::new(create_client_peer(&hello.ice_servers).await);
        let (signal_tx, mut signal_rx) = mpsc::unbounded_channel::<ClientSignalMessage>();
        let (event_tx, event_rx) = mpsc::unbounded_channel::<ServerControlEvent>();
        let (control_open_tx, control_open_rx) = oneshot::channel::<()>();

        tokio::spawn(async move {
            while let Some(message) = signal_rx.recv().await {
                let text = serde_json::to_string(&message).expect("signal messages should encode");
                if signal_writer
                    .send(ClientMessage::Text(text.into()))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        let control = create_client_channel(Arc::clone(&peer), "control", control_init())
            .await
            .expect("control data channel should be created");
        let input = create_client_channel(Arc::clone(&peer), "input", unreliable_init(1))
            .await
            .expect("input data channel should be created");
        let snapshot = create_client_channel(Arc::clone(&peer), "snapshot", unreliable_init(2))
            .await
            .expect("snapshot data channel should be created");

        install_event_channel(&control, event_tx.clone(), Some(control_open_tx));
        install_event_channel(&snapshot, event_tx.clone(), None);
        install_ignore_channel(&input);

        let signal_tx_for_candidates = signal_tx.clone();
        peer.on_ice_candidate(Box::new(move |candidate: Option<RTCIceCandidate>| {
            let signal_tx = signal_tx_for_candidates.clone();
            Box::pin(async move {
                let Some(candidate) = candidate else {
                    return;
                };
                let candidate_init = match candidate.to_json() {
                    Ok(candidate_init) => candidate_init,
                    Err(error) => panic!("local ICE candidate should serialize: {error}"),
                };
                let _ = signal_tx.send(ClientSignalMessage::IceCandidate {
                    candidate: SignalingIceCandidate::from_rtc_candidate_init(&candidate_init),
                });
            })
        }));

        let peer_for_reader = Arc::clone(&peer);
        tokio::spawn(async move {
            while let Some(message_result) = signal_reader.next().await {
                let message = match message_result {
                    Ok(message) => message,
                    Err(error) => panic!("signaling websocket should stay readable: {error}"),
                };
                match message {
                    ClientMessage::Text(text) => {
                        let signal_message = serde_json::from_str::<ServerSignalMessage>(&text)
                            .expect("server signal JSON should decode");
                        apply_server_signal(&peer_for_reader, &event_tx, signal_message).await;
                    }
                    ClientMessage::Binary(_) => {
                        panic!("server signaling should not send binary websocket frames");
                    }
                    ClientMessage::Close(_) => break,
                    ClientMessage::Ping(_) | ClientMessage::Pong(_) | ClientMessage::Frame(_) => {}
                }
            }
        });

        let offer = peer
            .create_offer(None)
            .await
            .expect("webrtc offer should be created");
        peer.set_local_description(offer.clone())
            .await
            .expect("local offer should be applied");
        let local_offer = peer.local_description().await.unwrap_or(offer);
        signal_tx
            .send(ClientSignalMessage::SessionDescription {
                description: SignalingSessionDescription::from_rtc_description(&local_offer),
            })
            .expect("local offer should be sent");

        tokio::time::timeout(Duration::from_secs(15), control_open_rx)
            .await
            .expect("control data channel should open in time")
            .expect("control open notifier should succeed");

        let mut client = Self {
            signal_tx,
            event_rx,
            peer,
            control,
            input,
            next_control_seq: 1,
            next_input_seq: 1,
        };
        client
            .send_command(ClientControlCommand::Connect {
                player_name: player_name(player_name_raw),
            })
            .await;
        client
    }

    async fn send_command(&mut self, command: ClientControlCommand) {
        let packet = command
            .encode_packet(self.next_control_seq, 0)
            .expect("command packet should encode");
        self.control
            .send(&Bytes::from(packet))
            .await
            .expect("command packet should send");
        self.next_control_seq += 1;
    }

    async fn recv_event(&mut self) -> ServerControlEvent {
        let event = tokio::time::timeout(Duration::from_secs(15), self.event_rx.recv())
            .await
            .expect("event should arrive in time");
        event.expect("event channel should stay open")
    }

    async fn recv_events_until<F>(
        &mut self,
        max_events: usize,
        mut predicate: F,
    ) -> Vec<ServerControlEvent>
    where
        F: FnMut(&ServerControlEvent) -> bool,
    {
        let mut events = Vec::new();
        for _ in 0..max_events {
            let event = self.recv_event().await;
            let done = predicate(&event);
            events.push(event);
            if done {
                return events;
            }
        }

        panic!("expected predicate to succeed within {max_events} events, got {events:?}");
    }

    async fn send_input(&mut self, frame: ValidatedInputFrame, sim_tick: u32) {
        let packet = frame
            .encode_packet(self.next_input_seq, sim_tick)
            .expect("input frame should encode");
        self.input
            .send(&Bytes::from(packet))
            .await
            .expect("input packet should send");
        self.next_input_seq += 1;
    }

    async fn close(self) {
        let _ = self.signal_tx.send(ClientSignalMessage::Bye);
        let _ = self.peer.close().await;
    }
}

fn player_name(raw: &str) -> PlayerName {
    PlayerName::new(raw).expect("valid player name")
}

fn slot_one_cast_input(client_input_tick: u32) -> ValidatedInputFrame {
    ValidatedInputFrame::new(client_input_tick, 0, 0, 0, 0, BUTTON_CAST, 1)
        .expect("slot one cast frame should be valid")
}

async fn recv_hello(
    signal_reader: &mut futures_util::stream::SplitStream<SignalStream>,
) -> HelloMessage {
    while let Some(message_result) = signal_reader.next().await {
        match message_result.expect("signaling websocket should stay readable") {
            ClientMessage::Text(text) => {
                let message = serde_json::from_str::<ServerSignalMessage>(&text)
                    .expect("server signal JSON should decode");
                if let ServerSignalMessage::Hello {
                    protocol_version,
                    ice_servers,
                    channels,
                } = message
                {
                    assert_eq!(protocol_version, game_net::PROTOCOL_VERSION);
                    assert_eq!(channels.control, 0);
                    assert_eq!(channels.input, 1);
                    assert_eq!(channels.snapshot, 2);
                    return HelloMessage { ice_servers };
                }
            }
            ClientMessage::Binary(_) => panic!("hello must be delivered as websocket text"),
            ClientMessage::Close(_) => panic!("signaling websocket closed before hello"),
            ClientMessage::Ping(_) | ClientMessage::Pong(_) | ClientMessage::Frame(_) => {}
        }
    }

    panic!("signaling websocket ended before hello")
}

async fn create_client_peer(ice_servers: &[WebRtcIceServerConfig]) -> RTCPeerConnection {
    let mut media_engine = MediaEngine::default();
    media_engine
        .register_default_codecs()
        .expect("default codecs should register");
    let api = APIBuilder::new().with_media_engine(media_engine).build();
    api.new_peer_connection(RTCConfiguration {
        ice_servers: ice_servers
            .iter()
            .map(WebRtcIceServerConfig::to_rtc_ice_server)
            .collect(),
        ..Default::default()
    })
    .await
    .expect("client peer connection should be created")
}

async fn create_client_channel(
    peer: Arc<RTCPeerConnection>,
    label: &str,
    init: RTCDataChannelInit,
) -> Result<Arc<RTCDataChannel>, webrtc::Error> {
    peer.create_data_channel(label, Some(init)).await
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
    event_tx: mpsc::UnboundedSender<ServerControlEvent>,
    control_open_tx: Option<oneshot::Sender<()>>,
) {
    let channel = Arc::clone(channel);
    if let Some(control_open_tx) = control_open_tx {
        let control_open_tx = Arc::new(tokio::sync::Mutex::new(Some(control_open_tx)));
        channel.on_open(Box::new(move || {
            let control_open_tx = Arc::clone(&control_open_tx);
            Box::pin(async move {
                let mut control_open_tx = control_open_tx.lock().await;
                if let Some(control_open_tx) = control_open_tx.take() {
                    let _ = control_open_tx.send(());
                }
            })
        }));
    }

    channel.on_message(Box::new(move |message: DataChannelMessage| {
        let event_tx = event_tx.clone();
        Box::pin(async move {
            assert!(
                !message.is_string,
                "server event channels must send binary packets"
            );
            let (_, event) = ServerControlEvent::decode_packet(&message.data)
                .expect("server event packet should decode");
            let _ = event_tx.send(event);
        })
    }));
}

fn install_ignore_channel(channel: &Arc<RTCDataChannel>) {
    let channel = Arc::clone(channel);
    channel.on_message(Box::new(|_message: DataChannelMessage| Box::pin(async {})));
}

async fn apply_server_signal(
    peer: &Arc<RTCPeerConnection>,
    event_tx: &mpsc::UnboundedSender<ServerControlEvent>,
    signal_message: ServerSignalMessage,
) {
    match signal_message {
        ServerSignalMessage::Hello { .. } => {
            panic!("received duplicate hello after client initialization");
        }
        ServerSignalMessage::SessionDescription { description } => {
            assert_eq!(description.sdp_type, "answer");
            let remote_description = description
                .to_rtc_description()
                .expect("server answer should parse");
            peer.set_remote_description(remote_description)
                .await
                .expect("server answer should apply");
        }
        ServerSignalMessage::IceCandidate { candidate } => {
            let candidate_init = candidate
                .to_rtc_candidate_init()
                .expect("server ICE candidate should parse");
            peer.add_ice_candidate(candidate_init)
                .await
                .expect("server ICE candidate should apply");
        }
        ServerSignalMessage::Error { message } => {
            let _ = event_tx.send(ServerControlEvent::Error { message });
        }
    }
}

fn temp_record_store_path() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after the unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("rusaren-realtime-webrtc-{unique}.tsv"))
}

fn repo_content_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("content")
}

fn temp_web_client_root(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after the unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("rusaren-webrtc-web-root-{prefix}-{unique}"));
    std::fs::create_dir_all(&root).expect("temporary web client root should be created");
    root
}

fn http_authority(base_url: &str) -> String {
    base_url
        .trim_start_matches("ws://")
        .trim_start_matches("wss://")
        .to_string()
}

async fn http_get(base_url: &str, path: &str) -> (u16, String) {
    let authority = http_authority(base_url);
    let mut stream = tokio::net::TcpStream::connect(&authority)
        .await
        .expect("http connection should succeed");
    let request = format!("GET {path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("http request should be written");

    let mut raw_response = Vec::new();
    stream
        .read_to_end(&mut raw_response)
        .await
        .expect("http response should be readable");

    let response =
        String::from_utf8(raw_response).expect("http response should be valid utf8 for tests");
    let (head, body) = response
        .split_once("\r\n\r\n")
        .expect("http response should contain a header/body split");
    let status_line = head.lines().next().expect("http status line should exist");
    let status_code = status_line
        .split_whitespace()
        .nth(1)
        .expect("http status line should contain a status code")
        .parse::<u16>()
        .expect("http status code should be numeric");

    (status_code, body.to_string())
}

async fn bootstrap_signal_url(base_url: &str) -> String {
    let (status_code, body) = http_get(base_url, "/session/bootstrap").await;
    assert_eq!(status_code, 200, "session bootstrap should return HTTP 200");
    let payload = serde_json::from_str::<Value>(&body).expect("bootstrap JSON should decode");
    let token = payload
        .get("token")
        .and_then(Value::as_str)
        .expect("bootstrap JSON should include a token");
    format!("{base_url}/ws?token={token}")
}

async fn start_server_fast() -> (game_api::DevServerHandle, String) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let server = spawn_dev_server_with_options(
        listener,
        DevServerOptions {
            tick_interval: Duration::from_millis(10),
            simulation_step_ms: COMBAT_FRAME_MS,
            record_store_path: temp_record_store_path(),
            content_root: repo_content_root(),
            web_client_root: temp_web_client_root("fast"),
            observability: None,
            webrtc: WebRtcRuntimeConfig::default(),
        },
    )
    .await
    .expect("server should spawn");
    let base_url = format!("ws://{}", server.local_addr());
    (server, base_url)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn webrtc_transport_connects_and_streams_control_plus_snapshot_events() {
    let (server, base_url) = start_server_fast().await;
    let mut alice = WebRtcClient::connect(&base_url, "Alice").await;
    let mut bob = WebRtcClient::connect(&base_url, "Bob").await;

    let alice_connect_events: Vec<ServerControlEvent> = alice
        .recv_events_until(3, |event| {
            matches!(event, ServerControlEvent::LobbyDirectorySnapshot { .. })
        })
        .await;
    let bob_connect_events: Vec<ServerControlEvent> = bob
        .recv_events_until(3, |event| {
            matches!(event, ServerControlEvent::LobbyDirectorySnapshot { .. })
        })
        .await;
    assert!(alice_connect_events
        .iter()
        .any(|event| matches!(event, ServerControlEvent::Connected { .. })));
    assert!(bob_connect_events
        .iter()
        .any(|event| matches!(event, ServerControlEvent::Connected { .. })));

    alice
        .send_command(ClientControlCommand::CreateGameLobby)
        .await;
    let created_events: Vec<ServerControlEvent> = alice
        .recv_events_until(4, |event| {
            matches!(event, ServerControlEvent::GameLobbyCreated { .. })
        })
        .await;
    let lobby_id = created_events
        .iter()
        .find_map(|event| match event {
            ServerControlEvent::GameLobbyCreated { lobby_id } => Some(*lobby_id),
            _ => None,
        })
        .expect("create flow should include GameLobbyCreated");
    let _ = alice
        .recv_events_until(4, |event| {
            matches!(event, ServerControlEvent::GameLobbySnapshot { .. })
        })
        .await;

    bob.send_command(ClientControlCommand::JoinGameLobby { lobby_id })
        .await;
    let _ = alice
        .recv_events_until(4, |event| {
            matches!(event, ServerControlEvent::GameLobbySnapshot { .. })
        })
        .await;
    let _ = bob
        .recv_events_until(4, |event| {
            matches!(event, ServerControlEvent::GameLobbySnapshot { .. })
        })
        .await;

    alice
        .send_command(ClientControlCommand::SelectTeam {
            team: TeamSide::TeamA,
        })
        .await;
    bob.send_command(ClientControlCommand::SelectTeam {
        team: TeamSide::TeamB,
    })
    .await;
    let _ = alice.recv_event().await;
    let _ = alice.recv_event().await;
    let _ = bob.recv_event().await;
    let _ = bob.recv_event().await;

    alice
        .send_command(ClientControlCommand::SetReady {
            ready: ReadyState::Ready,
        })
        .await;
    bob.send_command(ClientControlCommand::SetReady {
        ready: ReadyState::Ready,
    })
    .await;

    let alice_launch_events: Vec<ServerControlEvent> = alice
        .recv_events_until(24, |event| {
            matches!(event, ServerControlEvent::ArenaStateSnapshot { .. })
        })
        .await;
    let bob_launch_events: Vec<ServerControlEvent> = bob
        .recv_events_until(24, |event| {
            matches!(event, ServerControlEvent::ArenaStateSnapshot { .. })
        })
        .await;
    assert!(alice_launch_events
        .iter()
        .any(|event| matches!(event, ServerControlEvent::MatchStarted { .. })));
    assert!(bob_launch_events
        .iter()
        .any(|event| matches!(event, ServerControlEvent::MatchStarted { .. })));
    assert!(alice_launch_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::ArenaStateSnapshot { snapshot }
            if snapshot.players.iter().any(|player| player.player_name.as_str() == "Alice")
                && !snapshot.players.iter().any(|player| player.player_name.as_str() == "Bob")
                && !snapshot.obstacles.is_empty()
                && !snapshot.visible_tiles.is_empty()
    )));
    assert!(bob_launch_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::ArenaStateSnapshot { snapshot }
            if snapshot.players.iter().any(|player| player.player_name.as_str() == "Bob")
                && !snapshot.players.iter().any(|player| player.player_name.as_str() == "Alice")
                && !snapshot.obstacles.is_empty()
                && !snapshot.visible_tiles.is_empty()
    )));

    alice
        .send_command(ClientControlCommand::ChooseSkill {
            tree: SkillTree::Rogue,
            tier: 1,
        })
        .await;
    bob.send_command(ClientControlCommand::ChooseSkill {
        tree: SkillTree::Warrior,
        tier: 1,
    })
    .await;
    let _ = alice
        .recv_events_until(10, |event| {
            matches!(event, ServerControlEvent::PreCombatStarted { .. })
        })
        .await;
    let _ = bob
        .recv_events_until(10, |event| {
            matches!(event, ServerControlEvent::PreCombatStarted { .. })
        })
        .await;
    let _ = alice
        .recv_events_until(8, |event| {
            matches!(event, ServerControlEvent::CombatStarted)
        })
        .await;
    let _ = bob
        .recv_events_until(8, |event| {
            matches!(event, ServerControlEvent::CombatStarted)
        })
        .await;

    alice.send_input(slot_one_cast_input(101), 101).await;
    let alice_combat_events = alice
        .recv_events_until(24, |event| {
            matches!(
                event,
                ServerControlEvent::ArenaDeltaSnapshot { snapshot }
                    if snapshot.players.iter().any(|player| player.mana < player.max_mana)
            )
        })
        .await;
    let bob_combat_events = bob
        .recv_events_until(24, |event| {
            matches!(
                event,
                ServerControlEvent::ArenaDeltaSnapshot { snapshot }
                    if snapshot.players.iter().any(|player| player.player_name.as_str() == "Bob")
            )
        })
        .await;
    assert!(alice_combat_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::ArenaDeltaSnapshot { snapshot }
            if snapshot.players.iter().any(|player| player.mana < player.max_mana)
    )));
    assert!(bob_combat_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::ArenaDeltaSnapshot { snapshot }
            if snapshot.players.iter().any(|player| player.player_name.as_str() == "Bob")
    )));

    alice.close().await;
    bob.close().await;
    server.shutdown().await;
}
