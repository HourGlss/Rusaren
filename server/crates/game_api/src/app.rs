#![allow(
    clippy::manual_let_else,
    clippy::map_unwrap_or,
    clippy::needless_pass_by_value,
    clippy::semicolon_if_nothing_returned,
    clippy::single_match_else
)]

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use std::path::PathBuf;

use game_content::{ArenaMapDefinition, GameContent};
use game_domain::{
    LobbyId, MatchId, PlayerId, PlayerName, PlayerRecord, SkillChoice, TeamAssignment, TeamSide,
};
use game_lobby::{Lobby, LobbyEvent, LobbyPhase};
use game_match::MatchSession;
use game_net::{SequenceTracker, ServerControlEvent};
use game_sim::{SimPlayerSeed, SimulationWorld, COMBAT_FRAME_MS};
use getrandom::fill as fill_random;

mod ingress;
mod lifecycle;
mod snapshots;
mod support;

use crate::records::PlayerRecordStore;
use crate::{
    combat_feedback::MatchCombatFeedback,
    combat_log::{
        CombatLogCastCancelReason, CombatLogCastMode, CombatLogEntry, CombatLogEvent,
        CombatLogMissReason, CombatLogOutcome, CombatLogPhase, CombatLogRemovedStatus,
        CombatLogStatusRemovedReason, CombatLogStore, CombatLogStoreError, CombatLogTargetKind,
        CombatLogTeam, CombatLogTriggerReason,
    },
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
    Training(MatchId),
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
    combat_frame_index: u32,
    feedback: MatchCombatFeedback,
}

