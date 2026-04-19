use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

const WEBRTC_CONNECT_ATTEMPTS: usize = 3;
const WEBRTC_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

struct HelloMessage {
    ice_servers: Vec<WebRtcIceServerConfig>,
}

pub(super) struct WebRtcClient {
    signal_tx: mpsc::UnboundedSender<ClientSignalMessage>,
    event_rx: mpsc::UnboundedReceiver<ServerControlEvent>,
    peer: Arc<RTCPeerConnection>,
    control: Arc<RTCDataChannel>,
    input: Arc<RTCDataChannel>,
    next_control_seq: u32,
    next_input_seq: u32,
}

impl WebRtcClient {
    pub(super) async fn connect(base_url: &str, player_name_raw: &str) -> Self {
        let mut failures = Vec::new();
        for attempt in 1..=WEBRTC_CONNECT_ATTEMPTS {
            match Self::try_connect(base_url, player_name_raw).await {
                Ok(client) => return client,
                Err(error) => {
                    failures.push(format!("attempt {attempt}: {error}"));
                    if attempt < WEBRTC_CONNECT_ATTEMPTS {
                        tokio::time::sleep(Duration::from_millis(250 * attempt as u64)).await;
                    }
                }
            }
        }

        panic!(
            "webrtc client should connect within {WEBRTC_CONNECT_ATTEMPTS} attempts: {}",
            failures.join("; ")
        );
    }

