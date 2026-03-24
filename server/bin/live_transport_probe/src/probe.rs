use std::collections::{btree_map::Entry, BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use game_domain::{LobbyId, MatchId, PlayerId, PlayerRecord, ReadyState, SkillTree, TeamSide};
use game_net::{
    ArenaPlayerSnapshot, ClientControlCommand, LobbySnapshotPlayer, ServerControlEvent,
    SkillCatalogEntry,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeOutcome {
    pub log_path: PathBuf,
    pub matches_completed: usize,
    pub covered_skills: usize,
    pub total_skills: usize,
}

struct ProbeClientState {
    label: String,
    client: LiveClient,
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
    next_slot_cursor: u8,
    used_slots_this_round: BTreeSet<u8>,
    transport_broken: Option<String>,
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

impl ProbeClientState {
    fn new(label: &str, client: LiveClient) -> Self {
        Self {
            label: String::from(label),
            client,
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
            next_slot_cursor: 1,
            used_slots_this_round: BTreeSet::new(),
            transport_broken: None,
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
            "signal_closed"
                | "signal_read_error"
                | "signal_apply_error"
                | "peer_state_failed"
                | "peer_state_disconnected"
        ) {
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
                self.used_slots_this_round.clear();
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
                self.current_round = round.get();
                self.current_phase = PhaseState::SkillPick;
                self.used_slots_this_round.clear();
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
                self.apply_snapshot(&snapshot.players, snapshot.phase);
            }
            ServerControlEvent::ArenaDeltaSnapshot { snapshot } => {
                self.apply_snapshot(&snapshot.players, snapshot.phase);
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
        self.next_slot_cursor = 1;
        self.used_slots_this_round.clear();
        logger.info("returned_to_central", json!({ "client": self.label }))?;
        Ok(())
    }

    fn apply_snapshot(
        &mut self,
        players: &[ArenaPlayerSnapshot],
        phase: game_net::ArenaMatchPhase,
    ) {
        self.arena_players = players
            .iter()
            .cloned()
            .map(|player| (player.player_id, player))
            .collect();
        self.current_phase = phase.into();
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

        let (aim_x, aim_y, move_x, move_y) = self.navigation_target(&me);
        let (buttons, ability_or_context) = self.next_action(&me);
        Some(PendingInput {
            move_x,
            move_y,
            aim_x,
            aim_y,
            buttons,
            ability_or_context,
        })
    }

    fn navigation_target(&self, me: &ArenaPlayerSnapshot) -> (i16, i16, i16, i16) {
        let nearest_enemy = self
            .arena_players
            .values()
            .filter(|player| player.team != me.team && player.alive)
            .min_by_key(|player| {
                let dx = i32::from(player.x) - i32::from(me.x);
                let dy = i32::from(player.y) - i32::from(me.y);
                dx * dx + dy * dy
            });

        let (delta_x, delta_y) = if let Some(enemy) = nearest_enemy {
            (enemy.x.saturating_sub(me.x), enemy.y.saturating_sub(me.y))
        } else {
            (-me.x, -me.y)
        };

        let aim_x = if delta_x == 0 && delta_y == 0 {
            120
        } else {
            delta_x
        };
        let aim_y = if delta_x == 0 && delta_y == 0 {
            0
        } else {
            delta_y
        };
        let move_x = delta_x.signum();
        let move_y = delta_y.signum();
        (aim_x, aim_y, move_x, move_y)
    }

    fn next_action(&mut self, me: &ArenaPlayerSnapshot) -> (u16, u16) {
        let unlocked_slots = usize::from(me.unlocked_skill_slots.min(5));
        if unlocked_slots > 0 {
            for _ in 0..unlocked_slots {
                let slot = self
                    .next_slot_cursor
                    .clamp(1, me.unlocked_skill_slots.max(1));
                self.next_slot_cursor = if slot >= me.unlocked_skill_slots.max(1) {
                    1
                } else {
                    slot + 1
                };
                let slot_index = usize::from(slot - 1);
                if !self.used_slots_this_round.contains(&slot)
                    && me.slot_cooldown_remaining_ms[slot_index] == 0
                {
                    self.used_slots_this_round.insert(slot);
                    return (game_net::BUTTON_CAST, u16::from(slot));
                }
            }
        }

        if me.primary_cooldown_remaining_ms == 0 {
            return (game_net::BUTTON_PRIMARY, 0);
        }

        (0, 0)
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
}

pub async fn run_probe(config: ProbeConfig) -> ProbeResult<ProbeOutcome> {
    let mut logger = ProbeLogger::new(&config.output_path, &config.origin)?;

    let labels = ["probe-a1", "probe-a2", "probe-b1", "probe-b2"];
    let mut clients = Vec::new();
    for label in labels.into_iter().take(config.players_per_match) {
        logger.info("client_connecting", json!({ "client": label }))?;
        let client = LiveClient::connect(&config.origin, label, config.connect_timeout).await?;
        clients.push(ProbeClientState::new(label, client));
    }

    let mut runner = ProbeRunner {
        clients,
        config,
        logger,
        covered_skills: BTreeSet::new(),
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
    covered_skills: BTreeSet<(SkillTree, u8)>,
}

impl ProbeRunner {
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

        self.logger.info(
            "probe_completed",
            json!({
                "matches_completed": plans.len(),
                "covered_skills": self.covered_skills.len(),
                "total_skills": catalog.len(),
            }),
        )?;
        Ok(ProbeOutcome {
            log_path: self.logger.path().to_path_buf(),
            matches_completed: plans.len(),
            covered_skills: self.covered_skills.len(),
            total_skills: catalog.len(),
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
        self.assign_match_plan(plan);

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

    fn assign_match_plan(&mut self, plan: &MatchPlan) {
        for (client, tree_plan) in self.clients.iter_mut().zip(plan.players.iter()) {
            client.assigned_tree = Some(tree_plan.clone());
            client.next_slot_cursor = 1;
        }
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
            client
                .client
                .send_command(ClientControlCommand::ChooseSkill {
                    tree: tree_plan.tree.clone(),
                    tier,
                })
                .await?;
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
        while started_at.elapsed() < self.config.round_timeout {
            self.drain_all(Duration::from_millis(50)).await?;
            if self.round_finished(round) {
                return Ok(CombatDriveOutcome::RoundFinished);
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

        Err(ProbeError::new(format!(
            "round {round} did not finish within {}s",
            self.config.round_timeout.as_secs()
        )))
    }

    fn round_finished(&self, round: u8) -> bool {
        self.clients.iter().all(|client| {
            client.current_phase == PhaseState::Results || client.last_completed_round >= round
        })
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