impl MatchRuntime {
    fn rebuild_world(&mut self, content: &GameContent) {
        self.world = build_world(&self.roster, &self.session, content);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TrainingMetrics {
    damage_done: u32,
    healing_done: u32,
    elapsed_ms: u32,
}

#[derive(Debug)]
struct TrainingRuntime {
    participant: TeamAssignment,
    loadout: [Option<SkillChoice>; 5],
    world: SimulationWorld,
    explored_tiles: BTreeMap<PlayerId, Vec<u8>>,
    combat_frame_index: u32,
    metrics: TrainingMetrics,
}

impl TrainingRuntime {
    fn rebuild_world(&mut self, content: &GameContent) {
        self.world = build_training_world(&self.participant, &self.loadout, content);
    }

    fn reset_session(&mut self) {
        self.world.reset_training_session();
        self.metrics = TrainingMetrics::default();
    }
}

/// Errors returned while opening the server app's persistent stores.
#[derive(Debug)]
pub enum ServerAppPersistenceError {
    /// Opening the player record store failed.
    RecordStore(RecordStoreError),
    /// Opening the combat log store failed.
    CombatLog(CombatLogStoreError),
}

impl fmt::Display for ServerAppPersistenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RecordStore(error) => error.fmt(f),
            Self::CombatLog(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ServerAppPersistenceError {}

impl From<RecordStoreError> for ServerAppPersistenceError {
    fn from(value: RecordStoreError) -> Self {
        Self::RecordStore(value)
    }
}

impl From<CombatLogStoreError> for ServerAppPersistenceError {
    fn from(value: CombatLogStoreError) -> Self {
        Self::CombatLog(value)
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
    combat_log: CombatLogStore,
    connections: BTreeMap<ConnectionId, PlayerId>,
    player_connections: BTreeMap<PlayerId, ConnectionId>,
    players: BTreeMap<PlayerId, ConnectedPlayer>,
    game_lobbies: BTreeMap<LobbyId, GameLobbyRuntime>,
    matches: BTreeMap<MatchId, MatchRuntime>,
    training_sessions: BTreeMap<MatchId, TrainingRuntime>,
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
        Self::from_content_and_stores(
            content,
            PlayerRecordStore::new_ephemeral(),
            CombatLogStore::new_ephemeral().expect("ephemeral combat log should open"),
        )
    }

    /// Creates a server app backed by a persistent on-disk record store.
    pub fn new_persistent(path: impl Into<PathBuf>) -> Result<Self, ServerAppPersistenceError> {
        let content = match GameContent::bundled() {
            Ok(content) => content,
            Err(error) => {
                panic!("bundled content must load for tests and local development: {error}")
            }
        };
        let record_store_path = path.into();
        let combat_log_path = companion_combat_log_path(&record_store_path);
        Ok(Self::from_content_and_stores(
            content,
            PlayerRecordStore::new_persistent(record_store_path)?,
            CombatLogStore::new_persistent(combat_log_path)?,
        ))
    }

    /// Creates an ephemeral server app from explicit runtime content.
    #[must_use]
    pub fn new_with_content(content: GameContent) -> Self {
        Self::from_content_and_stores(
            content,
            PlayerRecordStore::new_ephemeral(),
            CombatLogStore::new_ephemeral().expect("ephemeral combat log should open"),
        )
    }

    /// Creates a persistent server app from explicit runtime content.
    pub fn new_persistent_with_content(
        content: GameContent,
        path: impl Into<PathBuf>,
    ) -> Result<Self, ServerAppPersistenceError> {
        let record_store_path = path.into();
        let combat_log_path = companion_combat_log_path(&record_store_path);
        Ok(Self::from_content_and_stores(
            content,
            PlayerRecordStore::new_persistent(record_store_path)?,
            CombatLogStore::new_persistent(combat_log_path)?,
        ))
    }

    /// Creates a persistent server app from explicit runtime content and explicit store paths.
    pub fn new_persistent_with_content_and_log(
        content: GameContent,
        record_store_path: impl Into<PathBuf>,
        combat_log_path: impl Into<PathBuf>,
    ) -> Result<Self, ServerAppPersistenceError> {
        Ok(Self::from_content_and_stores(
            content,
            PlayerRecordStore::new_persistent(record_store_path.into())?,
            CombatLogStore::new_persistent(combat_log_path.into())?,
        ))
    }

    fn from_content_and_stores(
        content: GameContent,
        record_store: PlayerRecordStore,
        combat_log: CombatLogStore,
    ) -> Self {
        Self {
            content,
            next_lobby_id: 1,
            next_match_id: 1,
            clock_seconds: 0,
            phase_accumulator_ms: 0,
            combat_accumulator_ms: 0,
            record_store,
            combat_log,
            connections: BTreeMap::new(),
            player_connections: BTreeMap::new(),
            players: BTreeMap::new(),
            game_lobbies: BTreeMap::new(),
            matches: BTreeMap::new(),
            training_sessions: BTreeMap::new(),
        }
    }

    /// Returns the number of currently connected players.
    #[must_use]
    pub fn connected_player_count(&self) -> usize {
        self.players.len()
    }

    /// Returns the number of currently bound transport connections.
    #[must_use]
    pub fn bound_connection_count(&self) -> usize {
        self.connections.len()
    }

    /// Returns the number of players currently in the central lobby.
    #[must_use]
    pub fn central_lobby_player_count(&self) -> usize {
        self.players
            .values()
            .filter(|player| matches!(player.location, PlayerLocation::CentralLobby))
            .count()
    }

    /// Returns the number of active game lobbies.
    #[must_use]
    pub fn active_lobby_count(&self) -> usize {
        self.game_lobbies.len()
    }

    /// Returns the number of active matches.
    #[must_use]
    pub fn active_match_count(&self) -> usize {
        self.matches.len()
    }

    /// Returns every durable combat-log row for one match in append order.
    pub fn combat_log_entries(
        &self,
        match_id: MatchId,
    ) -> Result<Vec<CombatLogEntry>, CombatLogStoreError> {
        self.combat_log.events_for_match(match_id)
    }

    pub(super) fn append_match_log(
        &mut self,
        match_id: MatchId,
        event: CombatLogEvent,
    ) -> Result<(), CombatLogStoreError> {
        let Some(runtime) = self.matches.get(&match_id) else {
            return Ok(());
        };
        let entry = CombatLogEntry::new(
            match_id,
            runtime.session.current_round().get(),
            phase_for_session(&runtime.session),
            runtime.combat_frame_index,
            event,
        );
        self.record_combat_entry(match_id, entry)
    }

    pub(super) fn append_simulation_logs(
        &mut self,
        match_id: MatchId,
        simulation_events: &[game_sim::SimulationEvent],
    ) -> Result<(), CombatLogStoreError> {
        let Some(runtime) = self.matches.get(&match_id) else {
            return Ok(());
        };
        let round = runtime.session.current_round().get();
        let phase = phase_for_session(&runtime.session);
        let frame_index = runtime.combat_frame_index;
        let entries = simulation_events
            .iter()
            .filter_map(map_simulation_event_to_log)
            .map(|event| CombatLogEntry::new(match_id, round, phase, frame_index, event))
            .collect::<Vec<_>>();
        for entry in entries {
            self.record_combat_entry(match_id, entry)?;
        }
        Ok(())
    }

    pub(super) fn drain_match_combat_text(
        &mut self,
        match_id: MatchId,
    ) -> Vec<(PlayerId, Vec<game_net::ArenaCombatTextEntry>)> {
        self.matches
            .get_mut(&match_id)
            .map(|runtime| runtime.feedback.drain_pending_text())
            .unwrap_or_default()
    }

    fn record_combat_entry(
        &mut self,
        match_id: MatchId,
        entry: CombatLogEntry,
    ) -> Result<(), CombatLogStoreError> {
        if let Some(runtime) = self.matches.get_mut(&match_id) {
            runtime.feedback.observe_entry(
                &self.content,
                &runtime.roster,
                &runtime.session,
                &runtime.world,
                &entry,
            );
        }
        self.combat_log.append(&entry)
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
            PlayerLocation::Training(training_id) => {
                self.players.remove(&player_id);
                self.remove_player_connection(player_id);
                self.cleanup_finished_training(training_id);
                self.broadcast_lobby_directory_snapshot(transport);
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

fn companion_combat_log_path(record_store_path: &Path) -> PathBuf {
    let stem = record_store_path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("player_records");
    let file_name = format!("{stem}.combat.sqlite");
    record_store_path.parent().map_or_else(
        || PathBuf::from(&file_name),
        |parent| parent.join(&file_name),
    )
}

fn phase_for_session(session: &MatchSession) -> CombatLogPhase {
    match session.phase() {
        game_match::MatchPhase::SkillPick { .. } => CombatLogPhase::SkillPick,
        game_match::MatchPhase::PreCombat { .. } => CombatLogPhase::PreCombat,
        game_match::MatchPhase::Combat => CombatLogPhase::Combat,
        game_match::MatchPhase::MatchEnd { .. } => CombatLogPhase::MatchEnd,
    }
}

fn map_team(team: TeamSide) -> CombatLogTeam {
    match team {
        TeamSide::TeamA => CombatLogTeam::TeamA,
        TeamSide::TeamB => CombatLogTeam::TeamB,
    }
}

fn map_outcome(outcome: game_domain::MatchOutcome) -> CombatLogOutcome {
    match outcome {
        game_domain::MatchOutcome::TeamAWin => CombatLogOutcome::TeamAWin,
        game_domain::MatchOutcome::TeamBWin => CombatLogOutcome::TeamBWin,
        game_domain::MatchOutcome::NoContest => CombatLogOutcome::NoContest,
    }
}

fn status_kind_label(kind: game_content::StatusKind) -> &'static str {
    match kind {
        game_content::StatusKind::Poison => "poison",
        game_content::StatusKind::Hot => "hot",
        game_content::StatusKind::Chill => "chill",
        game_content::StatusKind::Root => "root",
        game_content::StatusKind::Haste => "haste",
        game_content::StatusKind::Silence => "silence",
        game_content::StatusKind::Stun => "stun",
        game_content::StatusKind::Sleep => "sleep",
        game_content::StatusKind::Shield => "shield",
        game_content::StatusKind::Stealth => "stealth",
        game_content::StatusKind::Reveal => "reveal",
        game_content::StatusKind::Fear => "fear",
    }
}

fn payload_kind_label(kind: game_content::CombatValueKind) -> &'static str {
    match kind {
        game_content::CombatValueKind::Damage => "damage",
        game_content::CombatValueKind::Heal => "heal",
    }
}

fn dispel_scope_label(scope: game_content::DispelScope) -> &'static str {
    match scope {
        game_content::DispelScope::Positive => "positive",
        game_content::DispelScope::Negative => "negative",
        game_content::DispelScope::All => "all",
    }
}

fn map_trigger_reason(reason: game_sim::SimTriggerReason) -> CombatLogTriggerReason {
    match reason {
        game_sim::SimTriggerReason::Expire => CombatLogTriggerReason::Expire,
        game_sim::SimTriggerReason::Dispel => CombatLogTriggerReason::Dispel,
    }
}

fn map_status_removed_reason(
    reason: game_sim::SimStatusRemovedReason,
) -> CombatLogStatusRemovedReason {
    match reason {
        game_sim::SimStatusRemovedReason::Expired => CombatLogStatusRemovedReason::Expired,
        game_sim::SimStatusRemovedReason::Dispelled => CombatLogStatusRemovedReason::Dispelled,
        game_sim::SimStatusRemovedReason::DamageBroken => {
            CombatLogStatusRemovedReason::DamageBroken
        }
        game_sim::SimStatusRemovedReason::Defeat => CombatLogStatusRemovedReason::Defeat,
        game_sim::SimStatusRemovedReason::ShieldConsumed => {
            CombatLogStatusRemovedReason::ShieldConsumed
        }
    }
}

fn map_target_kind(kind: game_sim::SimTargetKind) -> CombatLogTargetKind {
    match kind {
        game_sim::SimTargetKind::Player => CombatLogTargetKind::Player,
        game_sim::SimTargetKind::Deployable => CombatLogTargetKind::Deployable,
    }
}

fn map_cancel_reason(reason: game_sim::SimCastCancelReason) -> CombatLogCastCancelReason {
    match reason {
        game_sim::SimCastCancelReason::Manual => CombatLogCastCancelReason::Manual,
        game_sim::SimCastCancelReason::Movement => CombatLogCastCancelReason::Movement,
        game_sim::SimCastCancelReason::ControlLoss => CombatLogCastCancelReason::ControlLoss,
        game_sim::SimCastCancelReason::Defeat => CombatLogCastCancelReason::Defeat,
        game_sim::SimCastCancelReason::Interrupt => CombatLogCastCancelReason::Interrupt,
    }
}

fn map_cast_mode(mode: game_sim::SimCastMode) -> CombatLogCastMode {
    match mode {
        game_sim::SimCastMode::Windup => CombatLogCastMode::Windup,
        game_sim::SimCastMode::Channel => CombatLogCastMode::Channel,
    }
}

fn map_miss_reason(reason: game_sim::SimMissReason) -> CombatLogMissReason {
    match reason {
        game_sim::SimMissReason::NoTarget => CombatLogMissReason::NoTarget,
        game_sim::SimMissReason::Blocked => CombatLogMissReason::Blocked,
        game_sim::SimMissReason::Expired => CombatLogMissReason::Expired,
    }
}

fn map_removed_status(status: game_sim::SimRemovedStatus) -> CombatLogRemovedStatus {
    CombatLogRemovedStatus {
        source_player_id: status.source.get(),
        slot: status.slot,
        status_kind: status_kind_label(status.kind).to_string(),
        stacks: status.stacks,
        remaining_ms: status.remaining_ms,
    }
}

fn map_simulation_event_to_log(event: &game_sim::SimulationEvent) -> Option<CombatLogEvent> {
    match event {
        game_sim::SimulationEvent::PlayerMoved { .. }
        | game_sim::SimulationEvent::EffectSpawned { .. } => None,
        game_sim::SimulationEvent::DamageApplied {
            attacker,
            target,
            slot,
            amount,
            remaining_hit_points,
            defeated,
            status_kind,
            trigger,
        } => Some(CombatLogEvent::DamageApplied {
            source_player_id: attacker.get(),
            target_kind: CombatLogTargetKind::Player,
            target_id: target.get(),
            slot: *slot,
            amount: *amount,
            remaining_hit_points: *remaining_hit_points,
            defeated: *defeated,
            status_kind: status_kind.map(|kind| status_kind_label(kind).to_string()),
            trigger: trigger.map(map_trigger_reason),
        }),
        game_sim::SimulationEvent::HealingApplied {
            source,
            target,
            slot,
            amount,
            resulting_hit_points,
            status_kind,
            trigger,
        } => Some(CombatLogEvent::HealingApplied {
            source_player_id: source.get(),
            target_player_id: target.get(),
            slot: *slot,
            amount: *amount,
            resulting_hit_points: *resulting_hit_points,
            status_kind: status_kind.map(|kind| status_kind_label(kind).to_string()),
            trigger: trigger.map(map_trigger_reason),
        }),
        game_sim::SimulationEvent::StatusApplied {
            source,
            target,
            slot,
            kind,
            stacks,
            stack_delta,
            remaining_ms,
        } => Some(CombatLogEvent::StatusApplied {
            source_player_id: source.get(),
            target_player_id: target.get(),
            slot: *slot,
            status_kind: status_kind_label(*kind).to_string(),
            stacks: *stacks,
            stack_delta: *stack_delta,
            remaining_ms: *remaining_ms,
        }),
        game_sim::SimulationEvent::StatusRemoved {
            source,
            target,
            slot,
            kind,
            stacks,
            remaining_ms,
            reason,
        } => Some(CombatLogEvent::StatusRemoved {
            source_player_id: source.get(),
            target_player_id: target.get(),
            slot: *slot,
            status_kind: status_kind_label(*kind).to_string(),
            stacks: *stacks,
            remaining_ms: *remaining_ms,
            reason: map_status_removed_reason(*reason),
        }),
        game_sim::SimulationEvent::CastStarted {
            player_id,
            slot,
            behavior,
            mode,
            total_ms,
        } => Some(CombatLogEvent::CastStarted {
            player_id: player_id.get(),
            slot: *slot,
            behavior: (*behavior).to_string(),
            mode: map_cast_mode(*mode),
            total_ms: *total_ms,
        }),
        game_sim::SimulationEvent::CastCompleted {
            player_id,
            slot,
            behavior,
        } => Some(CombatLogEvent::CastCompleted {
            player_id: player_id.get(),
            slot: *slot,
            behavior: (*behavior).to_string(),
        }),
        game_sim::SimulationEvent::CastCanceled {
            player_id,
            slot,
            reason,
        } => Some(CombatLogEvent::CastCanceled {
            player_id: player_id.get(),
            slot: *slot,
            reason: map_cancel_reason(*reason),
        }),
        game_sim::SimulationEvent::ChannelTick {
            player_id,
            slot,
            tick_index,
            behavior,
        } => Some(CombatLogEvent::ChannelTick {
            player_id: player_id.get(),
            slot: *slot,
            tick_index: *tick_index,
            behavior: (*behavior).to_string(),
        }),
        game_sim::SimulationEvent::ImpactHit {
            source,
            slot,
            target_kind,
            target_id,
        } => Some(CombatLogEvent::ImpactHit {
            source_player_id: source.get(),
            slot: *slot,
            target_kind: map_target_kind(*target_kind),
            target_id: *target_id,
        }),
        game_sim::SimulationEvent::ImpactMiss {
            source,
            slot,
            reason,
        } => Some(CombatLogEvent::ImpactMiss {
            source_player_id: source.get(),
            slot: *slot,
            reason: map_miss_reason(*reason),
        }),
        game_sim::SimulationEvent::DispelCast {
            source,
            slot,
            scope,
            max_statuses,
        } => Some(CombatLogEvent::DispelCast {
            source_player_id: source.get(),
            slot: *slot,
            scope: dispel_scope_label(*scope).to_string(),
            max_statuses: *max_statuses,
        }),
        game_sim::SimulationEvent::DispelResult {
            source,
            slot,
            target,
            removed_statuses,
            triggered_payload_count,
        } => Some(CombatLogEvent::DispelResult {
            source_player_id: source.get(),
            slot: *slot,
            target_player_id: target.get(),
            removed_statuses: removed_statuses
                .iter()
                .copied()
                .map(map_removed_status)
                .collect(),
            triggered_payload_count: *triggered_payload_count,
        }),
        game_sim::SimulationEvent::TriggerResolved {
            source,
            slot,
            status_kind,
            trigger,
            target_kind,
            target_id,
            payload_kind,
            amount,
        } => Some(CombatLogEvent::TriggerResolved {
            source_player_id: source.get(),
            target_kind: map_target_kind(*target_kind),
            target_id: *target_id,
            slot: *slot,
            status_kind: status_kind_label(*status_kind).to_string(),
            trigger: map_trigger_reason(*trigger),
            payload_kind: payload_kind_label(*payload_kind).to_string(),
            amount: *amount,
        }),
        game_sim::SimulationEvent::Defeat { attacker, target } => Some(CombatLogEvent::Defeat {
            source_player_id: attacker.map(|value| value.get()),
            target_player_id: target.get(),
        }),
        game_sim::SimulationEvent::DeployableSpawned { .. } => None,
        game_sim::SimulationEvent::DeployableDamaged {
            attacker,
            deployable_id,
            amount,
            remaining_hit_points,
            destroyed,
        } => Some(CombatLogEvent::DamageApplied {
            source_player_id: attacker.get(),
            target_kind: CombatLogTargetKind::Deployable,
            target_id: *deployable_id,
            slot: 0,
            amount: *amount,
            remaining_hit_points: *remaining_hit_points,
            defeated: *destroyed,
            status_kind: None,
            trigger: None,
        }),
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

fn build_training_world(
    participant: &TeamAssignment,
    loadout: &[Option<SkillChoice>; 5],
    content: &GameContent,
) -> SimulationWorld {
    let map = content
        .training_map()
        .unwrap_or_else(|| panic!("training world requires an authored training map"));
    match SimulationWorld::new(
        vec![build_player_seed(participant.clone(), loadout, content)],
        map,
    ) {
        Ok(world) => world,
        Err(error) => panic!("valid training loadout should build a simulation world: {error}"),
    }
}

fn build_player_seed(
    assignment: TeamAssignment,
    loadout: &[Option<SkillChoice>; 5],
    content: &GameContent,
) -> SimPlayerSeed {
    let primary_tree = loadout[0]
        .as_ref()
        .map(|choice| choice.tree.clone())
        .unwrap_or(game_domain::SkillTree::Warrior);
    let melee = if let Some(melee) = content.skills().melee_for(&primary_tree) {
        melee.clone()
    } else if let Some(melee) = content.skills().melee_for(&game_domain::SkillTree::Warrior) {
        melee.clone()
    } else {
        panic!("validated content should always define warrior melee");
    };
    SimPlayerSeed {
        assignment,
        hit_points: DEFAULT_HIT_POINTS,
        melee,
        skills: std::array::from_fn(|index| {
            loadout[index]
                .as_ref()
                .and_then(|choice| content.skills().resolve(choice).cloned())
        }),
    }
}

#[cfg(test)]
mod tests;