    async fn try_connect(base_url: &str, player_name_raw: &str) -> Result<Self, String> {
        let signal_url = bootstrap_signal_url(base_url).await;
        let (stream, _) = connect_async(&signal_url)
            .await
            .map_err(|error| format!("signaling websocket should connect: {error}"))?;
        let (mut signal_writer, mut signal_reader) = stream.split();
        let hello = recv_hello(&mut signal_reader).await?;

        let peer = Arc::new(create_client_peer(&hello.ice_servers).await?);
        let (signal_tx, mut signal_rx) = mpsc::unbounded_channel::<ClientSignalMessage>();
        let (event_tx, event_rx) = mpsc::unbounded_channel::<ServerControlEvent>();
        let (control_open_tx, control_open_rx) = oneshot::channel::<()>();
        let (startup_error_tx, mut startup_error_rx) = mpsc::unbounded_channel::<String>();
        let startup_complete = Arc::new(AtomicBool::new(false));
        let startup_error_tx_for_writer = startup_error_tx.clone();

        tokio::spawn(async move {
            while let Some(message) = signal_rx.recv().await {
                let text = match serde_json::to_string(&message) {
                    Ok(text) => text,
                    Err(error) => {
                        let _ = startup_error_tx_for_writer
                            .send(format!("signal messages should encode: {error}"));
                        break;
                    }
                };
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
            .map_err(|error| format!("control data channel should be created: {error}"))?;
        let input = create_client_channel(Arc::clone(&peer), "input", unreliable_init(1))
            .await
            .map_err(|error| format!("input data channel should be created: {error}"))?;
        let snapshot = create_client_channel(Arc::clone(&peer), "snapshot", unreliable_init(2))
            .await
            .map_err(|error| format!("snapshot data channel should be created: {error}"))?;

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
        let startup_complete_for_reader = Arc::clone(&startup_complete);
        let startup_error_tx_for_reader = startup_error_tx.clone();
        tokio::spawn(async move {
            while let Some(message_result) = signal_reader.next().await {
                let message = match message_result {
                    Ok(message) => message,
                    Err(error) => {
                        if !startup_complete_for_reader.load(Ordering::Relaxed) {
                            let _ = startup_error_tx_for_reader.send(format!(
                                "signaling websocket should stay readable until transport startup completes: {error}"
                            ));
                        }
                        break;
                    }
                };
                let result = match message {
                    ClientMessage::Text(text) => {
                        let signal_message = serde_json::from_str::<ServerSignalMessage>(&text)
                            .map_err(|error| format!("server signal JSON should decode: {error}"));
                        match signal_message {
                            Ok(signal_message) => {
                                apply_server_signal(&peer_for_reader, &event_tx, signal_message)
                                    .await
                            }
                            Err(error) => Err(error),
                        }
                    }
                    ClientMessage::Binary(_) => Err(String::from(
                        "server signaling should not send binary websocket frames",
                    )),
                    ClientMessage::Close(_) => break,
                    ClientMessage::Ping(_) | ClientMessage::Pong(_) | ClientMessage::Frame(_) => {
                        Ok(())
                    }
                };
                if let Err(error) = result {
                    if !startup_complete_for_reader.load(Ordering::Relaxed) {
                        let _ = startup_error_tx_for_reader.send(error);
                    }
                    break;
                }
            }
        });

        let offer = peer
            .create_offer(None)
            .await
            .map_err(|error| format!("webrtc offer should be created: {error}"))?;
        peer.set_local_description(offer.clone())
            .await
            .map_err(|error| format!("local offer should be applied: {error}"))?;
        let local_offer = peer.local_description().await.unwrap_or(offer);
        signal_tx
            .send(ClientSignalMessage::SessionDescription {
                description: SignalingSessionDescription::from_rtc_description(&local_offer),
            })
            .map_err(|_| String::from("local offer should be sent"))?;

        tokio::select! {
            startup_error = startup_error_rx.recv() => {
                let message = startup_error.unwrap_or_else(|| {
                    String::from("webrtc startup ended before the control data channel opened")
                });
                return Err(message);
            }
            open_result = tokio::time::timeout(WEBRTC_CONNECT_TIMEOUT, control_open_rx) => {
                open_result
                    .map_err(|_| {
                        format!(
                            "control data channel should open within {} seconds",
                            WEBRTC_CONNECT_TIMEOUT.as_secs()
                        )
                    })?
                    .map_err(|_| String::from("control open notifier should succeed"))?;
            }
        }
        startup_complete.store(true, Ordering::Relaxed);

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
            .try_send_command(ClientControlCommand::Connect {
                player_name: player_name(player_name_raw),
            })
            .await?;
        Ok(client)
    }

    pub(super) async fn send_command(&mut self, command: ClientControlCommand) {
        self.try_send_command(command)
            .await
            .expect("command packet should send");
    }

    async fn try_send_command(&mut self, command: ClientControlCommand) -> Result<(), String> {
        let packet = command
            .encode_packet(self.next_control_seq, 0)
            .map_err(|error| format!("command packet should encode: {error}"))?;
        self.control
            .send(&Bytes::from(packet))
            .await
            .map_err(|error| format!("command packet should send: {error}"))?;
        self.next_control_seq += 1;
        Ok(())
    }

    pub(super) async fn recv_event(&mut self) -> ServerControlEvent {
        let event = tokio::time::timeout(Duration::from_secs(15), self.event_rx.recv())
            .await
            .expect("event should arrive in time");
        event.expect("event channel should stay open")
    }

    pub(super) async fn recv_events_until<F>(
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

    pub(super) async fn send_input(&mut self, frame: ValidatedInputFrame, sim_tick: u32) {
        let packet = frame
            .encode_packet(self.next_input_seq, sim_tick)
            .expect("input frame should encode");
        self.input
            .send(&Bytes::from(packet))
            .await
            .expect("input packet should send");
        self.next_input_seq += 1;
    }

    pub(super) async fn close(self) {
        let _ = self.signal_tx.send(ClientSignalMessage::Bye);
        let _ = self.peer.close().await;
    }
}

pub(super) fn player_name(raw: &str) -> PlayerName {
    PlayerName::new(raw).expect("valid player name")
}

pub(super) fn slot_one_cast_input(client_input_tick: u32) -> ValidatedInputFrame {
    ValidatedInputFrame::new(client_input_tick, 0, 0, 0, 0, BUTTON_CAST, 1)
        .expect("slot one cast frame should be valid")
}

async fn recv_hello(
    signal_reader: &mut futures_util::stream::SplitStream<SignalStream>,
) -> Result<HelloMessage, String> {
    while let Some(message_result) = signal_reader.next().await {
        match message_result
            .map_err(|error| format!("signaling websocket should stay readable: {error}"))?
        {
            ClientMessage::Text(text) => {
                let message = serde_json::from_str::<ServerSignalMessage>(&text)
                    .map_err(|error| format!("server signal JSON should decode: {error}"))?;
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
                    return Ok(HelloMessage { ice_servers });
                }
            }
            ClientMessage::Binary(_) => {
                return Err(String::from("hello must be delivered as websocket text"));
            }
            ClientMessage::Close(_) => {
                return Err(String::from("signaling websocket closed before hello"));
            }
            ClientMessage::Ping(_) | ClientMessage::Pong(_) | ClientMessage::Frame(_) => {}
        }
    }

    Err(String::from("signaling websocket ended before hello"))
}

async fn create_client_peer(
    ice_servers: &[WebRtcIceServerConfig],
) -> Result<RTCPeerConnection, String> {
    let mut media_engine = MediaEngine::default();
    media_engine
        .register_default_codecs()
        .map_err(|error| format!("default codecs should register: {error}"))?;
    let api = APIBuilder::new().with_media_engine(media_engine).build();
    api.new_peer_connection(RTCConfiguration {
        ice_servers: ice_servers
            .iter()
            .map(WebRtcIceServerConfig::to_rtc_ice_server)
            .collect(),
        ..Default::default()
    })
    .await
    .map_err(|error| format!("client peer connection should be created: {error}"))
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
) -> Result<(), String> {
    match signal_message {
        ServerSignalMessage::Hello { .. } => {
            return Err(String::from(
                "received duplicate hello after client initialization",
            ));
        }
        ServerSignalMessage::SessionDescription { description } => {
            assert_eq!(description.sdp_type, "answer");
            let remote_description = description
                .to_rtc_description()
                .map_err(|error| format!("server answer should parse: {error}"))?;
            peer.set_remote_description(remote_description)
                .await
                .map_err(|error| format!("server answer should apply: {error}"))?;
        }
        ServerSignalMessage::IceCandidate { candidate } => {
            let candidate_init = candidate
                .to_rtc_candidate_init()
                .map_err(|error| format!("server ICE candidate should parse: {error}"))?;
            peer.add_ice_candidate(candidate_init)
                .await
                .map_err(|error| format!("server ICE candidate should apply: {error}"))?;
        }
        ServerSignalMessage::Error { message } => {
            let _ = event_tx.send(ServerControlEvent::Error { message });
        }
    }
    Ok(())
}
