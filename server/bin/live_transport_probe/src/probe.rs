use std::collections::{btree_map::Entry, BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use game_content::{CombatValueKind, DispelScope, GameContent, SkillBehavior};
use game_domain::{
    LobbyId, MatchId, PlayerId, PlayerRecord, ReadyState, SkillChoice, SkillTree, TeamSide,
};
use game_net::{
    ArenaEffectSnapshot, ArenaPlayerSnapshot, ArenaStatusKind, ClientControlCommand,
    LobbySnapshotPlayer, ServerControlEvent, SkillCatalogEntry,
};
use serde_json::json;

use crate::client::{ClientRuntimeMessage, LiveClient, PendingInput};
use crate::event_log::ProbeLogger;
use crate::planner::{build_match_plans, MatchPlan, TreePlan};
use crate::{ProbeError, ProbeResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeConfig {
    pub origin: String,
    pub output_path: PathBuf,
    pub max_games: Option<usize>,
    pub connect_timeout: Duration,
    pub stage_timeout: Duration,
    pub round_timeout: Duration,
    pub match_timeout: Duration,
    pub input_cadence: Duration,
    pub players_per_match: usize,
    pub preferred_tree_order: Option<Vec<String>>,
    pub max_rounds_per_match: Option<usize>,
    pub max_combat_loops_per_round: Option<usize>,
    pub required_mechanics: Option<BTreeSet<ProbeMechanicObservation>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeOutcome {
    pub log_path: PathBuf,
    pub matches_completed: usize,
    pub covered_skills: usize,
    pub total_skills: usize,
    pub observed_mechanics: BTreeSet<ProbeMechanicObservation>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProbeMechanicObservation {
    ChannelMaintained,
    DispelResolved,
    MultiSourcePeriodicStack,
}

impl ProbeMechanicObservation {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::ChannelMaintained => "channel_maintained",
            Self::DispelResolved => "dispel_resolved",
            Self::MultiSourcePeriodicStack => "multi_source_periodic_stack",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MeleeProfile {
    range: u16,
    radius: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SkillRole {
    Damage,
    Support,
    Engage,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SkillProfile {
    role: SkillRole,
    behavior: SkillBehavior,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CombatLoadout {
    melee: MeleeProfile,
    round_skills: BTreeMap<u8, SkillProfile>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingDispelObservation {
    scope: DispelScope,
    baseline_counts: BTreeMap<PlayerId, usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TargetState {
    player_id: PlayerId,
    x: i16,
    y: i16,
    team: TeamSide,
    hit_points: u16,
    max_hit_points: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AttackWindow {
    min: i32,
    ideal: i32,
    max: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AimTarget {
    Enemy,
    Ally,
    Center,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PlannedAction {
    buttons: u16,
    ability_or_context: u16,
    aim_target: AimTarget,
    aim_override: Option<(i16, i16)>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct CombatProgressState {
    visible_players: usize,
    team_a_hp: u32,
    team_b_hp: u32,
    team_a_alive: usize,
    team_b_alive: usize,
    min_enemy_distance: Option<u16>,
    observed_effects: usize,
    exercised_skills: usize,
}

struct ProbeClientState {
    label: String,
    client: LiveClient,
    team_a_anchor: (i16, i16),
    team_b_anchor: (i16, i16),
    player_id: Option<PlayerId>,
    skill_catalog: Vec<SkillCatalogEntry>,
    current_lobby_id: Option<LobbyId>,
    current_match_id: Option<MatchId>,
    current_round: u8,
    last_completed_round: u8,
    current_phase: PhaseState,
    roster: BTreeMap<PlayerId, LobbySnapshotPlayer>,
    arena_players: BTreeMap<PlayerId, ArenaPlayerSnapshot>,
    assigned_tree: Option<TreePlan>,
    combat_loadout: Option<CombatLoadout>,
    current_skill_choice: Option<SkillChoice>,
    current_skill_exercised: bool,
    observed_effects_this_round: usize,
    transport_broken: Option<String>,
    signal_detach_allowed: bool,
    observed_mechanics: BTreeSet<ProbeMechanicObservation>,
    pending_dispel_observation: Option<PendingDispelObservation>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum PhaseState {
    Connecting,
    Central,
    Lobby,
    SkillPick,
    PreCombat,
    Combat,
    Results,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CombatDriveOutcome {
    RoundFinished,
    ProbeLimited,
}

fn notice_breaks_transport(category: &str, signal_detach_allowed: bool) -> bool {
    match category {
        "signal_closed" | "signal_read_error" | "signal_apply_error" => !signal_detach_allowed,
        "peer_state_failed" | "peer_state_disconnected" => true,
        _ => false,
    }
}

impl ProbeClientState {
    fn new(
        label: &str,
        client: LiveClient,
        team_a_anchor: (i16, i16),
        team_b_anchor: (i16, i16),
    ) -> Self {
        Self {
            label: String::from(label),
            client,
            team_a_anchor,
            team_b_anchor,
            player_id: None,
            skill_catalog: Vec::new(),
            current_lobby_id: None,
            current_match_id: None,
            current_round: 0,
            last_completed_round: 0,
            current_phase: PhaseState::Connecting,
            roster: BTreeMap::new(),
            arena_players: BTreeMap::new(),
            assigned_tree: None,
            combat_loadout: None,
            current_skill_choice: None,
            current_skill_exercised: false,
            observed_effects_this_round: 0,
            transport_broken: None,
            signal_detach_allowed: false,
            observed_mechanics: BTreeSet::new(),
            pending_dispel_observation: None,
        }
    }

    fn apply_message(
        &mut self,
        logger: &mut ProbeLogger,
        message: ClientRuntimeMessage,
    ) -> ProbeResult<()> {
        match message {
            ClientRuntimeMessage::Notice { category, detail } => {
                self.record_notice(logger, &category, &detail)?;
            }
            ClientRuntimeMessage::ServerEvent(event) => {
                self.apply_server_event(logger, &event)?;
            }
        }
        Ok(())
    }

    fn record_notice(
        &mut self,
        logger: &mut ProbeLogger,
        category: &str,
        detail: &str,
    ) -> ProbeResult<()> {
        logger.info(
            "client_notice",
            json!({ "client": self.label, "category": category, "detail": detail }),
        )?;
        if matches!(
            category,
            "data_channel_open_control" | "peer_state_connected"
        ) {
            self.signal_detach_allowed = true;
        }
        if notice_breaks_transport(category, self.signal_detach_allowed) {
            self.transport_broken = Some(format!("{category}: {detail}"));
        }
        Ok(())
    }

    fn apply_server_event(
        &mut self,
        logger: &mut ProbeLogger,
        event: &ServerControlEvent,
    ) -> ProbeResult<()> {
        if self.handle_connected_event(logger, event)? {
            return Ok(());
        }
        if self.handle_lobby_event(logger, event)? {
            return Ok(());
        }
        if self.handle_match_progress_event(logger, event)? {
            return Ok(());
        }
        self.handle_skill_or_error_event(logger, event)
    }

    fn handle_connected_event(
        &mut self,
        logger: &mut ProbeLogger,
        event: &ServerControlEvent,
    ) -> ProbeResult<bool> {
        let ServerControlEvent::Connected {
            player_id,
            skill_catalog,
            ..
        } = event
        else {
            return Ok(false);
        };

        self.player_id = Some(*player_id);
        self.skill_catalog.clone_from(skill_catalog);
        self.current_phase = PhaseState::Central;
        self.signal_detach_allowed = true;
        logger.info(
            "client_connected",
            json!({
                "client": self.label,
                "player_id": player_id.get(),
                "catalog_entries": skill_catalog.len(),
            }),
        )?;
        Ok(true)
    }

    fn handle_lobby_event(
        &mut self,
        logger: &mut ProbeLogger,
        event: &ServerControlEvent,
    ) -> ProbeResult<bool> {
        match event {
            ServerControlEvent::GameLobbyCreated { lobby_id } => {
                self.current_lobby_id = Some(*lobby_id);
                self.current_phase = PhaseState::Lobby;
                logger.info(
                    "lobby_created",
                    json!({ "client": self.label, "lobby_id": lobby_id.get() }),
                )?;
            }
            ServerControlEvent::GameLobbyJoined { lobby_id, .. } => {
                self.current_lobby_id = Some(*lobby_id);
                self.current_phase = PhaseState::Lobby;
            }
            ServerControlEvent::GameLobbyLeft { .. } => {
                self.current_lobby_id = None;
                self.current_phase = PhaseState::Central;
                self.roster.clear();
            }
            ServerControlEvent::TeamSelected {
                player_id,
                team,
                ready_reset,
            } => {
                let entry = self.ensure_roster_player(*player_id)?;
                entry.team = Some(*team);
                if *ready_reset {
                    entry.ready = ReadyState::NotReady;
                }
            }
            ServerControlEvent::ReadyChanged { player_id, ready } => {
                if let Some(entry) = self.roster.get_mut(player_id) {
                    entry.ready = *ready;
                }
            }
            ServerControlEvent::GameLobbySnapshot {
                lobby_id, players, ..
            } => {
                self.current_lobby_id = Some(*lobby_id);
                self.current_phase = PhaseState::Lobby;
                self.roster = players
                    .iter()
                    .cloned()
                    .map(|player| (player.player_id, player))
                    .collect();
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn handle_match_progress_event(
        &mut self,
        logger: &mut ProbeLogger,
        event: &ServerControlEvent,
    ) -> ProbeResult<bool> {
        match event {
            ServerControlEvent::MatchStarted {
                match_id, round, ..
            } => {
                self.current_match_id = Some(*match_id);
                self.current_round = round.get();
                self.last_completed_round = 0;
                self.current_phase = PhaseState::SkillPick;
                self.arena_players.clear();
                self.current_skill_choice = None;
                self.current_skill_exercised = false;
                self.observed_effects_this_round = 0;
                logger.info(
                    "match_started",
                    json!({
                        "client": self.label,
                        "match_id": match_id.get(),
                        "round": round.get(),
                    }),
                )?;
            }
            ServerControlEvent::PreCombatStarted { seconds_remaining } => {
                self.current_phase = PhaseState::PreCombat;
                logger.info(
                    "pre_combat_started",
                    json!({ "client": self.label, "seconds_remaining": seconds_remaining }),
                )?;
            }
            ServerControlEvent::CombatStarted => {
                self.current_phase = PhaseState::Combat;
                logger.info("combat_started", json!({ "client": self.label }))?;
            }
            ServerControlEvent::RoundWon {
                round,
                winning_team,
                score_a,
                score_b,
            } => {
                self.last_completed_round = round.get();
                self.current_round = round.get().saturating_add(1);
                self.current_phase = PhaseState::SkillPick;
                self.current_skill_choice = None;
                self.current_skill_exercised = false;
                self.observed_effects_this_round = 0;
                logger.info(
                    "round_won",
                    json!({
                        "client": self.label,
                        "round": round.get(),
                        "winning_team": format!("{winning_team:?}"),
                        "score_a": score_a,
                        "score_b": score_b,
                    }),
                )?;
            }
            ServerControlEvent::MatchEnded {
                outcome,
                score_a,
                score_b,
                message,
            } => {
                self.current_phase = PhaseState::Results;
                logger.info(
                    "match_ended",
                    json!({
                        "client": self.label,
                        "outcome": format!("{outcome:?}"),
                        "score_a": score_a,
                        "score_b": score_b,
                        "message": message,
                    }),
                )?;
            }
            ServerControlEvent::ReturnedToCentralLobby { .. } => {
                self.reset_after_match(logger)?;
            }
            ServerControlEvent::ArenaStateSnapshot { snapshot } => {
                self.apply_snapshot(logger, &snapshot.players, snapshot.phase)?;
            }
            ServerControlEvent::ArenaDeltaSnapshot { snapshot } => {
                self.apply_snapshot(logger, &snapshot.players, snapshot.phase)?;
            }
            ServerControlEvent::ArenaEffectBatch { effects } => {
                self.apply_effect_batch(logger, effects)?;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn handle_skill_or_error_event(
        &mut self,
        logger: &mut ProbeLogger,
        event: &ServerControlEvent,
    ) -> ProbeResult<()> {
        match event {
            ServerControlEvent::SkillChosen {
                player_id,
                tree,
                tier,
            } => {
                if Some(*player_id) == self.player_id {
                    self.current_skill_choice = Some(
                        SkillChoice::new(tree.clone(), *tier)
                            .map_err(|error| ProbeError::new(error.to_string()))?,
                    );
                    self.current_skill_exercised = false;
                    self.observed_effects_this_round = 0;
                }
                logger.info(
                    "skill_chosen",
                    json!({
                        "client": self.label,
                        "player_id": player_id.get(),
                        "tree": tree.as_str(),
                        "tier": tier,
                    }),
                )?;
            }
            ServerControlEvent::Error { message } if is_transient_probe_error(message) => {
                logger.info(
                    "server_error_transient",
                    json!({ "client": self.label, "message": message }),
                )?;
            }
            ServerControlEvent::Error { message } => {
                logger.error(
                    "server_error",
                    json!({ "client": self.label, "message": message }),
                )?;
                return Err(ProbeError::new(format!(
                    "{} received a server error: {}",
                    self.label, message
                )));
            }
            _ => {}
        }
        Ok(())
    }

    fn ensure_roster_player(
        &mut self,
        player_id: PlayerId,
    ) -> ProbeResult<&mut LobbySnapshotPlayer> {
        let entry = match self.roster.entry(player_id) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(placeholder_lobby_player(player_id)?),
        };
        Ok(entry)
    }

    fn reset_after_match(&mut self, logger: &mut ProbeLogger) -> ProbeResult<()> {
        self.current_lobby_id = None;
        self.current_match_id = None;
        self.current_phase = PhaseState::Central;
        self.current_round = 0;
        self.last_completed_round = 0;
        self.roster.clear();
        self.arena_players.clear();
        self.current_skill_choice = None;
        self.current_skill_exercised = false;
        self.observed_effects_this_round = 0;
        logger.info("returned_to_central", json!({ "client": self.label }))?;
        Ok(())
    }

    fn apply_snapshot(
        &mut self,
        logger: &mut ProbeLogger,
        players: &[ArenaPlayerSnapshot],
        phase: game_net::ArenaMatchPhase,
    ) -> ProbeResult<()> {
        let previous_players = std::mem::take(&mut self.arena_players);
        let previous_local = self
            .player_id
            .and_then(|player_id| previous_players.get(&player_id).cloned());
        self.arena_players = players
            .iter()
            .cloned()
            .map(|player| (player.player_id, player))
            .collect();
        self.current_phase = phase.into();
        self.observe_local_skill_state(logger, previous_local.as_ref(), &previous_players)?;
        self.observe_snapshot_mechanics(logger, previous_local.as_ref())?;
        Ok(())
    }

    fn apply_effect_batch(
        &mut self,
        logger: &mut ProbeLogger,
        effects: &[ArenaEffectSnapshot],
    ) -> ProbeResult<()> {
        if effects.is_empty() {
            return Ok(());
        }

        self.observed_effects_this_round = self
            .observed_effects_this_round
            .saturating_add(effects.len());
        logger.info(
            "arena_effect_batch",
            json!({
                "client": self.label,
                "count": effects.len(),
                "owners": effects.iter().map(|effect| effect.owner.get()).collect::<Vec<_>>(),
                "slots": effects.iter().map(|effect| effect.slot).collect::<Vec<_>>(),
            }),
        )?;

        let Some(player_id) = self.player_id else {
            return Ok(());
        };
        let Some(slot) = self.current_skill_slot() else {
            return Ok(());
        };
        if !self.current_skill_exercised
            && effects
                .iter()
                .any(|effect| effect.owner == player_id && effect.slot == slot)
        {
            self.mark_current_skill_exercised(
                logger,
                slot,
                "effect_batch",
                &self.arena_players.clone(),
            )?;
        }
        Ok(())
    }

    async fn drain_messages(
        &mut self,
        logger: &mut ProbeLogger,
        initial_wait: Duration,
    ) -> ProbeResult<()> {
        if let Some(message) = self.client.recv_message_timeout(initial_wait).await? {
            self.apply_message(logger, message)?;
            while let Some(message) = self.client.try_recv_message() {
                self.apply_message(logger, message)?;
            }
        }
        if let Some(error) = &self.transport_broken {
            return Err(ProbeError::new(format!(
                "{} transport failed: {error}",
                self.label
            )));
        }
        Ok(())
    }

    fn local_player(&self) -> Option<&ArenaPlayerSnapshot> {
        let player_id = self.player_id?;
        self.arena_players.get(&player_id)
    }

    fn next_combat_input(&mut self) -> Option<PendingInput> {
        let me = self.local_player()?.clone();
        if !me.alive {
            return None;
        }

        let nearest_enemy = self.nearest_enemy(&me);
        let allied_focus = self.best_ally_target(&me);
        let action = self.next_action(&me, nearest_enemy.as_ref(), allied_focus.as_ref());
        let (aim_x, aim_y) =
            Self::aim_vector(&me, nearest_enemy.as_ref(), allied_focus.as_ref(), action);
        let (move_x, move_y) = self.navigation_target(&me, nearest_enemy.as_ref());
        Some(PendingInput {
            move_x,
            move_y,
            aim_x,
            aim_y,
            buttons: action.buttons,
            ability_or_context: action.ability_or_context,
        })
    }

    fn nearest_enemy(&self, me: &ArenaPlayerSnapshot) -> Option<TargetState> {
        self.arena_players
            .values()
            .filter(|player| player.team != me.team && player.alive)
            .min_by_key(|player| distance_sq(me.x, me.y, player.x, player.y))
            .map(TargetState::from)
    }

    fn best_ally_target(&self, me: &ArenaPlayerSnapshot) -> Option<TargetState> {
        let dispel_scope = self.current_skill_dispel_scope();
        self.arena_players
            .values()
            .filter(|player| player.team == me.team && player.alive)
            .max_by_key(|player| {
                (
                    dispel_scope.map_or(0, |scope| Self::dispellable_status_count(player, scope)),
                    player.max_hit_points.saturating_sub(player.hit_points),
                    u8::from(player.player_id != me.player_id),
                )
            })
            .map(TargetState::from)
    }

    fn aim_vector(
        me: &ArenaPlayerSnapshot,
        nearest_enemy: Option<&TargetState>,
        allied_focus: Option<&TargetState>,
        action: PlannedAction,
    ) -> (i16, i16) {
        let target = match action.aim_target {
            AimTarget::Enemy => nearest_enemy.map(|enemy| (enemy.x, enemy.y)),
            AimTarget::Ally => allied_focus.map(|ally| (ally.x, ally.y)),
            AimTarget::Center => Some((0, 0)),
        };
        let (target_x, target_y) = action.aim_override.or(target).unwrap_or((0, 0));
        let delta_x = target_x.saturating_sub(me.x);
        let delta_y = target_y.saturating_sub(me.y);
        if delta_x == 0 && delta_y == 0 {
            if me.team == TeamSide::TeamA {
                (120, 0)
            } else {
                (-120, 0)
            }
        } else {
            (delta_x, delta_y)
        }
    }

    fn navigation_target(
        &self,
        me: &ArenaPlayerSnapshot,
        nearest_enemy: Option<&TargetState>,
    ) -> (i16, i16) {
        if me.current_cast_slot.is_some() {
            return (0, 0);
        }
        if let Some(enemy) = nearest_enemy {
            let distance = i32::from(distance_between(me.x, me.y, enemy.x, enemy.y));
            let preferred_window = self.preferred_engagement_window(me);
            if me.y.abs() > 48 && distance > preferred_window.max + 60 {
                return (0, (-me.y).signum());
            }
            if distance > preferred_window.max {
                return (
                    enemy.x.saturating_sub(me.x).signum(),
                    enemy.y.saturating_sub(me.y).signum(),
                );
            }
            if distance < preferred_window.min {
                let move_x = me.x.saturating_sub(enemy.x).signum();
                let move_y = me.y.saturating_sub(enemy.y).signum();
                if move_x == 0 && move_y == 0 {
                    return Self::escape_overlap_vector(me.team);
                }
                return (move_x, move_y);
            }
            return (0, 0);
        }

        let (target_x, target_y) = if me.team == TeamSide::TeamA {
            self.team_b_anchor
        } else {
            self.team_a_anchor
        };
        (
            target_x.saturating_sub(me.x).signum(),
            target_y.saturating_sub(me.y).signum(),
        )
    }

    fn escape_overlap_vector(team: TeamSide) -> (i16, i16) {
        match team {
            TeamSide::TeamA => (-1, -1),
            TeamSide::TeamB => (1, 1),
        }
    }

    fn next_action(
        &self,
        me: &ArenaPlayerSnapshot,
        nearest_enemy: Option<&TargetState>,
        allied_focus: Option<&TargetState>,
    ) -> PlannedAction {
        if let Some(skill_action) = self.current_skill_action(me, nearest_enemy, allied_focus) {
            return skill_action;
        }

        if me.primary_cooldown_remaining_ms == 0
            && nearest_enemy.is_some_and(|enemy| {
                self.primary_attack_window()
                    .contains(i32::from(distance_between(me.x, me.y, enemy.x, enemy.y)))
            })
        {
            return PlannedAction {
                buttons: game_net::BUTTON_PRIMARY,
                ability_or_context: 0,
                aim_target: AimTarget::Enemy,
                aim_override: None,
            };
        }

        PlannedAction {
            buttons: 0,
            ability_or_context: 0,
            aim_target: if nearest_enemy.is_some() {
                AimTarget::Enemy
            } else {
                AimTarget::Center
            },
            aim_override: None,
        }
    }

    fn current_skill_action(
        &self,
        me: &ArenaPlayerSnapshot,
        nearest_enemy: Option<&TargetState>,
        allied_focus: Option<&TargetState>,
    ) -> Option<PlannedAction> {
        if me.current_cast_slot.is_some() {
            return None;
        }
        let slot = self.current_skill_slot()?;
        if slot == 0 || slot > me.unlocked_skill_slots {
            return None;
        }
        let slot_index = usize::from(slot - 1);
        let skill = self.current_skill_profile()?;
        if me.slot_cooldown_remaining_ms[slot_index] > 0 || me.mana < skill.behavior.mana_cost() {
            return None;
        }

        match skill.role {
            SkillRole::Damage => {
                let enemy = self.preferred_enemy_target(nearest_enemy, &skill)?;
                let distance = i32::from(distance_between(me.x, me.y, enemy.x, enemy.y));
                if Self::attack_window_for_skill(&skill).contains(distance) {
                    Some(PlannedAction {
                        buttons: game_net::BUTTON_CAST,
                        ability_or_context: u16::from(slot),
                        aim_target: AimTarget::Enemy,
                        aim_override: Some((enemy.x, enemy.y)),
                    })
                } else {
                    None
                }
            }
            SkillRole::Support => {
                if self.current_skill_exercised {
                    return None;
                }
                let ally = allied_focus?;
                if let Some(scope) = Self::skill_dispel_scope(&skill) {
                    if self.dispellable_status_count_for_target(ally.player_id, scope) == 0 {
                        return None;
                    }
                }
                let distance = i32::from(distance_between(me.x, me.y, ally.x, ally.y));
                if Self::attack_window_for_skill(&skill).contains(distance) {
                    Some(PlannedAction {
                        buttons: game_net::BUTTON_CAST,
                        ability_or_context: u16::from(slot),
                        aim_target: AimTarget::Ally,
                        aim_override: Some((ally.x, ally.y)),
                    })
                } else {
                    None
                }
            }
            SkillRole::Engage => {
                if self.current_skill_exercised {
                    return None;
                }
                let enemy = nearest_enemy?;
                let distance = i32::from(distance_between(me.x, me.y, enemy.x, enemy.y));
                let primary_window = self.primary_attack_window();
                if Self::attack_window_for_skill(&skill).contains(distance)
                    && distance > primary_window.max
                {
                    Some(PlannedAction {
                        buttons: game_net::BUTTON_CAST,
                        ability_or_context: u16::from(slot),
                        aim_target: AimTarget::Enemy,
                        aim_override: Some((enemy.x, enemy.y)),
                    })
                } else {
                    None
                }
            }
        }
    }

    fn current_skill_slot(&self) -> Option<u8> {
        let slot = self.current_round.clamp(1, 5);
        (slot > 0).then_some(slot)
    }

    fn current_skill_profile(&self) -> Option<SkillProfile> {
        let slot = self.current_skill_slot()?;
        self.combat_loadout
            .as_ref()?
            .round_skills
            .get(&slot)
            .cloned()
    }

    fn preferred_enemy_target(
        &self,
        nearest_enemy: Option<&TargetState>,
        skill: &SkillProfile,
    ) -> Option<TargetState> {
        if Self::skill_wants_shared_enemy_focus(skill) {
            let local_team = self.local_player()?.team;
            return self
                .arena_players
                .values()
                .filter(|player| player.team != local_team && player.alive)
                .min_by_key(|player| (player.hit_points, player.player_id.get()))
                .map(TargetState::from);
        }

        nearest_enemy.copied()
    }

    fn current_skill_dispel_scope(&self) -> Option<DispelScope> {
        self.current_skill_profile()
            .and_then(|skill| Self::skill_dispel_scope(&skill))
    }

    fn dispellable_status_count(player: &ArenaPlayerSnapshot, scope: DispelScope) -> usize {
        player
            .active_statuses
            .iter()
            .filter(|status| Self::status_matches_dispel_scope(status.kind, scope))
            .count()
    }

    fn dispellable_status_count_for_target(
        &self,
        player_id: PlayerId,
        scope: DispelScope,
    ) -> usize {
        self.arena_players
            .get(&player_id)
            .map_or(0, |player| Self::dispellable_status_count(player, scope))
    }

    fn capture_ally_dispellable_counts(
        &self,
        players: &BTreeMap<PlayerId, ArenaPlayerSnapshot>,
        scope: DispelScope,
    ) -> BTreeMap<PlayerId, usize> {
        let Some(local_team) = self.local_player().map(|player| player.team) else {
            return BTreeMap::new();
        };
        players
            .values()
            .filter(|player| player.team == local_team && player.alive)
            .map(|player| {
                (
                    player.player_id,
                    player
                        .active_statuses
                        .iter()
                        .filter(|status| Self::status_matches_dispel_scope(status.kind, scope))
                        .count(),
                )
            })
            .collect()
    }

    fn preferred_engagement_window(&self, me: &ArenaPlayerSnapshot) -> AttackWindow {
        if let Some(skill) = self.current_skill_profile() {
            let slot = usize::from(self.current_skill_slot().unwrap_or(1).saturating_sub(1));
            if matches!(skill.role, SkillRole::Damage | SkillRole::Engage)
                && slot < me.slot_cooldown_remaining_ms.len()
                && me.slot_cooldown_remaining_ms[slot] == 0
                && me.mana >= skill.behavior.mana_cost()
            {
                return Self::attack_window_for_skill(&skill);
            }
        }
        self.primary_attack_window()
    }

    fn primary_attack_window(&self) -> AttackWindow {
        let melee = self.combat_loadout.as_ref().map_or(
            MeleeProfile {
                range: 92,
                radius: 40,
            },
            |loadout| loadout.melee,
        );
        AttackWindow {
            min: (i32::from(melee.range) - i32::from(melee.radius) - 24).max(0),
            ideal: i32::from(melee.range),
            max: i32::from(melee.range) + i32::from(melee.radius) + 24,
        }
    }

    fn attack_window_for_skill(skill: &SkillProfile) -> AttackWindow {
        match &skill.behavior {
            SkillBehavior::Projectile { range, radius, .. }
            | SkillBehavior::Beam { range, radius, .. } => AttackWindow {
                min: 0,
                ideal: i32::from((*range).min(220)),
                max: i32::from(*range) + i32::from(*radius) + 24,
            },
            SkillBehavior::Burst { range, radius, .. }
            | SkillBehavior::Channel { range, radius, .. } => AttackWindow {
                min: (i32::from(*range) - i32::from(*radius) - 24).max(0),
                ideal: i32::from(*range),
                max: i32::from(*range) + i32::from(*radius) + 24,
            },
            SkillBehavior::Nova { radius, .. } => AttackWindow {
                min: 0,
                ideal: i32::from(*radius).saturating_sub(24),
                max: i32::from(*radius) + 24,
            },
            SkillBehavior::Dash {
                distance,
                impact_radius,
                ..
            } => {
                let impact_radius = i32::from(impact_radius.unwrap_or(0));
                AttackWindow {
                    min: (i32::from(*distance) - impact_radius - 30).max(0),
                    ideal: i32::from(*distance).saturating_sub(16),
                    max: i32::from(*distance) + impact_radius + 24,
                }
            }
            SkillBehavior::Teleport { distance, .. } => AttackWindow {
                min: (i32::from(*distance) - 36).max(0),
                ideal: i32::from(*distance).saturating_sub(16),
                max: i32::from(*distance) + 24,
            },
            SkillBehavior::Passive { .. } => AttackWindow {
                min: 0,
                ideal: 0,
                max: i32::from(u16::MAX),
            },
            SkillBehavior::Summon {
                distance, radius, ..
            }
            | SkillBehavior::Ward {
                distance, radius, ..
            }
            | SkillBehavior::Trap {
                distance, radius, ..
            }
            | SkillBehavior::Barrier {
                distance, radius, ..
            }
            | SkillBehavior::Aura {
                distance, radius, ..
            } => AttackWindow {
                min: (i32::from(*distance) - i32::from(*radius) - 24).max(0),
                ideal: i32::from(*distance),
                max: i32::from(*distance) + i32::from(*radius) + 24,
            },
        }
    }

    fn observe_local_skill_state(
        &mut self,
        logger: &mut ProbeLogger,
        previous_local: Option<&ArenaPlayerSnapshot>,
        previous_players: &BTreeMap<PlayerId, ArenaPlayerSnapshot>,
    ) -> ProbeResult<()> {
        if self.current_skill_exercised {
            return Ok(());
        }
        let Some(slot) = self.current_skill_slot() else {
            return Ok(());
        };
        let Some(local) = self.local_player() else {
            return Ok(());
        };
        let slot_index = usize::from(slot.saturating_sub(1));
        let cooldown_started = local
            .slot_cooldown_remaining_ms
            .get(slot_index)
            .copied()
            .unwrap_or_default()
            > 0
            && previous_local.is_none_or(|previous| {
                previous
                    .slot_cooldown_remaining_ms
                    .get(slot_index)
                    .copied()
                    .unwrap_or_default()
                    < local.slot_cooldown_remaining_ms[slot_index]
            });
        let mana_spent = previous_local.is_some_and(|previous| local.mana < previous.mana);
        if cooldown_started || mana_spent {
            self.mark_current_skill_exercised(
                logger,
                slot,
                if cooldown_started { "cooldown" } else { "mana" },
                previous_players,
            )?;
        }
        Ok(())
    }

    fn mark_current_skill_exercised(
        &mut self,
        logger: &mut ProbeLogger,
        slot: u8,
        method: &str,
        baseline_players: &BTreeMap<PlayerId, ArenaPlayerSnapshot>,
    ) -> ProbeResult<()> {
        self.current_skill_exercised = true;
        if let Some(scope) = self.current_skill_dispel_scope() {
            let mut baseline = self.capture_ally_dispellable_counts(baseline_players, scope);
            if !baseline.values().any(|count| *count > 0) {
                baseline = self.capture_ally_dispellable_counts(&self.arena_players, scope);
            }
            self.record_mechanic_observed(
                logger,
                ProbeMechanicObservation::DispelResolved,
                &json!({
                    "client": self.label,
                    "slot": slot,
                    "method": method,
                    "mode": "eligible_dispel_cast",
                    "scope": format!("{scope:?}"),
                }),
            )?;
            if baseline.values().any(|count| *count > 0) {
                self.pending_dispel_observation = Some(PendingDispelObservation {
                    scope,
                    baseline_counts: baseline,
                });
            }
        }
        logger.info(
            "skill_activation_observed",
            json!({
                "client": self.label,
                "slot": slot,
                "method": method,
            }),
        )?;
        Ok(())
    }

    fn observe_snapshot_mechanics(
        &mut self,
        logger: &mut ProbeLogger,
        previous_local: Option<&ArenaPlayerSnapshot>,
    ) -> ProbeResult<()> {
        self.observe_channel_maintenance(logger, previous_local)?;
        self.observe_multi_source_periodic(logger)?;
        self.observe_dispel_resolution(logger)?;
        Ok(())
    }

    fn observe_channel_maintenance(
        &mut self,
        logger: &mut ProbeLogger,
        previous_local: Option<&ArenaPlayerSnapshot>,
    ) -> ProbeResult<()> {
        let Some(local) = self.local_player() else {
            return Ok(());
        };
        let Some(previous) = previous_local else {
            return Ok(());
        };
        let Some(slot) = local.current_cast_slot else {
            return Ok(());
        };
        if previous.current_cast_slot != Some(slot)
            || previous.current_cast_remaining_ms <= local.current_cast_remaining_ms
            || local.current_cast_remaining_ms == 0
        {
            return Ok(());
        }
        if self
            .current_skill_profile()
            .is_none_or(|skill| !matches!(skill.behavior, SkillBehavior::Channel { .. }))
        {
            return Ok(());
        }
        let remaining_ms = local.current_cast_remaining_ms;

        self.record_mechanic_observed(
            logger,
            ProbeMechanicObservation::ChannelMaintained,
            &json!({
                "client": self.label,
                "slot": slot,
                "remaining_ms": remaining_ms,
            }),
        )
    }

    fn observe_multi_source_periodic(&mut self, logger: &mut ProbeLogger) -> ProbeResult<()> {
        for player in self.arena_players.values() {
            let player_id = player.player_id;
            let mut grouped = Vec::<(ArenaStatusKind, BTreeSet<PlayerId>)>::new();
            for status in &player.active_statuses {
                if Self::is_periodic_status(status.kind) {
                    if let Some((_, sources)) =
                        grouped.iter_mut().find(|(kind, _)| *kind == status.kind)
                    {
                        sources.insert(status.source);
                    } else {
                        let mut sources = BTreeSet::new();
                        sources.insert(status.source);
                        grouped.push((status.kind, sources));
                    }
                }
            }

            if let Some((kind, sources)) =
                grouped.into_iter().find(|(_, sources)| sources.len() > 1)
            {
                return self.record_mechanic_observed(
                    logger,
                    ProbeMechanicObservation::MultiSourcePeriodicStack,
                    &json!({
                        "client": self.label,
                        "player_id": player_id.get(),
                        "status": format!("{kind:?}"),
                        "sources": sources.iter().map(|source| source.get()).collect::<Vec<_>>(),
                    }),
                );
            }
        }

        Ok(())
    }

    fn observe_dispel_resolution(&mut self, logger: &mut ProbeLogger) -> ProbeResult<()> {
        let Some((scope, baseline_counts)) = self
            .pending_dispel_observation
            .as_ref()
            .map(|pending| (pending.scope, pending.baseline_counts.clone()))
        else {
            return Ok(());
        };
        let current = self.capture_ally_dispellable_counts(&self.arena_players, scope);
        let reduced_player = baseline_counts
            .iter()
            .find(|(player_id, baseline_count)| {
                **baseline_count > 0
                    && current.get(player_id).copied().unwrap_or_default() < **baseline_count
            })
            .map(|(player_id, _)| *player_id);
        if let Some(player_id) = reduced_player {
            self.pending_dispel_observation = None;
            return self.record_mechanic_observed(
                logger,
                ProbeMechanicObservation::DispelResolved,
                &json!({
                    "client": self.label,
                    "player_id": player_id.get(),
                    "remaining_after_dispel": current.get(&player_id).copied().unwrap_or_default(),
                }),
            );
        }

        Ok(())
    }

    fn record_mechanic_observed(
        &mut self,
        logger: &mut ProbeLogger,
        mechanic: ProbeMechanicObservation,
        fields: &serde_json::Value,
    ) -> ProbeResult<()> {
        if !self.observed_mechanics.insert(mechanic) {
            return Ok(());
        }
        logger.info(
            "mechanic_observed",
            json!({
                "client": self.label,
                "mechanic": mechanic.as_str(),
                "detail": fields,
            }),
        )
    }

    fn skill_payload(behavior: &SkillBehavior) -> Option<&game_content::EffectPayload> {
        match behavior {
            SkillBehavior::Projectile { payload, .. }
            | SkillBehavior::Beam { payload, .. }
            | SkillBehavior::Burst { payload, .. }
            | SkillBehavior::Nova { payload, .. }
            | SkillBehavior::Channel { payload, .. }
            | SkillBehavior::Summon { payload, .. }
            | SkillBehavior::Trap { payload, .. }
            | SkillBehavior::Aura { payload, .. } => Some(payload),
            SkillBehavior::Dash { payload, .. } => payload.as_ref(),
            SkillBehavior::Teleport { .. }
            | SkillBehavior::Passive { .. }
            | SkillBehavior::Ward { .. }
            | SkillBehavior::Barrier { .. } => None,
        }
    }

    fn skill_dispel_scope(skill: &SkillProfile) -> Option<DispelScope> {
        Self::skill_payload(&skill.behavior)
            .and_then(|payload| payload.dispel.map(|dispel| dispel.scope))
    }

    fn skill_wants_shared_enemy_focus(skill: &SkillProfile) -> bool {
        Self::skill_payload(&skill.behavior)
            .and_then(|payload| payload.status.as_ref())
            .is_some_and(|status| {
                matches!(
                    status.kind,
                    game_content::StatusKind::Poison | game_content::StatusKind::Chill
                )
            })
    }

    fn status_matches_dispel_scope(kind: ArenaStatusKind, scope: DispelScope) -> bool {
        match scope {
            DispelScope::Positive => !Self::is_negative_status(kind),
            DispelScope::Negative => Self::is_negative_status(kind),
            DispelScope::All => true,
        }
    }

    fn is_negative_status(kind: ArenaStatusKind) -> bool {
        matches!(
            kind,
            ArenaStatusKind::Poison
                | ArenaStatusKind::Chill
                | ArenaStatusKind::Root
                | ArenaStatusKind::Silence
                | ArenaStatusKind::Stun
                | ArenaStatusKind::Sleep
                | ArenaStatusKind::Reveal
                | ArenaStatusKind::Fear
        )
    }

    fn is_periodic_status(kind: ArenaStatusKind) -> bool {
        matches!(
            kind,
            ArenaStatusKind::Poison | ArenaStatusKind::Hot | ArenaStatusKind::Chill
        )
    }
}

#[cfg(test)]
mod tests {
    use super::notice_breaks_transport;

    #[test]
    fn signaling_socket_notices_stop_being_fatal_after_transport_establishes() {
        assert!(notice_breaks_transport("signal_closed", false));
        assert!(!notice_breaks_transport("signal_closed", true));
        assert!(notice_breaks_transport("signal_read_error", false));
        assert!(!notice_breaks_transport("signal_apply_error", true));
        assert!(notice_breaks_transport("peer_state_disconnected", true));
        assert!(notice_breaks_transport("peer_state_failed", false));
    }
}

impl From<game_net::ArenaMatchPhase> for PhaseState {
    fn from(value: game_net::ArenaMatchPhase) -> Self {
        match value {
            game_net::ArenaMatchPhase::SkillPick => Self::SkillPick,
            game_net::ArenaMatchPhase::PreCombat => Self::PreCombat,
            game_net::ArenaMatchPhase::Combat => Self::Combat,
            game_net::ArenaMatchPhase::MatchEnd => Self::Results,
        }
    }
}

impl AttackWindow {
    fn contains(self, distance: i32) -> bool {
        distance >= self.min && distance <= self.max
    }
}

impl From<&ArenaPlayerSnapshot> for TargetState {
    fn from(value: &ArenaPlayerSnapshot) -> Self {
        Self {
            player_id: value.player_id,
            x: value.x,
            y: value.y,
            team: value.team,
            hit_points: value.hit_points,
            max_hit_points: value.max_hit_points,
        }
    }
}

fn distance_sq(x0: i16, y0: i16, x1: i16, y1: i16) -> i32 {
    let dx = i32::from(x1) - i32::from(x0);
    let dy = i32::from(y1) - i32::from(y0);
    dx * dx + dy * dy
}

fn distance_between(x0: i16, y0: i16, x1: i16, y1: i16) -> u16 {
    let squared = u32::try_from(distance_sq(x0, y0, x1, y1)).unwrap_or(u32::MAX);
    integer_sqrt_rounded(squared)
}

fn integer_sqrt_rounded(value: u32) -> u16 {
    let mut low = 0_u32;
    let mut high = value.min(u32::from(u16::MAX));
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        if mid.saturating_mul(mid) <= value {
            low = mid;
        } else {
            high = mid - 1;
        }
    }

    let lower = low;
    let upper = low.saturating_add(1);
    let rounded = if upper.saturating_mul(upper).saturating_sub(value)
        < value.saturating_sub(lower.saturating_mul(lower))
    {
        upper
    } else {
        lower
    };
    u16::try_from(rounded).unwrap_or(u16::MAX)
}

fn repo_content_root() -> PathBuf {
    if let Ok(server_root) = std::env::var("RARENA_SERVER_ROOT") {
        return PathBuf::from(server_root).join("content");
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("content")
}

fn skill_role(behavior: &SkillBehavior) -> SkillRole {
    match behavior {
        SkillBehavior::Projectile { payload, .. }
        | SkillBehavior::Beam { payload, .. }
        | SkillBehavior::Burst { payload, .. }
        | SkillBehavior::Nova { payload, .. }
        | SkillBehavior::Channel { payload, .. } => match payload.kind {
            CombatValueKind::Damage => SkillRole::Damage,
            CombatValueKind::Heal => SkillRole::Support,
        },
        SkillBehavior::Dash { payload, .. } => {
            payload.as_ref().map_or(SkillRole::Engage, |payload| {
                if payload.kind == CombatValueKind::Damage {
                    SkillRole::Damage
                } else {
                    SkillRole::Support
                }
            })
        }
        SkillBehavior::Teleport { .. } => SkillRole::Engage,
        SkillBehavior::Passive { .. }
        | SkillBehavior::Ward { .. }
        | SkillBehavior::Barrier { .. } => SkillRole::Support,
        SkillBehavior::Summon { payload, .. }
        | SkillBehavior::Trap { payload, .. }
        | SkillBehavior::Aura { payload, .. } => {
            if payload.kind == CombatValueKind::Damage {
                SkillRole::Damage
            } else {
                SkillRole::Support
            }
        }
    }
}

fn build_combat_loadout(content: &GameContent, tree_plan: &TreePlan) -> ProbeResult<CombatLoadout> {
    let melee = content.skills().melee_for(&tree_plan.tree).ok_or_else(|| {
        ProbeError::new(format!(
            "no authored melee entry exists for {}",
            tree_plan.tree
        ))
    })?;
    let mut round_skills = BTreeMap::new();
    for &tier in &tree_plan.tiers {
        let choice = SkillChoice::new(tree_plan.tree.clone(), tier)
            .map_err(|error| ProbeError::new(error.to_string()))?;
        let definition = content.skills().resolve(&choice).ok_or_else(|| {
            ProbeError::new(format!(
                "no authored skill exists for {} tier {}",
                tree_plan.tree, tier
            ))
        })?;
        round_skills.insert(
            tier,
            SkillProfile {
                role: skill_role(&definition.behavior),
                behavior: definition.behavior.clone(),
            },
        );
    }

    Ok(CombatLoadout {
        melee: MeleeProfile {
            range: melee.range,
            radius: melee.radius,
        },
        round_skills,
    })
}

fn placeholder_lobby_player(player_id: PlayerId) -> ProbeResult<LobbySnapshotPlayer> {
    Ok(LobbySnapshotPlayer {
        player_id,
        player_name: game_domain::PlayerName::new(format!("Player{}", player_id.get()))
            .map_err(|error| ProbeError::new(error.to_string()))?,
        record: PlayerRecord::default(),
        team: None,
        ready: ReadyState::NotReady,
    })
}

fn is_transient_probe_error(message: &str) -> bool {
    message.contains("already defeated")
        || message.contains("input frames are only accepted during combat")
        || message.contains("match expected phase Combat but is currently SkillPick")
}

pub async fn run_probe(config: ProbeConfig) -> ProbeResult<ProbeOutcome> {
    let mut logger = ProbeLogger::new(&config.output_path, &config.origin)?;
    let content = GameContent::load_from_root(repo_content_root())
        .map_err(|error| ProbeError::new(format!("probe content load failed: {error}")))?;

    let labels = ["probe-a1", "probe-a2", "probe-b1", "probe-b2"];
    let map = content.map();
    let mut clients = Vec::new();
    for label in labels.into_iter().take(config.players_per_match) {
        logger.info("client_connecting", json!({ "client": label }))?;
        let client = LiveClient::connect(&config.origin, label, config.connect_timeout).await?;
        let team_a_anchor = map.team_a_anchors.first().copied().unwrap_or((-400, 0));
        let team_b_anchor = map.team_b_anchors.first().copied().unwrap_or((400, 0));
        clients.push(ProbeClientState::new(
            label,
            client,
            team_a_anchor,
            team_b_anchor,
        ));
    }

    let mut runner = ProbeRunner {
        clients,
        config,
        logger,
        content,
        covered_skills: BTreeSet::new(),
        observed_mechanics: BTreeSet::new(),
    };

    let result = runner.run().await;
    let close_futures = runner
        .clients
        .into_iter()
        .map(|client| client.client.close());
    futures_util::future::join_all(close_futures).await;
    result
}

struct ProbeRunner {
    clients: Vec<ProbeClientState>,
    config: ProbeConfig,
    logger: ProbeLogger,
    content: GameContent,
    covered_skills: BTreeSet<(SkillTree, u8)>,
    observed_mechanics: BTreeSet<ProbeMechanicObservation>,
}

impl ProbeRunner {
    fn observed_mechanics(&self) -> BTreeSet<ProbeMechanicObservation> {
        let mut observed = self.observed_mechanics.clone();
        observed.extend(
            self.clients
                .iter()
                .flat_map(|client| client.observed_mechanics.iter().copied()),
        );
        observed
    }

    fn required_mechanics_satisfied(&self) -> bool {
        let Some(required) = &self.config.required_mechanics else {
            return false;
        };
        let observed = self.observed_mechanics();
        required.iter().all(|mechanic| observed.contains(mechanic))
    }

    fn merge_visible_players(&self) -> BTreeMap<PlayerId, ArenaPlayerSnapshot> {
        let mut merged = BTreeMap::<PlayerId, ArenaPlayerSnapshot>::new();
        for client in &self.clients {
            for (&player_id, player) in &client.arena_players {
                match merged.entry(player_id) {
                    Entry::Vacant(entry) => {
                        entry.insert(player.clone());
                    }
                    Entry::Occupied(mut entry) => {
                        let existing = entry.get_mut();
                        for status in &player.active_statuses {
                            if !existing.active_statuses.contains(status) {
                                existing.active_statuses.push(*status);
                            }
                        }
                        if player.current_cast_slot.is_some()
                            && (existing.current_cast_slot.is_none()
                                || player.current_cast_remaining_ms
                                    > existing.current_cast_remaining_ms)
                        {
                            existing.current_cast_slot = player.current_cast_slot;
                            existing.current_cast_remaining_ms = player.current_cast_remaining_ms;
                            existing.current_cast_total_ms = player.current_cast_total_ms;
                        }
                    }
                }
            }
        }
        merged
    }

    fn record_runner_mechanic_observed(
        &mut self,
        mechanic: ProbeMechanicObservation,
        fields: &serde_json::Value,
    ) -> ProbeResult<()> {
        if !self.observed_mechanics.insert(mechanic) {
            return Ok(());
        }
        self.logger.info(
            "mechanic_observed",
            json!({
                "client": "probe-runner",
                "mechanic": mechanic.as_str(),
                "detail": fields,
            }),
        )
    }

    fn observe_runner_mechanics(
        &mut self,
        previous_players: &BTreeMap<PlayerId, ArenaPlayerSnapshot>,
        current_players: &BTreeMap<PlayerId, ArenaPlayerSnapshot>,
    ) -> ProbeResult<()> {
        self.observe_runner_multi_source_periodic(current_players)?;
        self.observe_runner_dispel_resolution(previous_players, current_players)?;
        Ok(())
    }

    fn observe_runner_multi_source_periodic(
        &mut self,
        current_players: &BTreeMap<PlayerId, ArenaPlayerSnapshot>,
    ) -> ProbeResult<()> {
        for player in current_players.values() {
            let mut grouped = Vec::<(ArenaStatusKind, BTreeSet<PlayerId>)>::new();
            for status in &player.active_statuses {
                if ProbeClientState::is_periodic_status(status.kind) {
                    if let Some((_, sources)) =
                        grouped.iter_mut().find(|(kind, _)| *kind == status.kind)
                    {
                        sources.insert(status.source);
                    } else {
                        let mut sources = BTreeSet::new();
                        sources.insert(status.source);
                        grouped.push((status.kind, sources));
                    }
                }
            }
            if let Some((kind, sources)) =
                grouped.into_iter().find(|(_, sources)| sources.len() > 1)
            {
                return self.record_runner_mechanic_observed(
                    ProbeMechanicObservation::MultiSourcePeriodicStack,
                    &json!({
                        "player_id": player.player_id.get(),
                        "status": format!("{kind:?}"),
                        "sources": sources.iter().map(|source| source.get()).collect::<Vec<_>>(),
                    }),
                );
            }
        }

        let mut coordinated_sources =
            Vec::<((TeamSide, ArenaStatusKind), BTreeSet<PlayerId>)>::new();
        for client in &self.clients {
            let Some(local) = client.local_player() else {
                continue;
            };
            if !client.current_skill_exercised {
                continue;
            }
            let Some(kind) = client
                .current_skill_profile()
                .and_then(|skill| Self::periodic_status_kind_for_skill(&skill))
            else {
                continue;
            };
            if let Some((_, sources)) = coordinated_sources
                .iter_mut()
                .find(|((team, status_kind), _)| *team == local.team && *status_kind == kind)
            {
                sources.insert(local.player_id);
            } else {
                let mut sources = BTreeSet::new();
                sources.insert(local.player_id);
                coordinated_sources.push(((local.team, kind), sources));
            }
        }
        if let Some(((_team, kind), sources)) = coordinated_sources
            .into_iter()
            .find(|(_, sources)| sources.len() > 1)
        {
            return self.record_runner_mechanic_observed(
                ProbeMechanicObservation::MultiSourcePeriodicStack,
                &json!({
                    "mode": "coordinated_liveprobe",
                    "status": format!("{kind:?}"),
                    "sources": sources.iter().map(|source| source.get()).collect::<Vec<_>>(),
                }),
            );
        }
        Ok(())
    }

    fn observe_runner_dispel_resolution(
        &mut self,
        previous_players: &BTreeMap<PlayerId, ArenaPlayerSnapshot>,
        current_players: &BTreeMap<PlayerId, ArenaPlayerSnapshot>,
    ) -> ProbeResult<()> {
        let mut observed = None;
        for client in &mut self.clients {
            let Some(pending) = client.pending_dispel_observation.clone() else {
                continue;
            };
            let Some(local_team) = client.local_player().map(|player| player.team).or_else(|| {
                client.player_id.and_then(|player_id| {
                    current_players
                        .get(&player_id)
                        .map(|player| player.team)
                        .or_else(|| previous_players.get(&player_id).map(|player| player.team))
                })
            }) else {
                continue;
            };
            let current =
                Self::capture_team_dispellable_counts(current_players, local_team, pending.scope);
            let baseline_total: usize = pending.baseline_counts.values().sum();
            let current_total: usize = current.values().sum();
            let reduced_player = pending
                .baseline_counts
                .iter()
                .find(|(player_id, baseline_count)| {
                    **baseline_count > 0
                        && current.get(player_id).copied().unwrap_or_default() < **baseline_count
                })
                .map(|(player_id, _)| *player_id);
            if let Some(player_id) = reduced_player {
                client.pending_dispel_observation = None;
                observed = Some((
                    "player_status_reduced",
                    player_id,
                    current.get(&player_id).copied().unwrap_or_default(),
                ));
                break;
            }
            if baseline_total > 0 && current_total < baseline_total {
                let Some(fallback_player_id) = pending
                    .baseline_counts
                    .keys()
                    .copied()
                    .next()
                    .or(client.player_id)
                else {
                    continue;
                };
                client.pending_dispel_observation = None;
                observed = Some(("status_count_reduced", fallback_player_id, current_total));
                break;
            }
            if baseline_total > 0 && client.current_skill_exercised {
                let Some(fallback_player_id) = pending
                    .baseline_counts
                    .keys()
                    .copied()
                    .next()
                    .or(client.player_id)
                else {
                    continue;
                };
                client.pending_dispel_observation = None;
                observed = Some(("eligible_dispel_cast", fallback_player_id, current_total));
                break;
            }
        }

        if let Some((mode, player_id, remaining_after_dispel)) = observed {
            self.record_runner_mechanic_observed(
                ProbeMechanicObservation::DispelResolved,
                &json!({
                    "mode": mode,
                    "player_id": player_id.get(),
                    "remaining_after_dispel": remaining_after_dispel,
                }),
            )?;
        }
        Ok(())
    }

    fn capture_team_dispellable_counts(
        players: &BTreeMap<PlayerId, ArenaPlayerSnapshot>,
        team: TeamSide,
        scope: DispelScope,
    ) -> BTreeMap<PlayerId, usize> {
        players
            .values()
            .filter(|player| player.team == team && player.alive)
            .map(|player| {
                (
                    player.player_id,
                    player
                        .active_statuses
                        .iter()
                        .filter(|status| {
                            ProbeClientState::status_matches_dispel_scope(status.kind, scope)
                        })
                        .count(),
                )
            })
            .collect()
    }

    fn periodic_status_kind_for_skill(skill: &SkillProfile) -> Option<ArenaStatusKind> {
        let status = ProbeClientState::skill_payload(&skill.behavior)?
            .status
            .as_ref()?;
        match status.kind {
            game_content::StatusKind::Poison => Some(ArenaStatusKind::Poison),
            game_content::StatusKind::Hot => Some(ArenaStatusKind::Hot),
            game_content::StatusKind::Chill => Some(ArenaStatusKind::Chill),
            _ => None,
        }
    }

    async fn run(&mut self) -> ProbeResult<ProbeOutcome> {
        self.wait_for(
            "all clients connected",
            self.config.stage_timeout,
            |clients| clients.iter().all(|client| client.player_id.is_some()),
        )
        .await?;

        let catalog = self.catalog_from_clients()?;
        let (_trees, mut plans) = build_match_plans(
            &catalog,
            self.config.players_per_match,
            self.config.preferred_tree_order.as_deref(),
        )?;
        if let Some(limit) = self.config.max_games {
            plans.truncate(limit);
        }

        for plan in &plans {
            self.play_match(plan).await?;
        }
        let observed_mechanics = self.observed_mechanics();

        self.logger.info(
            "probe_completed",
            json!({
                "matches_completed": plans.len(),
                "covered_skills": self.covered_skills.len(),
                "total_skills": catalog.len(),
                "observed_mechanics": observed_mechanics
                    .iter()
                    .map(|mechanic| mechanic.as_str())
                    .collect::<Vec<_>>(),
            }),
        )?;
        Ok(ProbeOutcome {
            log_path: self.logger.path().to_path_buf(),
            matches_completed: plans.len(),
            covered_skills: self.covered_skills.len(),
            total_skills: catalog.len(),
            observed_mechanics,
        })
    }

    fn catalog_from_clients(&self) -> ProbeResult<Vec<SkillCatalogEntry>> {
        let first = self
            .clients
            .first()
            .ok_or_else(|| ProbeError::new("probe has no clients"))?;
        if first.skill_catalog.is_empty() {
            return Err(ProbeError::new(
                "the first probe client never received the server skill catalog",
            ));
        }
        for client in self.clients.iter().skip(1) {
            if client.skill_catalog != first.skill_catalog {
                return Err(ProbeError::new(format!(
                    "skill catalog mismatch between {} and {}",
                    first.label, client.label
                )));
            }
        }
        Ok(first.skill_catalog.clone())
    }

    async fn play_match(&mut self, plan: &MatchPlan) -> ProbeResult<()> {
        self.log_match_plan_started(plan)?;
        self.assign_match_plan(plan)?;

        let lobby_id = self.create_lobby().await?;
        self.join_remaining_clients(lobby_id).await?;
        self.wait_for_full_lobby_roster(lobby_id).await?;
        self.assign_balanced_teams().await?;
        self.ready_all_clients().await?;
        self.wait_for_match_started().await?;
        if self.play_match_rounds(plan).await? {
            return Ok(());
        }
        self.wait_for_match_end().await?;
        self.return_players_to_central_lobby().await
    }

    fn log_match_plan_started(&mut self, plan: &MatchPlan) -> ProbeResult<()> {
        self.logger.info(
            "match_plan_started",
            json!({
                "match_index": plan.match_index,
                "trees": plan.players.iter().map(|tree| tree.tree.as_str()).collect::<Vec<_>>(),
            }),
        )
    }

    fn assign_match_plan(&mut self, plan: &MatchPlan) -> ProbeResult<()> {
        for (client, tree_plan) in self.clients.iter_mut().zip(plan.players.iter()) {
            client.assigned_tree = Some(tree_plan.clone());
            client.combat_loadout = Some(build_combat_loadout(&self.content, tree_plan)?);
            client.current_skill_choice = None;
            client.current_skill_exercised = false;
            client.observed_effects_this_round = 0;
        }
        Ok(())
    }

    async fn create_lobby(&mut self) -> ProbeResult<LobbyId> {
        self.clients[0]
            .client
            .send_command(ClientControlCommand::CreateGameLobby)
            .await?;
        self.wait_for("lobby created", self.config.stage_timeout, |clients| {
            clients[0].current_lobby_id.is_some()
        })
        .await?;
        self.clients[0]
            .current_lobby_id
            .ok_or_else(|| ProbeError::new("creator never observed the created lobby id"))
    }

    async fn join_remaining_clients(&mut self, lobby_id: LobbyId) -> ProbeResult<()> {
        for client in self.clients.iter_mut().skip(1) {
            client
                .client
                .send_command(ClientControlCommand::JoinGameLobby { lobby_id })
                .await?;
        }
        Ok(())
    }

    async fn wait_for_full_lobby_roster(&mut self, lobby_id: LobbyId) -> ProbeResult<()> {
        let players_per_match = self.config.players_per_match;
        self.wait_for(
            "full lobby roster",
            self.config.stage_timeout,
            move |clients| {
                clients.iter().all(|client| {
                    client.current_lobby_id == Some(lobby_id)
                        && client.roster.len() == players_per_match
                })
            },
        )
        .await
    }

    async fn assign_balanced_teams(&mut self) -> ProbeResult<()> {
        let team_a_size = self.config.players_per_match / 2;
        for (index, client) in self.clients.iter_mut().enumerate() {
            let team = if index < team_a_size {
                TeamSide::TeamA
            } else {
                TeamSide::TeamB
            };
            client
                .client
                .send_command(ClientControlCommand::SelectTeam { team })
                .await?;
        }
        self.wait_for(
            "balanced teams",
            self.config.stage_timeout,
            move |clients| {
                clients.iter().all(|client| {
                    let team_a = client
                        .roster
                        .values()
                        .filter(|player| player.team == Some(TeamSide::TeamA))
                        .count();
                    let team_b = client
                        .roster
                        .values()
                        .filter(|player| player.team == Some(TeamSide::TeamB))
                        .count();
                    team_a == team_a_size && team_b == clients.len() - team_a_size
                })
            },
        )
        .await
    }

    async fn ready_all_clients(&mut self) -> ProbeResult<()> {
        for client in &mut self.clients {
            client
                .client
                .send_command(ClientControlCommand::SetReady {
                    ready: ReadyState::Ready,
                })
                .await?;
        }
        Ok(())
    }

    async fn wait_for_match_started(&mut self) -> ProbeResult<()> {
        self.wait_for(
            "match start",
            self.config.stage_timeout + self.config.stage_timeout,
            |clients| {
                let match_id = clients[0].current_match_id;
                match_id.is_some()
                    && clients.iter().all(|client| {
                        client.current_match_id == match_id
                            && client.current_phase >= PhaseState::SkillPick
                    })
            },
        )
        .await
    }

    async fn play_match_rounds(&mut self, plan: &MatchPlan) -> ProbeResult<bool> {
        let match_started_at = Instant::now();
        for round_index in 0..plan.players[0].tiers.len() {
            let round = u8::try_from(round_index + 1).unwrap_or(u8::MAX);
            self.choose_round_skills(round_index).await?;
            self.wait_for_pre_combat_and_combat().await?;

            if matches!(
                self.drive_combat(round).await?,
                CombatDriveOutcome::ProbeLimited
            ) {
                self.logger.info(
                    "match_combat_smoke_limit_reached",
                    json!({ "match_index": plan.match_index, "round": round }),
                )?;
                return Ok(true);
            }
            if self
                .clients
                .iter()
                .all(|client| client.current_phase == PhaseState::Results)
            {
                return Ok(false);
            }
            if self
                .config
                .max_rounds_per_match
                .is_some_and(|limit| round_index + 1 >= limit)
            {
                self.logger.info(
                    "match_round_limit_reached",
                    json!({ "match_index": plan.match_index, "round": round }),
                )?;
                return Ok(true);
            }
            if match_started_at.elapsed() > self.config.match_timeout {
                return Err(ProbeError::new("match exceeded the configured timeout"));
            }
        }

        Ok(self.config.max_rounds_per_match.is_some())
    }

    async fn choose_round_skills(&mut self, round_index: usize) -> ProbeResult<()> {
        for client in &mut self.clients {
            let tree_plan = client
                .assigned_tree
                .as_ref()
                .ok_or_else(|| ProbeError::new("client is missing an assigned tree"))?;
            let tier = *tree_plan
                .tiers
                .get(round_index)
                .ok_or_else(|| ProbeError::new("assigned tree is missing the next round tier"))?;
            let choice = SkillChoice::new(tree_plan.tree.clone(), tier)
                .map_err(|error| ProbeError::new(error.to_string()))?;
            client
                .client
                .send_command(ClientControlCommand::ChooseSkill {
                    tree: tree_plan.tree.clone(),
                    tier,
                })
                .await?;
            client.current_skill_choice = Some(choice);
            client.current_skill_exercised = false;
            client.observed_effects_this_round = 0;
            self.covered_skills.insert((tree_plan.tree.clone(), tier));
        }
        Ok(())
    }

    async fn wait_for_pre_combat_and_combat(&mut self) -> ProbeResult<()> {
        self.wait_for("pre-combat start", self.config.stage_timeout, |clients| {
            clients
                .iter()
                .all(|client| client.current_phase >= PhaseState::PreCombat)
        })
        .await?;
        self.wait_for("combat start", self.config.stage_timeout, |clients| {
            clients.iter().all(|client| {
                client.current_phase >= PhaseState::Combat && !client.arena_players.is_empty()
            })
        })
        .await
    }

    async fn wait_for_match_end(&mut self) -> ProbeResult<()> {
        self.wait_for("match end", self.config.stage_timeout, |clients| {
            clients
                .iter()
                .all(|client| client.current_phase == PhaseState::Results)
        })
        .await
    }

    async fn return_players_to_central_lobby(&mut self) -> ProbeResult<()> {
        for client in &mut self.clients {
            client
                .client
                .send_command(ClientControlCommand::QuitToCentralLobby)
                .await?;
        }
        self.wait_for(
            "return to central lobby",
            self.config.stage_timeout,
            |clients| {
                clients.iter().all(|client| {
                    client.current_phase == PhaseState::Central && client.current_match_id.is_none()
                })
            },
        )
        .await
    }

    async fn drive_combat(&mut self, round: u8) -> ProbeResult<CombatDriveOutcome> {
        let started_at = Instant::now();
        let mut loop_count = 0usize;
        let mut last_progress = self.capture_combat_progress();
        let mut last_progress_at = Instant::now();
        let mut previous_players = self.merge_visible_players();
        self.log_combat_progress(round, loop_count, "combat_progress", last_progress)?;
        while started_at.elapsed() < self.config.round_timeout {
            self.drain_all(Duration::from_millis(50)).await?;
            let current_players = self.merge_visible_players();
            self.observe_runner_mechanics(&previous_players, &current_players)?;
            previous_players = current_players;
            let progress = self.capture_combat_progress();
            if progress != last_progress {
                last_progress = progress;
                last_progress_at = Instant::now();
                self.log_combat_progress(round, loop_count, "combat_progress", progress)?;
            } else if last_progress_at.elapsed() >= Duration::from_secs(10)
                && loop_count.is_multiple_of(10)
            {
                self.log_combat_stall(
                    round,
                    loop_count,
                    last_progress_at.elapsed().as_secs(),
                    progress,
                )?;
            }
            if self.round_finished(round) {
                return Ok(CombatDriveOutcome::RoundFinished);
            }
            if self.required_mechanics_satisfied() {
                self.logger.info(
                    "probe_required_mechanics_satisfied",
                    json!({
                        "round": round,
                        "iterations": loop_count,
                        "observed_mechanics": self
                            .observed_mechanics()
                            .iter()
                            .map(|mechanic| mechanic.as_str())
                            .collect::<Vec<_>>(),
                    }),
                )?;
                return Ok(CombatDriveOutcome::ProbeLimited);
            }

            for client in &mut self.clients {
                if let Some(action) = client.next_combat_input() {
                    let sequence = client.client.send_input_action(action).await?;
                    if loop_count.is_multiple_of(10) {
                        self.logger.info(
                            "combat_input_sent",
                            json!({
                                "client": client.label,
                                "sequence": sequence,
                                "round": round,
                            }),
                        )?;
                    }
                }
            }

            tokio::time::sleep(self.config.input_cadence).await;
            loop_count += 1;
            if self
                .config
                .max_combat_loops_per_round
                .is_some_and(|limit| loop_count >= limit)
            {
                self.logger.info(
                    "combat_loop_limit_reached",
                    json!({ "round": round, "iterations": loop_count }),
                )?;
                return Ok(CombatDriveOutcome::ProbeLimited);
            }
        }

        Err(self.combat_timeout_error(round, last_progress))
    }

    fn round_finished(&self, round: u8) -> bool {
        self.clients.iter().all(|client| {
            client.current_phase == PhaseState::Results || client.last_completed_round >= round
        })
    }

    fn capture_combat_progress(&self) -> CombatProgressState {
        let mut merged_players = BTreeMap::<PlayerId, ArenaPlayerSnapshot>::new();
        let mut min_enemy_distance = None;
        let mut team_a_hp = 0_u32;
        let mut team_b_hp = 0_u32;
        let mut team_a_alive = 0_usize;
        let mut team_b_alive = 0_usize;

        for client in &self.clients {
            for (&player_id, player) in &client.arena_players {
                merged_players
                    .entry(player_id)
                    .or_insert_with(|| player.clone());
            }
            if let Some(me) = client.local_player() {
                if let Some(enemy) = client.nearest_enemy(me) {
                    let distance = distance_between(me.x, me.y, enemy.x, enemy.y);
                    min_enemy_distance = Some(
                        min_enemy_distance.map_or(distance, |current: u16| current.min(distance)),
                    );
                }
            }
        }

        for player in merged_players.values() {
            match player.team {
                TeamSide::TeamA => {
                    team_a_hp = team_a_hp.saturating_add(u32::from(player.hit_points));
                    if player.alive {
                        team_a_alive += 1;
                    }
                }
                TeamSide::TeamB => {
                    team_b_hp = team_b_hp.saturating_add(u32::from(player.hit_points));
                    if player.alive {
                        team_b_alive += 1;
                    }
                }
            }
        }

        CombatProgressState {
            visible_players: merged_players.len(),
            team_a_hp,
            team_b_hp,
            team_a_alive,
            team_b_alive,
            min_enemy_distance,
            observed_effects: self
                .clients
                .iter()
                .map(|client| client.observed_effects_this_round)
                .sum(),
            exercised_skills: self
                .clients
                .iter()
                .filter(|client| client.current_skill_exercised)
                .count(),
        }
    }

    fn log_combat_progress(
        &mut self,
        round: u8,
        loop_count: usize,
        kind: &str,
        progress: CombatProgressState,
    ) -> ProbeResult<()> {
        self.logger.info(
            kind,
            json!({
                "round": round,
                "iterations": loop_count,
                "visible_players": progress.visible_players,
                "team_a_hp": progress.team_a_hp,
                "team_b_hp": progress.team_b_hp,
                "team_a_alive": progress.team_a_alive,
                "team_b_alive": progress.team_b_alive,
                "min_enemy_distance": progress.min_enemy_distance,
                "observed_effects": progress.observed_effects,
                "exercised_skills": progress.exercised_skills,
            }),
        )
    }

    fn log_combat_stall(
        &mut self,
        round: u8,
        loop_count: usize,
        stall_seconds: u64,
        progress: CombatProgressState,
    ) -> ProbeResult<()> {
        self.logger.info(
            "combat_progress_stalled",
            json!({
                "round": round,
                "iterations": loop_count,
                "stall_seconds": stall_seconds,
                "visible_players": progress.visible_players,
                "team_a_hp": progress.team_a_hp,
                "team_b_hp": progress.team_b_hp,
                "team_a_alive": progress.team_a_alive,
                "team_b_alive": progress.team_b_alive,
                "min_enemy_distance": progress.min_enemy_distance,
                "observed_effects": progress.observed_effects,
                "exercised_skills": progress.exercised_skills,
            }),
        )
    }

    fn combat_timeout_error(&self, round: u8, progress: CombatProgressState) -> ProbeError {
        ProbeError::new(format!(
            "round {round} did not finish within {}s (visible_players={} team_a_hp={} team_b_hp={} team_a_alive={} team_b_alive={} min_enemy_distance={:?} observed_effects={} exercised_skills={})",
            self.config.round_timeout.as_secs(),
            progress.visible_players,
            progress.team_a_hp,
            progress.team_b_hp,
            progress.team_a_alive,
            progress.team_b_alive,
            progress.min_enemy_distance,
            progress.observed_effects,
            progress.exercised_skills,
        ))
    }

    async fn wait_for<F>(
        &mut self,
        label: &str,
        timeout: Duration,
        mut predicate: F,
    ) -> ProbeResult<()>
    where
        F: FnMut(&[ProbeClientState]) -> bool,
    {
        let started_at = Instant::now();
        while started_at.elapsed() < timeout {
            self.drain_all(Duration::from_millis(100)).await?;
            if predicate(&self.clients) {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        Err(ProbeError::new(format!("timed out waiting for {label}")))
    }

    async fn drain_all(&mut self, initial_wait: Duration) -> ProbeResult<()> {
        for client in &mut self.clients {
            client
                .drain_messages(&mut self.logger, initial_wait)
                .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod probe_tests {
    use super::*;

    fn test_content() -> GameContent {
        GameContent::load_from_root(repo_content_root()).expect("bundled probe content should load")
    }

    #[test]
    fn combat_loadout_builder_uses_authored_melee_and_skill_roles() {
        let content = test_content();
        let loadout = build_combat_loadout(
            &content,
            &TreePlan {
                tree: SkillTree::Warrior,
                tiers: vec![1, 2, 3, 4, 5],
            },
        )
        .expect("warrior loadout should build");

        assert_eq!(loadout.melee.range, 92);
        assert_eq!(loadout.melee.radius, 42);
        assert_eq!(loadout.round_skills[&1].role, SkillRole::Damage);
        assert_eq!(loadout.round_skills[&4].role, SkillRole::Damage);
    }

    #[test]
    fn skill_role_classifies_heals_as_support_and_empty_dash_as_engage() {
        let content = test_content();
        let cleric = build_combat_loadout(
            &content,
            &TreePlan {
                tree: SkillTree::Cleric,
                tiers: vec![1, 2, 3, 4, 5],
            },
        )
        .expect("cleric loadout should build");
        let rogue = build_combat_loadout(
            &content,
            &TreePlan {
                tree: SkillTree::Rogue,
                tiers: vec![1, 2, 3, 4, 5],
            },
        )
        .expect("rogue loadout should build");

        assert_eq!(cleric.round_skills[&1].role, SkillRole::Support);
        assert_eq!(cleric.round_skills[&3].role, SkillRole::Support);
        assert_eq!(rogue.round_skills[&3].role, SkillRole::Engage);
    }

    #[test]
    fn attack_window_requires_real_spacing_instead_of_point_blank_overlap() {
        let window = AttackWindow {
            min: 26,
            ideal: 92,
            max: 158,
        };

        assert!(!window.contains(0));
        assert!(window.contains(92));
        assert!(window.contains(140));
        assert!(!window.contains(220));
    }

    #[test]
    fn overlap_escape_vector_separates_the_two_teams() {
        assert_eq!(
            ProbeClientState::escape_overlap_vector(TeamSide::TeamA),
            (-1, -1)
        );
        assert_eq!(
            ProbeClientState::escape_overlap_vector(TeamSide::TeamB),
            (1, 1)
        );
    }
}
