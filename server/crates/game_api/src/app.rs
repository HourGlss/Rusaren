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

use game_content::{ArenaMapDefinition, GameContent};
use game_domain::{
    LobbyId, MatchId, MatchOutcome, PlayerId, PlayerName, PlayerRecord, ReadyState, SkillChoice,
    TeamAssignment, TeamSide,
};
use game_lobby::{Lobby, LobbyEvent, LobbyPhase};
use game_match::{MatchConfig, MatchEvent, MatchPhase, MatchSession};
use game_net::{
    ArenaDeltaSnapshot, ArenaEffectKind, ArenaEffectSnapshot, ArenaMatchPhase, ArenaObstacleKind,
    ArenaObstacleSnapshot, ArenaPlayerSnapshot, ArenaProjectileSnapshot, ArenaStateSnapshot,
    ArenaStatusKind, ArenaStatusSnapshot, ClientControlCommand, LobbyDirectoryEntry,
    LobbySnapshotPhase, LobbySnapshotPlayer, SequenceTracker, ServerControlEvent,
    SkillCatalogEntry, ValidatedInputFrame, BUTTON_CAST, BUTTON_PRIMARY, BUTTON_QUIT_TO_LOBBY,
};
use game_sim::{
    obstacle_blocks_vision, obstacle_contains_point, segment_hits_obstacle, ArenaEffect,
    ArenaObstacle, ArenaObstacleKind as SimArenaObstacleKind, MovementIntent, SimPlayerSeed,
    SimulationEvent, SimulationWorld, COMBAT_FRAME_MS, VISION_RADIUS_UNITS,
};
use getrandom::fill as fill_random;

mod ingress;
mod lifecycle;
mod snapshots;
mod support;

use crate::records::PlayerRecordStore;
use crate::{
    transport::{AppTransport, ConnectionId},
    RecordStoreError,
};

const DEFAULT_HIT_POINTS: u16 = 100;

/// Errors returned by the server application orchestration layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppError {
    /// The requested player is not currently connected to the server app.
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
    newest_client_input_tick: Option<u32>,
    next_outbound_seq: u32,
}

impl ConnectedPlayer {
    fn next_outbound_seq(&mut self) -> u32 {
        self.next_outbound_seq = self.next_outbound_seq.saturating_add(1);
        self.next_outbound_seq
    }

    fn observe_client_input_tick(&mut self, client_input_tick: u32) -> Result<(), String> {
        if let Some(newest_client_input_tick) = self.newest_client_input_tick {
            if client_input_tick <= newest_client_input_tick {
                return Err(format!(
                    "client_input_tick {client_input_tick} is not newer than {newest_client_input_tick}"
                ));
            }
        }

        self.newest_client_input_tick = Some(client_input_tick);
        Ok(())
    }

    fn reset_combat_input_state(&mut self) {
        self.inbound_input = SequenceTracker::new();
        self.newest_client_input_tick = None;
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
    explored_tiles: BTreeMap<PlayerId, Vec<u8>>,
}

impl MatchRuntime {
    fn rebuild_world(&mut self, content: &GameContent) {
        self.world = build_world(&self.roster, &self.session, content);
    }
}

/// High-level authoritative application state for lobbies, matches, and persistence.
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
    /// Creates an ephemeral server app loaded from bundled content.
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

    /// Creates a server app backed by a persistent on-disk record store.
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

    /// Creates an ephemeral server app from explicit runtime content.
    #[must_use]
    pub fn new_with_content(content: GameContent) -> Self {
        Self::from_content_and_record_store(content, PlayerRecordStore::new_ephemeral())
    }

    /// Creates a persistent server app from explicit runtime content.
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

    /// Drains all currently queued transport packets and applies them to server state.
    pub fn pump_transport<T: AppTransport>(&mut self, transport: &mut T) {
        while let Some((connection_id, packet)) = transport.recv_from_client() {
            self.handle_packet(transport, connection_id, &packet);
        }
    }

    /// Advances the application clock by a number of milliseconds.
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

    /// Advances the application clock in whole-second steps.
    pub fn advance_seconds<T: AppTransport>(&mut self, transport: &mut T, seconds: u8) {
        for _ in 0..seconds {
            self.advance_millis(transport, 1000);
        }
    }

    /// Disconnects one player and applies the correct lobby or match-side cleanup.
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

    /// Disconnects one connection id if it is bound to a player.
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
                let melee = if let Some(melee) = content.skills().melee_for(&primary_tree) {
                    melee.clone()
                } else if let Some(melee) =
                    content.skills().melee_for(&game_domain::SkillTree::Warrior)
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
                            .and_then(|choice| content.skills().resolve(&choice).cloned())
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
mod tests;
