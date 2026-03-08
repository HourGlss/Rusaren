use std::collections::{BTreeMap, VecDeque};

use game_domain::{DomainError, LobbyId, PlayerId, PlayerName, ReadyState, SkillChoice, TeamSide};
use game_net::{ClientControlCommand, PacketError, ServerControlEvent, ValidatedInputFrame};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConnectionId(u64);

impl ConnectionId {
    pub fn new(value: u64) -> Result<Self, DomainError> {
        if value == 0 {
            return Err(DomainError::IdMustBeNonZero("connection_id"));
        }

        Ok(Self(value))
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

pub trait AppTransport {
    fn recv_from_client(&mut self) -> Option<(ConnectionId, Vec<u8>)>;
    fn send_to_client(&mut self, connection_id: ConnectionId, packet: Vec<u8>);
}

#[derive(Default)]
pub struct InMemoryTransport {
    server_inbox: VecDeque<(ConnectionId, Vec<u8>)>,
    client_inboxes: BTreeMap<ConnectionId, VecDeque<Vec<u8>>>,
}

impl InMemoryTransport {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn send_from_client(&mut self, connection_id: ConnectionId, packet: Vec<u8>) {
        self.server_inbox.push_back((connection_id, packet));
    }

    #[must_use]
    pub fn drain_client_packets(&mut self, connection_id: ConnectionId) -> Vec<Vec<u8>> {
        match self.client_inboxes.remove(&connection_id) {
            Some(queue) => queue.into_iter().collect(),
            None => Vec::new(),
        }
    }
}

impl AppTransport for InMemoryTransport {
    fn recv_from_client(&mut self) -> Option<(ConnectionId, Vec<u8>)> {
        self.server_inbox.pop_front()
    }

    fn send_to_client(&mut self, connection_id: ConnectionId, packet: Vec<u8>) {
        self.client_inboxes
            .entry(connection_id)
            .or_default()
            .push_back(packet);
    }
}

pub struct HeadlessClient {
    connection_id: ConnectionId,
    player_name: PlayerName,
    assigned_player_id: Option<PlayerId>,
    control_seq: u32,
    input_seq: u32,
}

impl HeadlessClient {
    #[must_use]
    pub fn new(connection_id: ConnectionId, player_name: PlayerName) -> Self {
        Self {
            connection_id,
            player_name,
            assigned_player_id: None,
            control_seq: 0,
            input_seq: 0,
        }
    }

    #[must_use]
    pub const fn connection_id(&self) -> ConnectionId {
        self.connection_id
    }

    #[must_use]
    pub const fn player_id(&self) -> Option<PlayerId> {
        self.assigned_player_id
    }

    pub fn connect(&mut self, transport: &mut InMemoryTransport) -> Result<(), PacketError> {
        self.send_control(
            transport,
            ClientControlCommand::Connect {
                player_name: self.player_name.clone(),
            },
        )
    }

    pub fn create_game_lobby(
        &mut self,
        transport: &mut InMemoryTransport,
    ) -> Result<(), PacketError> {
        self.send_control(transport, ClientControlCommand::CreateGameLobby)
    }

    pub fn join_game_lobby(
        &mut self,
        transport: &mut InMemoryTransport,
        lobby_id: LobbyId,
    ) -> Result<(), PacketError> {
        self.send_control(transport, ClientControlCommand::JoinGameLobby { lobby_id })
    }

    pub fn leave_game_lobby(
        &mut self,
        transport: &mut InMemoryTransport,
    ) -> Result<(), PacketError> {
        self.send_control(transport, ClientControlCommand::LeaveGameLobby)
    }

    pub fn select_team(
        &mut self,
        transport: &mut InMemoryTransport,
        team: TeamSide,
    ) -> Result<(), PacketError> {
        self.send_control(transport, ClientControlCommand::SelectTeam { team })
    }

    pub fn set_ready(
        &mut self,
        transport: &mut InMemoryTransport,
        ready: ReadyState,
    ) -> Result<(), PacketError> {
        self.send_control(transport, ClientControlCommand::SetReady { ready })
    }

    pub fn choose_skill(
        &mut self,
        transport: &mut InMemoryTransport,
        choice: SkillChoice,
    ) -> Result<(), PacketError> {
        self.send_control(
            transport,
            ClientControlCommand::ChooseSkill {
                tree: choice.tree,
                tier: choice.tier,
            },
        )
    }

    pub fn quit_to_central_lobby(
        &mut self,
        transport: &mut InMemoryTransport,
    ) -> Result<(), PacketError> {
        self.send_control(transport, ClientControlCommand::QuitToCentralLobby)
    }

    pub fn send_input(
        &mut self,
        transport: &mut InMemoryTransport,
        frame: ValidatedInputFrame,
        sim_tick: u32,
    ) -> Result<(), PacketError> {
        self.input_seq = self.input_seq.saturating_add(1);
        transport.send_from_client(
            self.connection_id,
            frame.encode_packet(self.input_seq, sim_tick)?,
        );
        Ok(())
    }

    pub fn drain_events(
        &mut self,
        transport: &mut InMemoryTransport,
    ) -> Result<Vec<ServerControlEvent>, PacketError> {
        let events = transport
            .drain_client_packets(self.connection_id)
            .into_iter()
            .map(|packet| ServerControlEvent::decode_packet(&packet).map(|(_, event)| event))
            .collect::<Result<Vec<_>, _>>()?;

        for event in &events {
            if let ServerControlEvent::Connected { player_id, .. } = event {
                self.assigned_player_id = Some(*player_id);
            }
        }

        Ok(events)
    }

    fn send_control(
        &mut self,
        transport: &mut InMemoryTransport,
        command: ClientControlCommand,
    ) -> Result<(), PacketError> {
        self.control_seq = self.control_seq.saturating_add(1);
        transport.send_from_client(
            self.connection_id,
            command.encode_packet(self.control_seq, 0)?,
        );
        Ok(())
    }
}
