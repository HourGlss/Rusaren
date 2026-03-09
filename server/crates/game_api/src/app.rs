#![allow(
    clippy::manual_let_else,
    clippy::map_unwrap_or,
    clippy::needless_pass_by_value,
    clippy::semicolon_if_nothing_returned,
    clippy::single_match_else
)]

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use game_content::GameContent;
use game_domain::{
    LobbyId, MatchId, MatchOutcome, PlayerId, PlayerName, PlayerRecord, ReadyState, SkillChoice,
    TeamAssignment, TeamSide,
};
use game_lobby::{Lobby, LobbyEvent, LobbyPhase};
use game_match::{MatchConfig, MatchEvent, MatchPhase, MatchSession};
use game_net::{
    ArenaEffectKind, ArenaEffectSnapshot, ArenaObstacleKind, ArenaObstacleSnapshot,
    ArenaPlayerSnapshot, ArenaProjectileSnapshot, ArenaStateSnapshot, ClientControlCommand,
    LobbyDirectoryEntry, LobbySnapshotPhase, LobbySnapshotPlayer, SequenceTracker,
    ServerControlEvent, ValidatedInputFrame, BUTTON_CAST, BUTTON_PRIMARY, BUTTON_QUIT_TO_LOBBY,
};
use game_sim::{
    ArenaEffect, ArenaObstacle, MovementIntent, SimPlayerSeed, SimulationEvent, SimulationWorld,
    COMBAT_FRAME_MS,
};
use getrandom::fill as fill_random;

use crate::records::PlayerRecordStore;
use crate::{
    transport::{AppTransport, ConnectionId},
    RecordStoreError,
};

const DEFAULT_HIT_POINTS: u16 = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppError {
    PlayerMissing(PlayerId),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PlayerMissing(player_id) => {
                write!(f, "player {} is not connected", player_id.get())
            }
        }
    }
}

impl std::error::Error for AppError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlayerLocation {
    CentralLobby,
    GameLobby(LobbyId),
    Match(MatchId),
    Results(MatchId),
}

#[derive(Debug)]
struct ConnectedPlayer {
    player_name: PlayerName,
    record: PlayerRecord,
    location: PlayerLocation,
    inbound_control: SequenceTracker,
    inbound_input: SequenceTracker,
    next_outbound_seq: u32,
}

impl ConnectedPlayer {
    fn next_outbound_seq(&mut self) -> u32 {
        self.next_outbound_seq = self.next_outbound_seq.saturating_add(1);
        self.next_outbound_seq
    }
}

#[derive(Debug)]
struct GameLobbyRuntime {
    lobby: Lobby,
}

#[derive(Debug)]
struct MatchRuntime {
    roster: Vec<TeamAssignment>,
    participants: Vec<PlayerId>,
    session: MatchSession,
    world: SimulationWorld,
}

impl MatchRuntime {
    fn rebuild_world(&mut self, content: &GameContent) {
        self.world = build_world(&self.roster, &self.session, content);
    }
}

#[derive(Debug)]
pub struct ServerApp {
    content: GameContent,
    next_lobby_id: u32,
    next_match_id: u32,
    clock_seconds: u32,
    phase_accumulator_ms: u32,
    combat_accumulator_ms: u32,
    record_store: PlayerRecordStore,
    connections: BTreeMap<ConnectionId, PlayerId>,
    player_connections: BTreeMap<PlayerId, ConnectionId>,
    players: BTreeMap<PlayerId, ConnectedPlayer>,
    game_lobbies: BTreeMap<LobbyId, GameLobbyRuntime>,
    matches: BTreeMap<MatchId, MatchRuntime>,
}

impl Default for ServerApp {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerApp {
    #[must_use]
    pub fn new() -> Self {
        let content = match GameContent::bundled() {
            Ok(content) => content,
            Err(error) => {
                panic!("bundled content must load for tests and local development: {error}")
            }
        };
        Self::from_content_and_record_store(content, PlayerRecordStore::new_ephemeral())
    }

    pub fn new_persistent(path: impl Into<PathBuf>) -> Result<Self, RecordStoreError> {
        let content = match GameContent::bundled() {
            Ok(content) => content,
            Err(error) => {
                panic!("bundled content must load for tests and local development: {error}")
            }
        };
        Ok(Self::from_content_and_record_store(
            content,
            PlayerRecordStore::new_persistent(path.into())?,
        ))
    }

    #[must_use]
    pub fn new_with_content(content: GameContent) -> Self {
        Self::from_content_and_record_store(content, PlayerRecordStore::new_ephemeral())
    }

    pub fn new_persistent_with_content(
        content: GameContent,
        path: impl Into<PathBuf>,
    ) -> Result<Self, RecordStoreError> {
        Ok(Self::from_content_and_record_store(
            content,
            PlayerRecordStore::new_persistent(path.into())?,
        ))
    }

    fn from_content_and_record_store(
        content: GameContent,
        record_store: PlayerRecordStore,
    ) -> Self {
        Self {
            content,
            next_lobby_id: 1,
            next_match_id: 1,
            clock_seconds: 0,
            phase_accumulator_ms: 0,
            combat_accumulator_ms: 0,
            record_store,
            connections: BTreeMap::new(),
            player_connections: BTreeMap::new(),
            players: BTreeMap::new(),
            game_lobbies: BTreeMap::new(),
            matches: BTreeMap::new(),
        }
    }

    pub fn pump_transport<T: AppTransport>(&mut self, transport: &mut T) {
        while let Some((connection_id, packet)) = transport.recv_from_client() {
            self.handle_packet(transport, connection_id, &packet);
        }
    }

    pub fn advance_millis<T: AppTransport>(&mut self, transport: &mut T, delta_ms: u16) {
        if delta_ms == 0 {
            return;
        }

        self.phase_accumulator_ms = self
            .phase_accumulator_ms
            .saturating_add(u32::from(delta_ms));
        self.combat_accumulator_ms = self
            .combat_accumulator_ms
            .saturating_add(u32::from(delta_ms));

        while self.combat_accumulator_ms >= u32::from(COMBAT_FRAME_MS) {
            self.combat_accumulator_ms = self
                .combat_accumulator_ms
                .saturating_sub(u32::from(COMBAT_FRAME_MS));
            self.advance_combat_frames(transport);
        }

        while self.phase_accumulator_ms >= 1000 {
            self.phase_accumulator_ms = self.phase_accumulator_ms.saturating_sub(1000);
            self.clock_seconds = self.clock_seconds.saturating_add(1);
            self.advance_lobby_countdowns(transport);
            self.advance_match_phases(transport);
        }
    }

    pub fn advance_seconds<T: AppTransport>(&mut self, transport: &mut T, seconds: u8) {
        for _ in 0..seconds {
            self.advance_millis(transport, 1000);
        }
    }

    pub fn disconnect_player<T: AppTransport>(
        &mut self,
        transport: &mut T,
        player_id: PlayerId,
    ) -> Result<(), AppError> {
        let location = self
            .players
            .get(&player_id)
            .map(|player| player.location)
            .ok_or(AppError::PlayerMissing(player_id))?;

        match location {
            PlayerLocation::CentralLobby => {
                self.players.remove(&player_id);
                self.remove_player_connection(player_id);
                self.broadcast_lobby_directory_snapshot(transport);
            }
            PlayerLocation::GameLobby(lobby_id) => {
                let recipients = self.lobby_members(lobby_id);
                let event = match self.game_lobbies.get_mut(&lobby_id) {
                    Some(runtime) => runtime.lobby.leave_or_disconnect_player(player_id),
                    None => return Err(AppError::PlayerMissing(player_id)),
                };

                match event {
                    Ok(LobbyEvent::PlayerLeft { .. }) => {
                        self.players.remove(&player_id);
                        self.remove_player_connection(player_id);
                        let remaining = recipients
                            .into_iter()
                            .filter(|recipient| *recipient != player_id)
                            .collect::<Vec<_>>();
                        self.broadcast_event(
                            transport,
                            &remaining,
                            ServerControlEvent::GameLobbyLeft {
                                lobby_id,
                                player_id,
                            },
                        );
                        self.broadcast_game_lobby_snapshot(transport, lobby_id);
                    }
                    Ok(LobbyEvent::MatchAborted { message, .. }) => {
                        self.players.remove(&player_id);
                        self.remove_player_connection(player_id);
                        let remaining = recipients
                            .into_iter()
                            .filter(|recipient| *recipient != player_id)
                            .collect::<Vec<_>>();
                        self.broadcast_event(
                            transport,
                            &remaining,
                            ServerControlEvent::Error { message },
                        );
                        self.broadcast_game_lobby_snapshot(transport, lobby_id);
                    }
                    Ok(other) => {
                        self.players.remove(&player_id);
                        self.remove_player_connection(player_id);
                        self.broadcast_event(
                            transport,
                            &recipients,
                            ServerControlEvent::Error {
                                message: format!("unexpected lobby disconnect event: {other:?}"),
                            },
                        );
                    }
                    Err(error) => {
                        self.send_error(transport, player_id, &error.to_string());
                    }
                }

                self.cleanup_empty_lobby(lobby_id);
                self.broadcast_lobby_directory_snapshot(transport);
            }
            PlayerLocation::Match(match_id) => {
                self.end_match_as_no_contest(transport, match_id, player_id);
                self.players.remove(&player_id);
                self.remove_player_connection(player_id);
                self.cleanup_finished_match(match_id);
            }
            PlayerLocation::Results(match_id) => {
                self.players.remove(&player_id);
                self.remove_player_connection(player_id);
                self.cleanup_finished_match(match_id);
            }
        }

        Ok(())
    }

    pub fn disconnect_connection<T: AppTransport>(
        &mut self,
        transport: &mut T,
        connection_id: ConnectionId,
    ) -> Result<(), AppError> {
        match self.connections.get(&connection_id).copied() {
            Some(player_id) => self.disconnect_player(transport, player_id),
            None => Ok(()),
        }
    }

    fn handle_packet<T: AppTransport>(
        &mut self,
        transport: &mut T,
        connection_id: ConnectionId,
        packet: &[u8],
    ) {
        let bound_player = self.connections.get(&connection_id).copied();
        if let Ok((header, command)) = ClientControlCommand::decode_packet(packet) {
            match bound_player {
                Some(player_id) => {
                    if let Some(player) = self.players.get_mut(&player_id) {
                        if let Err(error) = player.inbound_control.observe(header.seq) {
                            self.send_error(transport, player_id, &error.to_string());
                            return;
                        }
                    }

                    self.handle_control_command(
                        transport,
                        connection_id,
                        player_id,
                        header.seq,
                        command,
                    );
                }
                None => match command {
                    ClientControlCommand::Connect { player_name } => {
                        self.handle_connect_command(
                            transport,
                            connection_id,
                            header.seq,
                            player_name,
                        );
                    }
                    _ => self.send_direct_error(
                        transport,
                        connection_id,
                        "first packet must be a connect command",
                    ),
                },
            }
            return;
        }

        match ValidatedInputFrame::decode_packet(packet) {
            Ok((header, frame)) => match bound_player {
                Some(player_id) => {
                    if let Some(player) = self.players.get_mut(&player_id) {
                        if let Err(error) = player.inbound_input.observe(header.seq) {
                            self.send_error(transport, player_id, &error.to_string());
                            return;
                        }
                    }

                    self.handle_input_frame(transport, player_id, frame);
                }
                None => self.send_direct_error(
                    transport,
                    connection_id,
                    "first packet must be a connect command",
                ),
            },
            Err(error) => match bound_player {
                Some(player_id) => self.send_error(transport, player_id, &error.to_string()),
                None => self.send_direct_error(transport, connection_id, &error.to_string()),
            },
        }
    }

    fn handle_control_command<T: AppTransport>(
        &mut self,
        transport: &mut T,
        _connection_id: ConnectionId,
        sender_id: PlayerId,
        _seq: u32,
        command: ClientControlCommand,
    ) {
        match command {
            ClientControlCommand::Connect { .. } => {
                self.send_error(transport, sender_id, "player is already connected");
            }
            ClientControlCommand::CreateGameLobby => {
                self.handle_create_game_lobby(transport, sender_id)
            }
            ClientControlCommand::JoinGameLobby { lobby_id } => {
                self.handle_join_game_lobby(transport, sender_id, lobby_id);
            }
            ClientControlCommand::LeaveGameLobby => {
                self.handle_leave_game_lobby(transport, sender_id);
            }
            ClientControlCommand::SelectTeam { team } => {
                self.handle_select_team(transport, sender_id, team);
            }
            ClientControlCommand::SetReady { ready } => {
                self.handle_set_ready(transport, sender_id, ready);
            }
            ClientControlCommand::ChooseSkill { tree, tier } => {
                self.handle_choose_skill(transport, sender_id, tree, tier);
            }
            ClientControlCommand::QuitToCentralLobby => {
                self.handle_quit_to_central_lobby(transport, sender_id);
            }
        }
    }

    fn handle_connect_command<T: AppTransport>(
        &mut self,
        transport: &mut T,
        connection_id: ConnectionId,
        seq: u32,
        player_name: PlayerName,
    ) {
        if self.connections.contains_key(&connection_id) {
            self.send_direct_error(transport, connection_id, "connection is already bound");
            return;
        }

        let player_id = match self.allocate_player_id() {
            Ok(player_id) => player_id,
            Err(message) => {
                self.send_direct_error(transport, connection_id, &message);
                return;
            }
        };
        let record = match self.record_store.load_or_create(&player_name) {
            Ok(record) => record,
            Err(error) => {
                self.send_direct_error(transport, connection_id, &error.to_string());
                return;
            }
        };

        let mut inbound_control = SequenceTracker::new();
        if let Err(error) = inbound_control.observe(seq) {
            self.send_direct_error(transport, connection_id, &error.to_string());
            return;
        }

        self.connections.insert(connection_id, player_id);
        self.player_connections.insert(player_id, connection_id);
        self.players.insert(
            player_id,
            ConnectedPlayer {
                player_name: player_name.clone(),
                record,
                location: PlayerLocation::CentralLobby,
                inbound_control,
                inbound_input: SequenceTracker::new(),
                next_outbound_seq: 0,
            },
        );

        self.send_event(
            transport,
            player_id,
            ServerControlEvent::Connected {
                player_id,
                player_name,
                record,
            },
        );
        self.send_lobby_directory_snapshot(transport, player_id);
    }

    fn handle_create_game_lobby<T: AppTransport>(
        &mut self,
        transport: &mut T,
        sender_id: PlayerId,
    ) {
        if !self.ensure_location(transport, sender_id, PlayerLocation::CentralLobby) {
            return;
        }

        let lobby_id = self.allocate_lobby_id();
        let mut lobby = Lobby::new(lobby_id);
        let (player_name, record) = match self.players.get(&sender_id) {
            Some(player) => (player.player_name.clone(), player.record),
            None => {
                self.send_error(transport, sender_id, "player is not connected");
                return;
            }
        };

        if let Err(error) = lobby.add_player(sender_id, player_name, record) {
            self.send_error(transport, sender_id, &error.to_string());
            return;
        }

        self.game_lobbies
            .insert(lobby_id, GameLobbyRuntime { lobby });
        if let Some(player) = self.players.get_mut(&sender_id) {
            player.location = PlayerLocation::GameLobby(lobby_id);
        }

        self.send_event(
            transport,
            sender_id,
            ServerControlEvent::GameLobbyCreated { lobby_id },
        );
        self.send_event(
            transport,
            sender_id,
            ServerControlEvent::GameLobbyJoined {
                lobby_id,
                player_id: sender_id,
            },
        );
        self.send_game_lobby_snapshot(transport, lobby_id, sender_id);
        self.broadcast_lobby_directory_snapshot(transport);
    }

    fn handle_join_game_lobby<T: AppTransport>(
        &mut self,
        transport: &mut T,
        sender_id: PlayerId,
        lobby_id: LobbyId,
    ) {
        if !self.ensure_location(transport, sender_id, PlayerLocation::CentralLobby) {
            return;
        }

        let (player_name, record) = match self.players.get(&sender_id) {
            Some(player) => (player.player_name.clone(), player.record),
            None => {
                self.send_error(transport, sender_id, "player is not connected");
                return;
            }
        };

        let lobby = match self.game_lobbies.get_mut(&lobby_id) {
            Some(runtime) => &mut runtime.lobby,
            None => {
                self.send_error(transport, sender_id, "game lobby does not exist");
                return;
            }
        };

        if let Err(error) = lobby.add_player(sender_id, player_name, record) {
            self.send_error(transport, sender_id, &error.to_string());
            return;
        }

        if let Some(player) = self.players.get_mut(&sender_id) {
            player.location = PlayerLocation::GameLobby(lobby_id);
        }

        let recipients = self.lobby_members(lobby_id);
        self.broadcast_event(
            transport,
            &recipients,
            ServerControlEvent::GameLobbyJoined {
                lobby_id,
                player_id: sender_id,
            },
        );
        self.broadcast_game_lobby_snapshot(transport, lobby_id);
        self.broadcast_lobby_directory_snapshot(transport);
    }

    fn handle_leave_game_lobby<T: AppTransport>(&mut self, transport: &mut T, sender_id: PlayerId) {
        let lobby_id = match self.require_game_lobby(transport, sender_id) {
            Some(lobby_id) => lobby_id,
            None => return,
        };

        let lobby_phase = match self.game_lobbies.get(&lobby_id) {
            Some(runtime) => runtime.lobby.phase().clone(),
            None => {
                self.send_error(transport, sender_id, "game lobby does not exist");
                return;
            }
        };
        if !matches!(lobby_phase, LobbyPhase::Open) {
            self.send_error(
                transport,
                sender_id,
                "players cannot leave after countdown starts",
            );
            return;
        }

        let event = match self.game_lobbies.get_mut(&lobby_id) {
            Some(runtime) => runtime.lobby.leave_or_disconnect_player(sender_id),
            None => {
                self.send_error(transport, sender_id, "game lobby does not exist");
                return;
            }
        };

        match event {
            Ok(LobbyEvent::PlayerLeft { .. }) => {
                if let Some(player) = self.players.get_mut(&sender_id) {
                    player.location = PlayerLocation::CentralLobby;
                }
                let recipients = self.lobby_members(lobby_id);
                self.broadcast_event(
                    transport,
                    &recipients,
                    ServerControlEvent::GameLobbyLeft {
                        lobby_id,
                        player_id: sender_id,
                    },
                );
                let record = self
                    .players
                    .get(&sender_id)
                    .map(|player| player.record)
                    .unwrap_or_default();
                self.send_event(
                    transport,
                    sender_id,
                    ServerControlEvent::ReturnedToCentralLobby { record },
                );
                self.cleanup_empty_lobby(lobby_id);
                self.broadcast_game_lobby_snapshot(transport, lobby_id);
                self.send_lobby_directory_snapshot(transport, sender_id);
                self.broadcast_lobby_directory_snapshot(transport);
            }
            Ok(other) => self.send_error(
                transport,
                sender_id,
                &format!("unexpected leave event: {other:?}"),
            ),
            Err(error) => self.send_error(transport, sender_id, &error.to_string()),
        }
    }

    fn handle_select_team<T: AppTransport>(
        &mut self,
        transport: &mut T,
        sender_id: PlayerId,
        team: TeamSide,
    ) {
        let lobby_id = match self.require_game_lobby(transport, sender_id) {
            Some(lobby_id) => lobby_id,
            None => return,
        };

        let events = match self.game_lobbies.get_mut(&lobby_id) {
            Some(runtime) => runtime.lobby.select_team(sender_id, team),
            None => {
                self.send_error(transport, sender_id, "game lobby does not exist");
                return;
            }
        };

        match events {
            Ok(events) => {
                self.broadcast_lobby_events(transport, lobby_id, &events);
                self.broadcast_game_lobby_snapshot(transport, lobby_id);
                self.broadcast_lobby_directory_snapshot(transport);
            }
            Err(error) => self.send_error(transport, sender_id, &error.to_string()),
        }
    }

    fn handle_set_ready<T: AppTransport>(
        &mut self,
        transport: &mut T,
        sender_id: PlayerId,
        ready: ReadyState,
    ) {
        let lobby_id = match self.require_game_lobby(transport, sender_id) {
            Some(lobby_id) => lobby_id,
            None => return,
        };

        let events = match self.game_lobbies.get_mut(&lobby_id) {
            Some(runtime) => runtime.lobby.set_ready(sender_id, ready),
            None => {
                self.send_error(transport, sender_id, "game lobby does not exist");
                return;
            }
        };

        match events {
            Ok(events) => {
                self.broadcast_lobby_events(transport, lobby_id, &events);
                self.broadcast_game_lobby_snapshot(transport, lobby_id);
                self.broadcast_lobby_directory_snapshot(transport);
            }
            Err(error) => self.send_error(transport, sender_id, &error.to_string()),
        }
    }

    fn handle_choose_skill<T: AppTransport>(
        &mut self,
        transport: &mut T,
        sender_id: PlayerId,
        tree: game_domain::SkillTree,
        tier: u8,
    ) {
        let match_id = match self.require_match(transport, sender_id) {
            Some(match_id) => match_id,
            None => return,
        };

        let choice = match SkillChoice::new(tree, tier) {
            Ok(choice) => choice,
            Err(error) => {
                self.send_error(transport, sender_id, &error.to_string());
                return;
            }
        };
        if self.content.skills().resolve(choice).is_none() {
            self.send_error(
                transport,
                sender_id,
                &format!("no authored skill exists for {tree} tier {tier}"),
            );
            return;
        }

        let events = match self.matches.get_mut(&match_id) {
            Some(runtime) => runtime.session.submit_skill_pick(sender_id, choice),
            None => {
                self.send_error(transport, sender_id, "match does not exist");
                return;
            }
        };

        match events {
            Ok(events) => self.dispatch_match_events(transport, match_id, &events),
            Err(error) => self.send_error(transport, sender_id, &error.to_string()),
        }
    }

    fn handle_quit_to_central_lobby<T: AppTransport>(
        &mut self,
        transport: &mut T,
        sender_id: PlayerId,
    ) {
        let match_id = match self.require_results(transport, sender_id) {
            Some(match_id) => match_id,
            None => return,
        };

        if let Some(record) = self.players.get(&sender_id).map(|player| player.record) {
            if let Some(player) = self.players.get_mut(&sender_id) {
                player.location = PlayerLocation::CentralLobby;
            }
            self.send_event(
                transport,
                sender_id,
                ServerControlEvent::ReturnedToCentralLobby { record },
            );
            self.send_lobby_directory_snapshot(transport, sender_id);
        }

        self.cleanup_finished_match(match_id);
    }

    #[allow(clippy::too_many_lines)]
    fn handle_input_frame<T: AppTransport>(
        &mut self,
        transport: &mut T,
        sender_id: PlayerId,
        frame: ValidatedInputFrame,
    ) {
        let match_id = match self.require_match(transport, sender_id) {
            Some(match_id) => match_id,
            None => return,
        };

        let phase = match self.matches.get(&match_id) {
            Some(runtime) => runtime.session.phase().clone(),
            None => {
                self.send_error(transport, sender_id, "match does not exist");
                return;
            }
        };
        if !matches!(phase, MatchPhase::Combat) {
            self.send_error(
                transport,
                sender_id,
                "input frames are only accepted during combat",
            );
            return;
        }
        if frame.buttons & BUTTON_QUIT_TO_LOBBY != 0 {
            self.send_error(
                transport,
                sender_id,
                "quit-to-lobby input is not valid during combat",
            );
            return;
        }

        let move_x = match Self::decode_axis(frame.move_horizontal_q, "move_horizontal_q") {
            Ok(value) => value,
            Err(message) => {
                self.send_error(transport, sender_id, &message);
                return;
            }
        };
        let move_y = match Self::decode_axis(frame.move_vertical_q, "move_vertical_q") {
            Ok(value) => value,
            Err(message) => {
                self.send_error(transport, sender_id, &message);
                return;
            }
        };

        let runtime = match self.matches.get_mut(&match_id) {
            Some(runtime) => runtime,
            None => {
                self.send_error(transport, sender_id, "match does not exist");
                return;
            }
        };

        let movement = match MovementIntent::new(move_x, move_y) {
            Ok(movement) => movement,
            Err(error) => {
                self.send_error(transport, sender_id, &error.to_string());
                return;
            }
        };
        let aim_changed =
            match runtime
                .world
                .update_aim(sender_id, frame.aim_horizontal_q, frame.aim_vertical_q)
            {
                Ok(changed) => changed,
                Err(error) => {
                    self.send_error(transport, sender_id, &error.to_string());
                    return;
                }
            };
        if let Err(error) = runtime.world.submit_input(sender_id, movement) {
            self.send_error(transport, sender_id, &error.to_string());
            return;
        }

        if frame.buttons & BUTTON_PRIMARY != 0 {
            if let Err(error) = runtime.world.queue_primary_attack(sender_id) {
                self.send_error(transport, sender_id, &error.to_string());
                return;
            }
        }

        if frame.buttons & BUTTON_CAST != 0 {
            let slot = u8::try_from(frame.ability_or_context).unwrap_or(u8::MAX);
            let unlocked_slots = runtime.session.current_round().get();
            if slot == 0 || slot > unlocked_slots {
                self.send_error(
                    transport,
                    sender_id,
                    &format!("skill slot {slot} is not unlocked for round {unlocked_slots}"),
                );
                return;
            }
            let Some(choice) = runtime.session.equipped_choice(sender_id, slot) else {
                self.send_error(
                    transport,
                    sender_id,
                    &format!("skill slot {slot} is not equipped"),
                );
                return;
            };
            let Some(skill) = self.content.skills().resolve(choice) else {
                self.send_error(
                    transport,
                    sender_id,
                    &format!(
                        "authored skill data is missing for {} tier {}",
                        choice.tree, choice.tier
                    ),
                );
                return;
            };

            let _ = skill;
            if let Err(error) = runtime.world.queue_cast(sender_id, slot) {
                self.send_error(transport, sender_id, &error.to_string());
                return;
            }
        }

        let _ = runtime;
        if aim_changed {
            self.broadcast_arena_state_snapshot(transport, match_id);
        }
    }

    fn advance_combat_frames<T: AppTransport>(&mut self, transport: &mut T) {
        let match_ids = self.matches.keys().copied().collect::<Vec<_>>();
        for match_id in match_ids {
            let phase = match self.matches.get(&match_id) {
                Some(runtime) => runtime.session.phase().clone(),
                None => continue,
            };
            if !matches!(phase, MatchPhase::Combat) {
                continue;
            }

            let (effect_batch, match_events, errors) = {
                let runtime = match self.matches.get_mut(&match_id) {
                    Some(runtime) => runtime,
                    None => continue,
                };
                let simulation_events = runtime.world.tick(COMBAT_FRAME_MS);
                let effect_batch = Self::collect_effect_batch(&simulation_events);
                let defeated_targets = Self::collect_defeated_targets(&simulation_events);
                let mut match_events = Vec::new();
                let mut errors = Vec::new();
                for target_id in defeated_targets {
                    match runtime.session.mark_player_defeated(target_id) {
                        Ok(events) => match_events.extend(events),
                        Err(error) => errors.push(error.to_string()),
                    }
                }
                if matches!(runtime.session.phase(), MatchPhase::SkillPick { .. })
                    && !matches!(runtime.session.phase(), MatchPhase::MatchEnd { .. })
                {
                    runtime.rebuild_world(&self.content);
                }
                (effect_batch, match_events, errors)
            };

            if !effect_batch.is_empty() {
                self.broadcast_arena_effect_batch(transport, match_id, &effect_batch);
            }
            if !errors.is_empty() {
                self.broadcast_event(
                    transport,
                    &self.match_recipients(match_id),
                    ServerControlEvent::Error {
                        message: errors.join(" | "),
                    },
                );
            }
            if !match_events.is_empty() {
                self.dispatch_match_events(transport, match_id, &match_events);
            }
            self.broadcast_arena_state_snapshot(transport, match_id);
        }
    }

    fn advance_lobby_countdowns<T: AppTransport>(&mut self, transport: &mut T) {
        let countdowns = self
            .game_lobbies
            .iter()
            .filter_map(|(lobby_id, runtime)| {
                if matches!(runtime.lobby.phase(), LobbyPhase::LaunchCountdown { .. }) {
                    Some(*lobby_id)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        for lobby_id in countdowns {
            let event = match self.game_lobbies.get_mut(&lobby_id) {
                Some(runtime) => runtime.lobby.advance_countdown(),
                None => continue,
            };

            match event {
                Ok(LobbyEvent::LaunchCountdownTick { seconds_remaining }) => {
                    self.broadcast_event(
                        transport,
                        &self.lobby_members(lobby_id),
                        ServerControlEvent::LaunchCountdownTick {
                            lobby_id,
                            seconds_remaining,
                        },
                    );
                    self.broadcast_game_lobby_snapshot(transport, lobby_id);
                    self.broadcast_lobby_directory_snapshot(transport);
                }
                Ok(LobbyEvent::MatchLaunchReady { roster }) => {
                    self.start_match_from_lobby(transport, lobby_id, roster);
                }
                Ok(other) => {
                    self.broadcast_event(
                        transport,
                        &self.lobby_members(lobby_id),
                        ServerControlEvent::Error {
                            message: format!("unexpected countdown event: {other:?}"),
                        },
                    );
                }
                Err(error) => {
                    self.broadcast_event(
                        transport,
                        &self.lobby_members(lobby_id),
                        ServerControlEvent::Error {
                            message: error.to_string(),
                        },
                    );
                }
            }
        }
    }

    fn advance_match_phases<T: AppTransport>(&mut self, transport: &mut T) {
        let match_ids = self.matches.keys().copied().collect::<Vec<_>>();
        for match_id in match_ids {
            let phase = match self.matches.get(&match_id) {
                Some(runtime) => runtime.session.phase().clone(),
                None => continue,
            };

            if !matches!(
                phase,
                MatchPhase::SkillPick { .. } | MatchPhase::PreCombat { .. }
            ) {
                continue;
            }

            let events = match self.matches.get_mut(&match_id) {
                Some(runtime) => runtime.session.advance_phase_by(1),
                None => continue,
            };

            match events {
                Ok(events) => self.dispatch_match_events(transport, match_id, &events),
                Err(error) => {
                    self.broadcast_event(
                        transport,
                        &self.match_recipients(match_id),
                        ServerControlEvent::Error {
                            message: error.to_string(),
                        },
                    );
                }
            }
        }
    }

    fn dispatch_match_events<T: AppTransport>(
        &mut self,
        transport: &mut T,
        match_id: MatchId,
        events: &[MatchEvent],
    ) {
        for event in events {
            match event {
                MatchEvent::SkillChosen { player_id, choice } => {
                    self.broadcast_event(
                        transport,
                        &self.match_recipients(match_id),
                        ServerControlEvent::SkillChosen {
                            player_id: *player_id,
                            tree: choice.tree,
                            tier: choice.tier,
                        },
                    );
                }
                MatchEvent::PreCombatStarted { seconds_remaining } => {
                    self.broadcast_event(
                        transport,
                        &self.match_recipients(match_id),
                        ServerControlEvent::PreCombatStarted {
                            seconds_remaining: *seconds_remaining,
                        },
                    );
                }
                MatchEvent::CombatStarted => {
                    if let Some(runtime) = self.matches.get_mut(&match_id) {
                        runtime.rebuild_world(&self.content);
                    }
                    self.broadcast_event(
                        transport,
                        &self.match_recipients(match_id),
                        ServerControlEvent::CombatStarted,
                    );
                }
                MatchEvent::RoundWon {
                    round,
                    winning_team,
                    score,
                } => {
                    self.broadcast_event(
                        transport,
                        &self.match_recipients(match_id),
                        ServerControlEvent::RoundWon {
                            round: *round,
                            winning_team: *winning_team,
                            score_a: score.team_a,
                            score_b: score.team_b,
                        },
                    );
                }
                MatchEvent::MatchEnded {
                    outcome,
                    message,
                    score,
                } => {
                    self.apply_match_outcome(transport, match_id, *outcome);
                    self.broadcast_event(
                        transport,
                        &self.match_recipients(match_id),
                        ServerControlEvent::MatchEnded {
                            outcome: *outcome,
                            score_a: score.team_a,
                            score_b: score.team_b,
                            message: message.clone(),
                        },
                    );
                }
                MatchEvent::ManualResolutionRequired { reason } => {
                    self.broadcast_event(
                        transport,
                        &self.match_recipients(match_id),
                        ServerControlEvent::Error {
                            message: (*reason).to_string(),
                        },
                    );
                }
            }
        }

        if self.matches.contains_key(&match_id) {
            self.broadcast_arena_state_snapshot(transport, match_id);
        }
    }

    fn broadcast_lobby_events<T: AppTransport>(
        &mut self,
        transport: &mut T,
        lobby_id: LobbyId,
        events: &[LobbyEvent],
    ) {
        let recipients = self.lobby_members(lobby_id);
        for event in events {
            match event {
                LobbyEvent::PlayerJoined { .. }
                | LobbyEvent::PlayerLeft { .. }
                | LobbyEvent::MatchLaunchReady { .. } => {}
                LobbyEvent::TeamSelected {
                    player_id,
                    team,
                    ready_reset,
                } => self.broadcast_event(
                    transport,
                    &recipients,
                    ServerControlEvent::TeamSelected {
                        player_id: *player_id,
                        team: *team,
                        ready_reset: *ready_reset,
                    },
                ),
                LobbyEvent::ReadyChanged { player_id, ready } => self.broadcast_event(
                    transport,
                    &recipients,
                    ServerControlEvent::ReadyChanged {
                        player_id: *player_id,
                        ready: *ready,
                    },
                ),
                LobbyEvent::LaunchCountdownStarted {
                    seconds_remaining,
                    roster,
                } => self.broadcast_event(
                    transport,
                    &recipients,
                    ServerControlEvent::LaunchCountdownStarted {
                        lobby_id,
                        seconds_remaining: *seconds_remaining,
                        roster_size: u16::try_from(roster.len()).unwrap_or(u16::MAX),
                    },
                ),
                LobbyEvent::LaunchCountdownTick { seconds_remaining } => self.broadcast_event(
                    transport,
                    &recipients,
                    ServerControlEvent::LaunchCountdownTick {
                        lobby_id,
                        seconds_remaining: *seconds_remaining,
                    },
                ),
                LobbyEvent::MatchAborted { message, .. } => self.broadcast_event(
                    transport,
                    &recipients,
                    ServerControlEvent::Error {
                        message: message.clone(),
                    },
                ),
            }
        }
    }

    fn send_lobby_directory_snapshot<T: AppTransport>(
        &mut self,
        transport: &mut T,
        player_id: PlayerId,
    ) {
        let event = ServerControlEvent::LobbyDirectorySnapshot {
            lobbies: self.build_lobby_directory_entries(),
        };
        self.send_event(transport, player_id, event);
    }

    fn broadcast_lobby_directory_snapshot<T: AppTransport>(&mut self, transport: &mut T) {
        let recipients = self.central_lobby_players();
        if recipients.is_empty() {
            return;
        }

        let event = ServerControlEvent::LobbyDirectorySnapshot {
            lobbies: self.build_lobby_directory_entries(),
        };
        self.broadcast_event(transport, &recipients, event);
    }

    fn send_game_lobby_snapshot<T: AppTransport>(
        &mut self,
        transport: &mut T,
        lobby_id: LobbyId,
        player_id: PlayerId,
    ) {
        let Some(event) = self.build_game_lobby_snapshot(lobby_id) else {
            return;
        };
        self.send_event(transport, player_id, event);
    }

    fn broadcast_game_lobby_snapshot<T: AppTransport>(
        &mut self,
        transport: &mut T,
        lobby_id: LobbyId,
    ) {
        let recipients = self.lobby_members(lobby_id);
        if recipients.is_empty() {
            return;
        }

        let Some(event) = self.build_game_lobby_snapshot(lobby_id) else {
            return;
        };
        self.broadcast_event(transport, &recipients, event);
    }

    fn build_lobby_directory_entries(&self) -> Vec<LobbyDirectoryEntry> {
        self.game_lobbies
            .iter()
            .map(|(lobby_id, runtime)| {
                let players = runtime.lobby.players();
                let team_a_count = players
                    .iter()
                    .filter(|player| player.team == Some(TeamSide::TeamA))
                    .count();
                let team_b_count = players
                    .iter()
                    .filter(|player| player.team == Some(TeamSide::TeamB))
                    .count();
                let ready_count = players
                    .iter()
                    .filter(|player| player.ready_state.is_ready())
                    .count();

                LobbyDirectoryEntry {
                    lobby_id: *lobby_id,
                    player_count: u16::try_from(players.len()).unwrap_or(u16::MAX),
                    team_a_count: u16::try_from(team_a_count).unwrap_or(u16::MAX),
                    team_b_count: u16::try_from(team_b_count).unwrap_or(u16::MAX),
                    ready_count: u16::try_from(ready_count).unwrap_or(u16::MAX),
                    phase: Self::lobby_snapshot_phase(runtime.lobby.phase()),
                }
            })
            .collect()
    }

    fn build_game_lobby_snapshot(&self, lobby_id: LobbyId) -> Option<ServerControlEvent> {
        let runtime = self.game_lobbies.get(&lobby_id)?;
        let players = runtime
            .lobby
            .players()
            .into_iter()
            .map(|player| LobbySnapshotPlayer {
                player_id: player.player_id,
                player_name: player.player_name,
                record: player.record,
                team: player.team,
                ready: player.ready_state,
            })
            .collect();

        Some(ServerControlEvent::GameLobbySnapshot {
            lobby_id,
            phase: Self::lobby_snapshot_phase(runtime.lobby.phase()),
            players,
        })
    }

    fn build_arena_state_snapshot(&self, match_id: MatchId) -> Option<ArenaStateSnapshot> {
        let runtime = self.matches.get(&match_id)?;
        let unlocked_skill_slots = runtime.session.current_round().get();
        let obstacles = runtime
            .world
            .obstacles()
            .iter()
            .map(Self::arena_obstacle_snapshot)
            .collect();
        let players = runtime
            .roster
            .iter()
            .filter_map(|assignment| {
                runtime
                    .world
                    .player_state(assignment.player_id)
                    .map(|state| {
                        Self::arena_player_snapshot(assignment, state, unlocked_skill_slots)
                    })
            })
            .collect();
        let projectiles = runtime
            .world
            .projectiles()
            .into_iter()
            .map(|projectile| ArenaProjectileSnapshot {
                owner: projectile.owner,
                slot: projectile.slot,
                kind: match projectile.kind {
                    game_sim::ArenaEffectKind::MeleeSwing => ArenaEffectKind::MeleeSwing,
                    game_sim::ArenaEffectKind::SkillShot => ArenaEffectKind::SkillShot,
                    game_sim::ArenaEffectKind::DashTrail => ArenaEffectKind::DashTrail,
                    game_sim::ArenaEffectKind::Burst => ArenaEffectKind::Burst,
                    game_sim::ArenaEffectKind::Nova => ArenaEffectKind::Nova,
                    game_sim::ArenaEffectKind::Beam => ArenaEffectKind::Beam,
                    game_sim::ArenaEffectKind::HitSpark => ArenaEffectKind::HitSpark,
                },
                x: projectile.x,
                y: projectile.y,
                radius: projectile.radius,
            })
            .collect();

        Some(ArenaStateSnapshot {
            width: runtime.world.arena_width_units(),
            height: runtime.world.arena_height_units(),
            obstacles,
            players,
            projectiles,
        })
    }

    fn broadcast_arena_state_snapshot<T: AppTransport>(
        &mut self,
        transport: &mut T,
        match_id: MatchId,
    ) {
        let recipients = self.match_recipients(match_id);
        if recipients.is_empty() {
            return;
        }

        let Some(snapshot) = self.build_arena_state_snapshot(match_id) else {
            return;
        };
        self.broadcast_event(
            transport,
            &recipients,
            ServerControlEvent::ArenaStateSnapshot { snapshot },
        );
    }

    fn broadcast_arena_effect_batch<T: AppTransport>(
        &mut self,
        transport: &mut T,
        match_id: MatchId,
        effects: &[ArenaEffectSnapshot],
    ) {
        if effects.is_empty() {
            return;
        }

        let recipients = self.match_recipients(match_id);
        if recipients.is_empty() {
            return;
        }

        self.broadcast_event(
            transport,
            &recipients,
            ServerControlEvent::ArenaEffectBatch {
                effects: effects.to_vec(),
            },
        );
    }

    fn arena_obstacle_snapshot(obstacle: &ArenaObstacle) -> ArenaObstacleSnapshot {
        ArenaObstacleSnapshot {
            kind: match obstacle.kind {
                game_sim::ArenaObstacleKind::Pillar => ArenaObstacleKind::Pillar,
                game_sim::ArenaObstacleKind::Shrub => ArenaObstacleKind::Shrub,
            },
            center_x: obstacle.center_x,
            center_y: obstacle.center_y,
            half_width: obstacle.half_width,
            half_height: obstacle.half_height,
        }
    }

    fn arena_player_snapshot(
        assignment: &TeamAssignment,
        state: game_sim::SimPlayerState,
        unlocked_skill_slots: u8,
    ) -> ArenaPlayerSnapshot {
        ArenaPlayerSnapshot {
            player_id: assignment.player_id,
            player_name: assignment.player_name.clone(),
            team: assignment.team,
            x: state.x,
            y: state.y,
            aim_x: state.aim_x,
            aim_y: state.aim_y,
            hit_points: state.hit_points,
            max_hit_points: state.max_hit_points,
            alive: state.alive,
            unlocked_skill_slots,
            primary_cooldown_remaining_ms: state.primary_cooldown_remaining_ms,
            primary_cooldown_total_ms: state.primary_cooldown_total_ms,
            slot_cooldown_remaining_ms: state.slot_cooldown_remaining_ms,
            slot_cooldown_total_ms: state.slot_cooldown_total_ms,
        }
    }

    fn collect_defeated_targets(events: &[SimulationEvent]) -> Vec<PlayerId> {
        let mut defeated_targets = Vec::new();
        for event in events {
            if let SimulationEvent::DamageApplied {
                target, defeated, ..
            } = event
            {
                if *defeated && !defeated_targets.contains(target) {
                    defeated_targets.push(*target);
                }
            }
        }
        defeated_targets
    }

    fn collect_effect_batch(events: &[SimulationEvent]) -> Vec<ArenaEffectSnapshot> {
        events
            .iter()
            .filter_map(|event| match event {
                SimulationEvent::EffectSpawned { effect } => {
                    Some(Self::arena_effect_snapshot(effect))
                }
                _ => None,
            })
            .collect()
    }

    fn arena_effect_snapshot(effect: &ArenaEffect) -> ArenaEffectSnapshot {
        ArenaEffectSnapshot {
            kind: match effect.kind {
                game_sim::ArenaEffectKind::MeleeSwing => ArenaEffectKind::MeleeSwing,
                game_sim::ArenaEffectKind::SkillShot => ArenaEffectKind::SkillShot,
                game_sim::ArenaEffectKind::DashTrail => ArenaEffectKind::DashTrail,
                game_sim::ArenaEffectKind::Burst => ArenaEffectKind::Burst,
                game_sim::ArenaEffectKind::Nova => ArenaEffectKind::Nova,
                game_sim::ArenaEffectKind::Beam => ArenaEffectKind::Beam,
                game_sim::ArenaEffectKind::HitSpark => ArenaEffectKind::HitSpark,
            },
            owner: effect.owner,
            slot: effect.slot,
            x: effect.x,
            y: effect.y,
            target_x: effect.target_x,
            target_y: effect.target_y,
            radius: effect.radius,
        }
    }

    fn lobby_snapshot_phase(phase: &LobbyPhase) -> LobbySnapshotPhase {
        match phase {
            LobbyPhase::Open => LobbySnapshotPhase::Open,
            LobbyPhase::LaunchCountdown {
                seconds_remaining, ..
            } => LobbySnapshotPhase::LaunchCountdown {
                seconds_remaining: *seconds_remaining,
            },
        }
    }

    fn start_match_from_lobby<T: AppTransport>(
        &mut self,
        transport: &mut T,
        lobby_id: LobbyId,
        roster: Vec<TeamAssignment>,
    ) {
        let match_id = self.allocate_match_id();
        let session = match MatchSession::new(match_id, roster.clone(), MatchConfig::v1()) {
            Ok(session) => session,
            Err(error) => {
                self.broadcast_event(
                    transport,
                    &self.lobby_members(lobby_id),
                    ServerControlEvent::Error {
                        message: error.to_string(),
                    },
                );
                return;
            }
        };

        let participants = roster
            .iter()
            .map(|assignment| assignment.player_id)
            .collect::<Vec<_>>();
        for player_id in &participants {
            if let Some(player) = self.players.get_mut(player_id) {
                player.location = PlayerLocation::Match(match_id);
            }
        }

        self.matches.insert(
            match_id,
            MatchRuntime {
                world: build_world(&roster, &session, &self.content),
                roster,
                participants: participants.clone(),
                session,
            },
        );
        self.game_lobbies.remove(&lobby_id);
        self.broadcast_lobby_directory_snapshot(transport);

        self.broadcast_event(
            transport,
            &participants,
            ServerControlEvent::MatchStarted {
                match_id,
                round: match game_domain::RoundNumber::new(1) {
                    Ok(round) => round,
                    Err(error) => panic!("round one must be valid: {error}"),
                },
                skill_pick_seconds: MatchConfig::v1().skill_pick_seconds,
            },
        );
        self.broadcast_arena_state_snapshot(transport, match_id);
    }

    fn apply_match_outcome<T: AppTransport>(
        &mut self,
        transport: &mut T,
        match_id: MatchId,
        outcome: MatchOutcome,
    ) {
        let roster = match self.matches.get(&match_id) {
            Some(runtime) => runtime.roster.clone(),
            None => return,
        };

        let mut dirty_players = Vec::new();
        for assignment in roster {
            if let Some(player) = self.players.get_mut(&assignment.player_id) {
                match outcome {
                    MatchOutcome::TeamAWin => {
                        if assignment.team == TeamSide::TeamA {
                            player.record.record_win();
                        } else {
                            player.record.record_loss();
                        }
                    }
                    MatchOutcome::TeamBWin => {
                        if assignment.team == TeamSide::TeamB {
                            player.record.record_win();
                        } else {
                            player.record.record_loss();
                        }
                    }
                    MatchOutcome::NoContest => player.record.record_no_contest(),
                }
                player.location = PlayerLocation::Results(match_id);
                dirty_players.push(assignment.player_id);
            }
        }

        for player_id in dirty_players {
            let _ = self.persist_player_record(transport, player_id);
        }
    }

    fn persist_player_record<T: AppTransport>(
        &mut self,
        transport: &mut T,
        player_id: PlayerId,
    ) -> bool {
        let Some((player_name, record)) = self
            .players
            .get(&player_id)
            .map(|player| (player.player_name.clone(), player.record))
        else {
            return false;
        };

        let save_result = self.record_store.save(&player_name, record);
        match save_result {
            Ok(()) => true,
            Err(error) => {
                self.send_error(transport, player_id, &error.to_string());
                false
            }
        }
    }

    fn end_match_as_no_contest<T: AppTransport>(
        &mut self,
        transport: &mut T,
        match_id: MatchId,
        disconnecting_player: PlayerId,
    ) {
        let ended_event = match self.matches.get_mut(&match_id) {
            Some(runtime) => match runtime.session.disconnect_player(disconnecting_player) {
                Ok(MatchEvent::MatchEnded {
                    outcome,
                    score,
                    message,
                }) => ServerControlEvent::MatchEnded {
                    outcome,
                    score_a: score.team_a,
                    score_b: score.team_b,
                    message,
                },
                Ok(other) => ServerControlEvent::Error {
                    message: format!("unexpected disconnect result: {other:?}"),
                },
                Err(error) => ServerControlEvent::Error {
                    message: error.to_string(),
                },
            },
            None => return,
        };

        self.apply_match_outcome(transport, match_id, MatchOutcome::NoContest);
        let recipients = self
            .match_recipients(match_id)
            .into_iter()
            .filter(|recipient| *recipient != disconnecting_player)
            .collect::<Vec<_>>();
        self.broadcast_event(transport, &recipients, ended_event);
    }

    fn send_error<T: AppTransport>(
        &mut self,
        transport: &mut T,
        player_id: PlayerId,
        message: &str,
    ) {
        self.send_event(
            transport,
            player_id,
            ServerControlEvent::Error {
                message: message.to_string(),
            },
        );
    }

    fn send_direct_error<T: AppTransport>(
        &mut self,
        transport: &mut T,
        connection_id: ConnectionId,
        message: &str,
    ) {
        self.send_direct_event(
            transport,
            connection_id,
            0,
            ServerControlEvent::Error {
                message: message.to_string(),
            },
        );
    }

    fn send_event<T: AppTransport>(
        &mut self,
        transport: &mut T,
        player_id: PlayerId,
        event: ServerControlEvent,
    ) {
        let Some(connection_id) = self.player_connections.get(&player_id).copied() else {
            return;
        };
        let seq = match self.players.get_mut(&player_id) {
            Some(player) => player.next_outbound_seq(),
            None => 0,
        };
        self.send_direct_event(transport, connection_id, seq, event);
    }

    fn send_direct_event<T: AppTransport>(
        &mut self,
        transport: &mut T,
        connection_id: ConnectionId,
        seq: u32,
        event: ServerControlEvent,
    ) {
        let packet = match event.encode_packet(seq, self.clock_seconds) {
            Ok(packet) => packet,
            Err(_) => return,
        };
        transport.send_to_client(connection_id, packet);
    }

    fn broadcast_event<T: AppTransport>(
        &mut self,
        transport: &mut T,
        recipients: &[PlayerId],
        event: ServerControlEvent,
    ) {
        for recipient in recipients {
            self.send_event(transport, *recipient, event.clone());
        }
    }

    fn ensure_location<T: AppTransport>(
        &mut self,
        transport: &mut T,
        player_id: PlayerId,
        expected: PlayerLocation,
    ) -> bool {
        match self.players.get(&player_id) {
            Some(player) if player.location == expected => true,
            Some(_) => {
                self.send_error(
                    transport,
                    player_id,
                    "player is in the wrong state for that command",
                );
                false
            }
            None => {
                self.send_error(transport, player_id, "player is not connected");
                false
            }
        }
    }

    fn require_game_lobby<T: AppTransport>(
        &mut self,
        transport: &mut T,
        player_id: PlayerId,
    ) -> Option<LobbyId> {
        match self.players.get(&player_id) {
            Some(player) => match player.location {
                PlayerLocation::GameLobby(lobby_id) => Some(lobby_id),
                _ => {
                    self.send_error(transport, player_id, "player is not inside a game lobby");
                    None
                }
            },
            None => {
                self.send_error(transport, player_id, "player is not connected");
                None
            }
        }
    }

    fn require_match<T: AppTransport>(
        &mut self,
        transport: &mut T,
        player_id: PlayerId,
    ) -> Option<MatchId> {
        match self.players.get(&player_id) {
            Some(player) => match player.location {
                PlayerLocation::Match(match_id) => Some(match_id),
                _ => {
                    self.send_error(transport, player_id, "player is not inside an active match");
                    None
                }
            },
            None => {
                self.send_error(transport, player_id, "player is not connected");
                None
            }
        }
    }

    fn require_results<T: AppTransport>(
        &mut self,
        transport: &mut T,
        player_id: PlayerId,
    ) -> Option<MatchId> {
        match self.players.get(&player_id) {
            Some(player) => match player.location {
                PlayerLocation::Results(match_id) => Some(match_id),
                _ => {
                    self.send_error(transport, player_id, "player is not on the results screen");
                    None
                }
            },
            None => {
                self.send_error(transport, player_id, "player is not connected");
                None
            }
        }
    }

    fn lobby_members(&self, lobby_id: LobbyId) -> Vec<PlayerId> {
        self.players
            .iter()
            .filter_map(|(player_id, player)| match player.location {
                PlayerLocation::GameLobby(current) if current == lobby_id => Some(*player_id),
                _ => None,
            })
            .collect()
    }

    fn central_lobby_players(&self) -> Vec<PlayerId> {
        self.players
            .iter()
            .filter_map(|(player_id, player)| match player.location {
                PlayerLocation::CentralLobby => Some(*player_id),
                _ => None,
            })
            .collect()
    }

    fn match_recipients(&self, match_id: MatchId) -> Vec<PlayerId> {
        self.matches
            .get(&match_id)
            .map(|runtime| {
                runtime
                    .participants
                    .iter()
                    .copied()
                    .filter(|player_id| self.players.contains_key(player_id))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn cleanup_empty_lobby(&mut self, lobby_id: LobbyId) {
        let empty = self
            .game_lobbies
            .get(&lobby_id)
            .is_some_and(|runtime| runtime.lobby.player_count() == 0);
        if empty {
            self.game_lobbies.remove(&lobby_id);
        }
    }

    fn cleanup_finished_match(&mut self, match_id: MatchId) {
        let still_present = self.players.values().any(|player| {
            matches!(player.location, PlayerLocation::Match(current) | PlayerLocation::Results(current) if current == match_id)
        });
        if !still_present {
            self.matches.remove(&match_id);
        }
    }

    fn remove_player_connection(&mut self, player_id: PlayerId) {
        if let Some(connection_id) = self.player_connections.remove(&player_id) {
            self.connections.remove(&connection_id);
        }
    }

    fn allocate_lobby_id(&mut self) -> LobbyId {
        let lobby_id = match LobbyId::new(self.next_lobby_id) {
            Ok(lobby_id) => lobby_id,
            Err(error) => panic!("generated lobby id should be valid: {error}"),
        };
        self.next_lobby_id = self.next_lobby_id.saturating_add(1);
        lobby_id
    }

    fn allocate_match_id(&mut self) -> MatchId {
        let match_id = match MatchId::new(self.next_match_id) {
            Ok(match_id) => match_id,
            Err(error) => panic!("generated match id should be valid: {error}"),
        };
        self.next_match_id = self.next_match_id.saturating_add(1);
        match_id
    }

    fn allocate_player_id(&self) -> Result<PlayerId, String> {
        for _ in 0..64 {
            let mut bytes = [0_u8; 4];
            fill_random(&mut bytes).map_err(|error| {
                format!("failed to allocate a secure player id from the operating system: {error}")
            })?;
            let raw = u32::from_le_bytes(bytes);
            let Ok(player_id) = PlayerId::new(raw) else {
                continue;
            };
            if !self.players.contains_key(&player_id) {
                return Ok(player_id);
            }
        }

        Err(String::from(
            "failed to allocate a unique player id after repeated attempts",
        ))
    }

    #[must_use]
    pub fn player_id_for_connection(&self, connection_id: ConnectionId) -> Option<PlayerId> {
        self.connections.get(&connection_id).copied()
    }

    fn decode_axis(value: i16, field: &'static str) -> Result<i8, String> {
        match value {
            -1..=1 => match i8::try_from(value) {
                Ok(value) => Ok(value),
                Err(_) => Err(format!(
                    "{field}={value} is outside the allowed range -1..=1"
                )),
            },
            _ => Err(format!(
                "{field}={value} is outside the allowed range -1..=1"
            )),
        }
    }
}

fn build_world(
    roster: &[TeamAssignment],
    session: &MatchSession,
    content: &GameContent,
) -> SimulationWorld {
    match SimulationWorld::new(
        roster
            .iter()
            .cloned()
            .map(|assignment| {
                let player_id = assignment.player_id;
                let primary_tree = session
                    .equipped_choice(player_id, 1)
                    .map(|choice| choice.tree)
                    .unwrap_or(game_domain::SkillTree::Warrior);
                let melee = if let Some(melee) = content.skills().melee_for(primary_tree) {
                    melee.clone()
                } else if let Some(melee) =
                    content.skills().melee_for(game_domain::SkillTree::Warrior)
                {
                    melee.clone()
                } else {
                    panic!("validated content should always define warrior melee");
                };
                SimPlayerSeed {
                    assignment,
                    hit_points: DEFAULT_HIT_POINTS,
                    melee,
                    skills: std::array::from_fn(|index| {
                        session
                            .equipped_choice(player_id, u8::try_from(index + 1).unwrap_or(5))
                            .and_then(|choice| content.skills().resolve(choice).cloned())
                    }),
                }
            })
            .collect(),
        content.map(),
    ) {
        Ok(world) => world,
        Err(error) => panic!("valid match roster should build a simulation world: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::transport::{ConnectionId, HeadlessClient, InMemoryTransport};
    use game_domain::{PlayerName, SkillTree};
    use game_net::{LobbyDirectoryEntry, LobbySnapshotPlayer, ServerControlEvent};

    fn connection_id(raw: u64) -> ConnectionId {
        ConnectionId::new(raw).expect("valid connection id")
    }

    fn player_name(raw: &str) -> PlayerName {
        PlayerName::new(raw).expect("valid player name")
    }

    fn skill(tree: SkillTree, tier: u8) -> SkillChoice {
        SkillChoice::new(tree, tier).expect("valid skill choice")
    }

    fn temp_path(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should move forward")
            .as_nanos();
        std::env::temp_dir()
            .join("rusaren-tests")
            .join(format!("{label}-{}-{unique}.tsv", std::process::id()))
    }

    fn remove_if_exists(path: &PathBuf) {
        if path.exists() {
            fs::remove_file(path).expect("temp file should be removable");
        }
        if let Some(parent) = path.parent() {
            if parent.exists() {
                let _ = fs::remove_dir(parent);
            }
        }
    }

    fn assert_connected(events: &[ServerControlEvent], player_id: PlayerId, player_name: &str) {
        assert!(events.iter().any(|event| matches!(
            event,
            ServerControlEvent::Connected {
                player_id: connected_id,
                player_name: connected_name,
                ..
            } if *connected_id == player_id && connected_name.as_str() == player_name
        )));
    }

    fn assert_directory_lobby_count(events: &[ServerControlEvent], expected_count: usize) {
        assert!(events.iter().any(|event| matches!(
            event,
            ServerControlEvent::LobbyDirectorySnapshot { lobbies }
                if lobbies.len() == expected_count
        )));
    }

    fn lobby_directory(entries: &[ServerControlEvent]) -> Option<&[LobbyDirectoryEntry]> {
        entries.iter().rev().find_map(|event| match event {
            ServerControlEvent::LobbyDirectorySnapshot { lobbies } => Some(lobbies.as_slice()),
            _ => None,
        })
    }

    fn slot_one_cast_input(client_input_tick: u32) -> ValidatedInputFrame {
        ValidatedInputFrame::new(client_input_tick, 0, 0, 0, 0, BUTTON_CAST, 1)
            .expect("slot one cast input should be valid")
    }

    fn cast_until_round_won(
        server: &mut ServerApp,
        transport: &mut InMemoryTransport,
        alice: &mut HeadlessClient,
        bob: &mut HeadlessClient,
        round: u8,
    ) -> (Vec<ServerControlEvent>, Vec<ServerControlEvent>) {
        let mut alice_events = Vec::new();
        let mut bob_events = Vec::new();

        for offset in 0_u32..18 {
            let sequence = u32::from(round) * 100 + offset + 1;
            alice
                .send_input(transport, slot_one_cast_input(sequence), sequence)
                .expect("attack packet");
            server.pump_transport(transport);
            alice_events.extend(alice.drain_events(transport).expect("alice input events"));
            bob_events.extend(bob.drain_events(transport).expect("bob input events"));

            for _ in 0..12 {
                server.advance_millis(transport, COMBAT_FRAME_MS);
                alice_events.extend(alice.drain_events(transport).expect("alice combat events"));
                bob_events.extend(bob.drain_events(transport).expect("bob combat events"));
                if alice_events.iter().any(|event| matches!(
                    event,
                    ServerControlEvent::RoundWon { round: won_round, .. } if won_round.get() == round
                )) {
                    return (alice_events, bob_events);
                }
            }
        }

        panic!("expected round {round} to end after repeated slot-one casts");
    }

    fn lobby_snapshot_players(entries: &[ServerControlEvent]) -> Option<&[LobbySnapshotPlayer]> {
        entries.iter().rev().find_map(|event| match event {
            ServerControlEvent::GameLobbySnapshot { players, .. } => Some(players.as_slice()),
            _ => None,
        })
    }

    fn connect_player(
        server: &mut ServerApp,
        transport: &mut InMemoryTransport,
        raw_connection_id: u64,
        raw_name: &str,
    ) -> HeadlessClient {
        let mut client =
            HeadlessClient::new(connection_id(raw_connection_id), player_name(raw_name));
        client.connect(transport).expect("connect packet");
        server.pump_transport(transport);

        let events = client.drain_events(transport).expect("connect events");
        assert_connected(
            &events,
            client
                .player_id()
                .expect("headless client should receive Connected before assertions"),
            raw_name,
        );
        assert_directory_lobby_count(&events, 0);
        client
    }

    fn connect_pair(
        server: &mut ServerApp,
        transport: &mut InMemoryTransport,
    ) -> (HeadlessClient, HeadlessClient) {
        let alice = connect_player(server, transport, 1, "Alice");
        let bob = connect_player(server, transport, 2, "Bob");
        assert_ne!(
            alice
                .player_id()
                .expect("alice should be connected before uniqueness checks"),
            bob.player_id()
                .expect("bob should be connected before uniqueness checks"),
            "server-assigned player ids should be unique across live clients"
        );
        (alice, bob)
    }

    fn lobby_id_from(events: &[ServerControlEvent]) -> LobbyId {
        events
            .iter()
            .find_map(|event| match event {
                ServerControlEvent::GameLobbyCreated { lobby_id } => Some(*lobby_id),
                _ => None,
            })
            .expect("game lobby should exist")
    }

    fn launch_match(
        server: &mut ServerApp,
        transport: &mut InMemoryTransport,
        alice: &mut HeadlessClient,
        bob: &mut HeadlessClient,
    ) -> MatchId {
        alice.create_game_lobby(transport).expect("create lobby");
        server.pump_transport(transport);
        let alice_events = alice.drain_events(transport).expect("alice events");
        let lobby_id = lobby_id_from(&alice_events);
        assert_eq!(
            lobby_snapshot_players(&alice_events)
                .expect("creator should receive a full lobby snapshot")
                .len(),
            1
        );

        bob.join_game_lobby(transport, lobby_id)
            .expect("join lobby");
        server.pump_transport(transport);
        let alice_join_events = alice.drain_events(transport).expect("alice join events");
        let bob_join_events = bob.drain_events(transport).expect("bob join events");
        assert_eq!(
            lobby_snapshot_players(&alice_join_events)
                .expect("existing member should receive updated snapshot")
                .len(),
            2
        );
        assert_eq!(
            lobby_snapshot_players(&bob_join_events)
                .expect("late joiner should receive a full lobby snapshot")
                .len(),
            2
        );

        alice
            .select_team(transport, TeamSide::TeamA)
            .expect("alice team");
        bob.select_team(transport, TeamSide::TeamB)
            .expect("bob team");
        server.pump_transport(transport);
        let _ = alice.drain_events(transport).expect("alice select events");
        let _ = bob.drain_events(transport).expect("bob select events");

        alice
            .set_ready(transport, ReadyState::Ready)
            .expect("alice ready");
        bob.set_ready(transport, ReadyState::Ready)
            .expect("bob ready");
        server.pump_transport(transport);
        let alice_events = alice.drain_events(transport).expect("alice ready events");
        let bob_events = bob.drain_events(transport).expect("bob ready events");
        assert!(alice_events
            .iter()
            .any(|event| matches!(event, ServerControlEvent::LaunchCountdownStarted { .. })));
        assert!(bob_events
            .iter()
            .any(|event| matches!(event, ServerControlEvent::LaunchCountdownStarted { .. })));

        server.advance_seconds(transport, 5);
        let alice_events = alice
            .drain_events(transport)
            .expect("alice countdown events");
        let bob_events = bob.drain_events(transport).expect("bob countdown events");

        let match_id = alice_events
            .iter()
            .find_map(|event| match event {
                ServerControlEvent::MatchStarted { match_id, .. } => Some(*match_id),
                _ => None,
            })
            .expect("match should start");
        assert!(alice_events.iter().any(|event| matches!(
            event,
            ServerControlEvent::ArenaStateSnapshot { snapshot }
                if snapshot.players.len() == 2
                    && snapshot.obstacles.len() == server.content.map().obstacles.len()
        )));
        assert!(bob_events.iter().any(|event| matches!(
            event,
            ServerControlEvent::MatchStarted { match_id: other, .. } if *other == match_id
        )));

        match_id
    }

    #[test]
    fn end_to_end_game_lobby_countdown_and_match_start_work_via_fake_clients() {
        let mut server = ServerApp::new();
        let mut transport = InMemoryTransport::new();
        let (mut alice, mut bob) = connect_pair(&mut server, &mut transport);

        let match_id = launch_match(&mut server, &mut transport, &mut alice, &mut bob);
        assert_eq!(match_id.get(), 1);
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn end_to_end_skill_pick_round_flow_match_end_and_quit_work_via_fake_clients() {
        let mut server = ServerApp::new();
        let mut transport = InMemoryTransport::new();
        let (mut alice, mut bob) = connect_pair(&mut server, &mut transport);

        let _ = launch_match(&mut server, &mut transport, &mut alice, &mut bob);

        for tier in 1..=5 {
            alice
                .choose_skill(&mut transport, skill(SkillTree::Mage, tier))
                .expect("alice skill");
            bob.choose_skill(&mut transport, skill(SkillTree::Rogue, tier))
                .expect("bob skill");
            server.pump_transport(&mut transport);
            let alice_events = alice
                .drain_events(&mut transport)
                .expect("alice skill events");
            let bob_events = bob.drain_events(&mut transport).expect("bob skill events");
            assert!(alice_events.iter().any(|event| matches!(
                event,
                ServerControlEvent::PreCombatStarted {
                    seconds_remaining: 5
                }
            )));
            assert!(bob_events.iter().any(|event| matches!(
                event,
                ServerControlEvent::PreCombatStarted {
                    seconds_remaining: 5
                }
            )));

            server.advance_seconds(&mut transport, 5);
            let alice_events = alice
                .drain_events(&mut transport)
                .expect("alice pre-combat events");
            assert!(alice_events
                .iter()
                .any(|event| matches!(event, ServerControlEvent::CombatStarted)));
            let _ = bob
                .drain_events(&mut transport)
                .expect("bob pre-combat events");

            let (alice_events, bob_events) =
                cast_until_round_won(&mut server, &mut transport, &mut alice, &mut bob, tier);
            assert!(alice_events.iter().any(|event| matches!(
                event,
                ServerControlEvent::ArenaEffectBatch { effects }
                    if effects.iter().any(|effect| effect.slot == 1)
            )));
            assert!(alice_events.iter().any(|event| matches!(
                event,
                ServerControlEvent::RoundWon {
                    round,
                    winning_team: TeamSide::TeamA,
                    ..
                } if round.get() == tier
            )));
            assert!(bob_events.iter().any(|event| matches!(
                event,
                ServerControlEvent::RoundWon { round, .. } if round.get() == tier
            )));

            if tier == 5 {
                assert!(alice_events.iter().any(|event| matches!(
                    event,
                    ServerControlEvent::MatchEnded {
                        outcome: MatchOutcome::TeamAWin,
                        score_a: 5,
                        score_b: 0,
                        ..
                    }
                )));
                assert!(bob_events.iter().any(|event| matches!(
                    event,
                    ServerControlEvent::MatchEnded {
                        outcome: MatchOutcome::TeamAWin,
                        score_a: 5,
                        score_b: 0,
                        ..
                    }
                )));
            }
        }

        alice
            .quit_to_central_lobby(&mut transport)
            .expect("alice quit");
        bob.quit_to_central_lobby(&mut transport).expect("bob quit");
        server.pump_transport(&mut transport);

        let alice_events = alice.drain_events(&mut transport).expect("alice return");
        let bob_events = bob.drain_events(&mut transport).expect("bob return");
        assert!(alice_events.iter().any(|event| matches!(
            event,
            ServerControlEvent::ReturnedToCentralLobby { record }
                if record.wins == 1 && record.losses == 0 && record.no_contests == 0
        )));
        assert!(bob_events.iter().any(|event| matches!(
            event,
            ServerControlEvent::ReturnedToCentralLobby { record }
                if record.wins == 0 && record.losses == 1 && record.no_contests == 0
        )));
    }

    #[test]
    fn end_to_end_skill_pick_rejects_tier_skips_but_accepts_the_next_valid_tier() {
        let mut server = ServerApp::new();
        let mut transport = InMemoryTransport::new();
        let (mut alice, mut bob) = connect_pair(&mut server, &mut transport);

        let _ = launch_match(&mut server, &mut transport, &mut alice, &mut bob);

        alice
            .choose_skill(&mut transport, skill(SkillTree::Mage, 5))
            .expect("invalid skill packet should still encode");
        server.pump_transport(&mut transport);
        let alice_events = alice
            .drain_events(&mut transport)
            .expect("alice invalid skill response");
        assert!(alice_events.iter().any(|event| matches!(
            event,
            ServerControlEvent::Error { message }
                if message == "skill progression for Mage expected tier 1 but received tier 5"
        )));

        alice
            .choose_skill(&mut transport, skill(SkillTree::Mage, 1))
            .expect("alice valid tier one");
        bob.choose_skill(&mut transport, skill(SkillTree::Rogue, 1))
            .expect("bob valid tier one");
        server.pump_transport(&mut transport);
        let _ = alice
            .drain_events(&mut transport)
            .expect("alice first-round skill events");
        let _ = bob
            .drain_events(&mut transport)
            .expect("bob first-round skill events");
        server.advance_seconds(&mut transport, 5);
        let _ = alice
            .drain_events(&mut transport)
            .expect("alice first-round pre-combat events");
        let _ = bob
            .drain_events(&mut transport)
            .expect("bob first-round pre-combat events");
        let _ = cast_until_round_won(&mut server, &mut transport, &mut alice, &mut bob, 1);

        alice
            .choose_skill(&mut transport, skill(SkillTree::Mage, 3))
            .expect("invalid second-round skill packet should still encode");
        server.pump_transport(&mut transport);
        let alice_events = alice
            .drain_events(&mut transport)
            .expect("alice second invalid skill response");
        assert!(alice_events.iter().any(|event| matches!(
            event,
            ServerControlEvent::Error { message }
                if message == "skill progression for Mage expected tier 2 but received tier 3"
        )));

        alice
            .choose_skill(&mut transport, skill(SkillTree::Mage, 2))
            .expect("alice valid tier two");
        bob.choose_skill(&mut transport, skill(SkillTree::Rogue, 2))
            .expect("bob valid tier two");
        server.pump_transport(&mut transport);
        let alice_events = alice
            .drain_events(&mut transport)
            .expect("alice second-round skill events");
        assert!(alice_events.iter().any(|event| matches!(
            event,
            ServerControlEvent::PreCombatStarted {
                seconds_remaining: 5
            }
        )));
    }

    #[test]
    fn end_to_end_disconnect_ends_the_match_as_no_contest() {
        let mut server = ServerApp::new();
        let mut transport = InMemoryTransport::new();
        let (mut alice, mut bob) = connect_pair(&mut server, &mut transport);

        let match_id = launch_match(&mut server, &mut transport, &mut alice, &mut bob);

        server
            .disconnect_player(
                &mut transport,
                bob.player_id()
                    .expect("bob should be connected before disconnect"),
            )
            .expect("disconnect should work");
        let alice_events = alice
            .drain_events(&mut transport)
            .expect("alice disconnect events");
        assert!(alice_events.iter().any(|event| matches!(
            event,
            ServerControlEvent::MatchEnded {
                outcome: MatchOutcome::NoContest,
                message,
                ..
            } if message == "Bob has disconnected. Game is over."
        )));

        alice
            .quit_to_central_lobby(&mut transport)
            .expect("alice quit");
        server.pump_transport(&mut transport);
        let alice_events = alice
            .drain_events(&mut transport)
            .expect("alice return events");
        assert!(alice_events.iter().any(|event| matches!(
            event,
            ServerControlEvent::ReturnedToCentralLobby { record }
                if record.wins == 0 && record.losses == 0 && record.no_contests == 1
        )));
        assert!(!server.matches.contains_key(&match_id));
    }

    #[test]
    fn end_to_end_rejects_invalid_sequences_and_wrong_state_commands() {
        let mut server = ServerApp::new();
        let mut transport = InMemoryTransport::new();
        let (mut alice, mut bob) = connect_pair(&mut server, &mut transport);

        bob.leave_game_lobby(&mut transport).expect("leave packet");
        server.pump_transport(&mut transport);
        let bob_events = bob.drain_events(&mut transport).expect("bob error");
        assert!(bob_events.iter().any(|event| matches!(
            event,
            ServerControlEvent::Error { message }
                if message == "player is not inside a game lobby"
        )));

        let stale = ClientControlCommand::CreateGameLobby
            .encode_packet(1, 0)
            .expect("stale packet");
        transport.send_from_client(alice.connection_id(), stale);
        server.pump_transport(&mut transport);
        let alice_events = alice
            .drain_events(&mut transport)
            .expect("alice stale error");
        assert!(alice_events.iter().any(|event| matches!(
            event,
            ServerControlEvent::Error { message } if message.contains("incoming sequence")
        )));
    }

    #[test]
    fn central_lobby_receives_directory_snapshots_as_lobbies_change() {
        let mut server = ServerApp::new();
        let mut transport = InMemoryTransport::new();
        let mut alice = connect_player(&mut server, &mut transport, 1, "Alice");
        let mut bob = connect_player(&mut server, &mut transport, 2, "Bob");
        let mut charlie = connect_player(&mut server, &mut transport, 3, "Charlie");

        alice
            .create_game_lobby(&mut transport)
            .expect("create lobby");
        server.pump_transport(&mut transport);
        let alice_events = alice
            .drain_events(&mut transport)
            .expect("alice create events");
        let lobby_id = lobby_id_from(&alice_events);
        let bob_events = bob
            .drain_events(&mut transport)
            .expect("bob directory events");
        let charlie_events = charlie
            .drain_events(&mut transport)
            .expect("charlie directory events");
        for events in [&bob_events, &charlie_events] {
            let directory = lobby_directory(events).expect("central players should see lobbies");
            assert_eq!(directory.len(), 1);
            assert_eq!(directory[0].player_count, 1);
        }

        bob.join_game_lobby(&mut transport, lobby_id)
            .expect("join lobby");
        server.pump_transport(&mut transport);
        let _ = alice
            .drain_events(&mut transport)
            .expect("alice join events");
        let _ = bob.drain_events(&mut transport).expect("bob join events");
        let charlie_events = charlie
            .drain_events(&mut transport)
            .expect("charlie updated directory");
        let directory = lobby_directory(&charlie_events).expect("directory snapshot");
        assert_eq!(directory.len(), 1);
        assert_eq!(directory[0].player_count, 2);

        bob.leave_game_lobby(&mut transport).expect("leave lobby");
        server.pump_transport(&mut transport);
        let _ = alice
            .drain_events(&mut transport)
            .expect("alice leave events");
        let bob_events = bob.drain_events(&mut transport).expect("bob leave events");
        let charlie_events = charlie
            .drain_events(&mut transport)
            .expect("charlie leave directory");
        assert!(bob_events
            .iter()
            .any(|event| matches!(event, ServerControlEvent::ReturnedToCentralLobby { .. })));
        let directory = lobby_directory(&charlie_events).expect("directory snapshot");
        assert_eq!(directory.len(), 1);
        assert_eq!(directory[0].player_count, 1);
    }

    #[test]
    fn persistent_player_records_survive_reconnect() {
        let path = temp_path("server-app-records");
        remove_if_exists(&path);

        let mut server = ServerApp::new_persistent(&path).expect("persistent server should start");
        let mut transport = InMemoryTransport::new();
        let (mut alice, mut bob) = connect_pair(&mut server, &mut transport);
        let _ = launch_match(&mut server, &mut transport, &mut alice, &mut bob);

        for tier in 1..=5 {
            alice
                .choose_skill(&mut transport, skill(SkillTree::Mage, tier))
                .expect("alice skill");
            bob.choose_skill(&mut transport, skill(SkillTree::Rogue, tier))
                .expect("bob skill");
            server.pump_transport(&mut transport);
            let _ = alice
                .drain_events(&mut transport)
                .expect("alice skill events");
            let _ = bob.drain_events(&mut transport).expect("bob skill events");
            server.advance_seconds(&mut transport, 5);
            let _ = alice
                .drain_events(&mut transport)
                .expect("alice pre-combat events");
            let _ = bob
                .drain_events(&mut transport)
                .expect("bob pre-combat events");
            let _ = cast_until_round_won(&mut server, &mut transport, &mut alice, &mut bob, tier);
        }

        alice
            .quit_to_central_lobby(&mut transport)
            .expect("alice quit");
        bob.quit_to_central_lobby(&mut transport).expect("bob quit");
        server.pump_transport(&mut transport);
        let _ = alice.drain_events(&mut transport).expect("alice return");
        let _ = bob.drain_events(&mut transport).expect("bob return");
        server
            .disconnect_player(
                &mut transport,
                alice
                    .player_id()
                    .expect("alice should be connected before disconnect"),
            )
            .expect("alice disconnect");
        server
            .disconnect_player(
                &mut transport,
                bob.player_id()
                    .expect("bob should be connected before disconnect"),
            )
            .expect("bob disconnect");

        let mut reloaded =
            ServerApp::new_persistent(&path).expect("persistent server should reload");
        let mut transport = InMemoryTransport::new();
        let mut alice = HeadlessClient::new(connection_id(9), player_name("Alice"));
        alice.connect(&mut transport).expect("connect packet");
        reloaded.pump_transport(&mut transport);

        let events = alice
            .drain_events(&mut transport)
            .expect("alice reconnect events");
        assert!(events.iter().any(|event| matches!(
            event,
            ServerControlEvent::Connected { record, .. }
                if record.wins == 1 && record.losses == 0 && record.no_contests == 0
        )));

        remove_if_exists(&path);
    }
}
