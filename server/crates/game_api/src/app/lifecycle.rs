use game_domain::{LobbyId, MatchId, MatchOutcome, PlayerId, TeamAssignment, TeamSide};
use game_lobby::{LobbyEvent, LobbyPhase};
use game_match::{MatchConfig, MatchEvent, MatchPhase, MatchSession};
use game_net::ServerControlEvent;
use game_sim::COMBAT_FRAME_MS;

use crate::combat_log::CombatLogEvent;

use super::{build_world, AppTransport, MatchRuntime, PlayerLocation, ServerApp};

impl ServerApp {
    pub(super) fn advance_combat_frames<T: AppTransport>(&mut self, transport: &mut T) {
        let match_ids = self.matches.keys().copied().collect::<Vec<_>>();
        for match_id in match_ids {
            if !self.is_combat_match(match_id) {
                continue;
            }

            self.advance_combat_match(transport, match_id);
        }
    }

    fn is_combat_match(&self, match_id: MatchId) -> bool {
        self.matches
            .get(&match_id)
            .is_some_and(|runtime| matches!(runtime.session.phase(), MatchPhase::Combat))
    }

    fn advance_combat_match<T: AppTransport>(&mut self, transport: &mut T, match_id: MatchId) {
        let Some((effect_batch, match_events, errors)) = self.tick_combat_match(match_id) else {
            return;
        };

        for (recipient, entries) in self.drain_match_combat_text(match_id) {
            if entries.is_empty() {
                continue;
            }
            self.send_event(
                transport,
                recipient,
                ServerControlEvent::ArenaCombatTextBatch { entries },
            );
        }
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
        self.broadcast_arena_delta_snapshot(transport, match_id);
    }

    fn tick_combat_match(
        &mut self,
        match_id: MatchId,
    ) -> Option<(
        Vec<game_net::ArenaEffectSnapshot>,
        Vec<MatchEvent>,
        Vec<String>,
    )> {
        let (simulation_events, effect_batch, match_events, mut combined_errors) = {
            let runtime = self.matches.get_mut(&match_id)?;
            runtime.combat_frame_index = runtime.combat_frame_index.saturating_add(1);
            let simulation_events = runtime.world.tick(COMBAT_FRAME_MS);
            let effect_batch = Self::collect_effect_batch(&simulation_events);
            let (match_events, progress_errors) =
                Self::resolve_match_progress(runtime, &simulation_events, &self.content);
            (
                simulation_events,
                effect_batch,
                match_events,
                progress_errors,
            )
        };
        if let Err(error) = self.append_simulation_logs(match_id, &simulation_events) {
            combined_errors.push(error.to_string());
        }
        Some((effect_batch, match_events, combined_errors))
    }

    fn resolve_match_progress(
        runtime: &mut MatchRuntime,
        simulation_events: &[game_sim::SimulationEvent],
        content: &game_content::GameContent,
    ) -> (Vec<MatchEvent>, Vec<String>) {
        let mut match_events = Vec::new();
        let mut errors = Vec::new();
        for target_id in Self::collect_defeated_targets(simulation_events) {
            match runtime.session.mark_player_defeated(target_id) {
                Ok(events) => match_events.extend(events),
                Err(error) => errors.push(error.to_string()),
            }
        }

        if matches!(runtime.session.phase(), MatchPhase::SkillPick { .. }) {
            runtime.rebuild_world(content);
        }

        (match_events, errors)
    }

    pub(super) fn advance_lobby_countdowns<T: AppTransport>(&mut self, transport: &mut T) {
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

    pub(super) fn advance_match_phases<T: AppTransport>(&mut self, transport: &mut T) {
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

    pub(super) fn dispatch_match_events<T: AppTransport>(
        &mut self,
        transport: &mut T,
        match_id: MatchId,
        events: &[MatchEvent],
    ) {
        let recipients = self.match_recipients(match_id);
        for event in events {
            match event {
                MatchEvent::SkillChosen { player_id, choice } => {
                    if let Err(error) = self.append_match_log(
                        match_id,
                        CombatLogEvent::SkillPicked {
                            player_id: player_id.get(),
                            tree: choice.tree.to_string(),
                            tier: choice.tier,
                        },
                    ) {
                        self.broadcast_event(
                            transport,
                            &recipients,
                            ServerControlEvent::Error {
                                message: error.to_string(),
                            },
                        );
                    }
                    self.broadcast_event(
                        transport,
                        &recipients,
                        ServerControlEvent::SkillChosen {
                            player_id: *player_id,
                            tree: choice.tree.clone(),
                            tier: choice.tier,
                        },
                    );
                }
                MatchEvent::PreCombatStarted { seconds_remaining } => {
                    if let Err(error) = self.append_match_log(
                        match_id,
                        CombatLogEvent::PreCombatStarted {
                            seconds_remaining: *seconds_remaining,
                        },
                    ) {
                        self.broadcast_event(
                            transport,
                            &recipients,
                            ServerControlEvent::Error {
                                message: error.to_string(),
                            },
                        );
                    }
                    self.broadcast_event(
                        transport,
                        &recipients,
                        ServerControlEvent::PreCombatStarted {
                            seconds_remaining: *seconds_remaining,
                        },
                    );
                }
                MatchEvent::CombatStarted => {
                    if let Some(runtime) = self.matches.get_mut(&match_id) {
                        runtime.rebuild_world(&self.content);
                        runtime.combat_frame_index = 0;
                    }
                    if let Err(error) =
                        self.append_match_log(match_id, CombatLogEvent::CombatStarted)
                    {
                        self.broadcast_event(
                            transport,
                            &recipients,
                            ServerControlEvent::Error {
                                message: error.to_string(),
                            },
                        );
                    }
                    self.broadcast_event(transport, &recipients, ServerControlEvent::CombatStarted);
                }
                MatchEvent::RoundWon {
                    round,
                    winning_team,
                    score,
                } => {
                    let round_summary = self
                        .matches
                        .get_mut(&match_id)
                        .map(|runtime| runtime.feedback.finalize_round(&runtime.roster, *round));
                    if let Err(error) = self.append_match_log(
                        match_id,
                        CombatLogEvent::RoundWon {
                            round: round.get(),
                            winning_team: super::map_team(*winning_team),
                            score_a: score.team_a,
                            score_b: score.team_b,
                        },
                    ) {
                        self.broadcast_event(
                            transport,
                            &recipients,
                            ServerControlEvent::Error {
                                message: error.to_string(),
                            },
                        );
                    }
                    self.broadcast_event(
                        transport,
                        &recipients,
                        ServerControlEvent::RoundWon {
                            round: *round,
                            winning_team: *winning_team,
                            score_a: score.team_a,
                            score_b: score.team_b,
                        },
                    );
                    if let Some(summary) = round_summary {
                        self.broadcast_event(
                            transport,
                            &recipients,
                            ServerControlEvent::RoundSummary { summary },
                        );
                    }
                }
                MatchEvent::MatchEnded {
                    outcome,
                    message,
                    score,
                } => {
                    let match_summary = self.matches.get(&match_id).map(|runtime| {
                        runtime.feedback.build_match_summary(
                            &runtime.roster,
                            score.team_a.saturating_add(score.team_b),
                        )
                    });
                    self.apply_match_outcome(transport, match_id, *outcome);
                    if let Err(error) = self.append_match_log(
                        match_id,
                        CombatLogEvent::MatchEnded {
                            outcome: super::map_outcome(*outcome),
                            score_a: score.team_a,
                            score_b: score.team_b,
                            message: message.clone(),
                        },
                    ) {
                        self.broadcast_event(
                            transport,
                            &recipients,
                            ServerControlEvent::Error {
                                message: error.to_string(),
                            },
                        );
                    }
                    self.broadcast_event(
                        transport,
                        &recipients,
                        ServerControlEvent::MatchEnded {
                            outcome: *outcome,
                            score_a: score.team_a,
                            score_b: score.team_b,
                            message: message.clone(),
                        },
                    );
                    if let Some(summary) = match_summary {
                        self.broadcast_event(
                            transport,
                            &recipients,
                            ServerControlEvent::MatchSummary { summary },
                        );
                    }
                }
                MatchEvent::ManualResolutionRequired { reason } => {
                    self.broadcast_event(
                        transport,
                        &recipients,
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

    pub(super) fn broadcast_lobby_events<T: AppTransport>(
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

    pub(super) fn start_match_from_lobby<T: AppTransport>(
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
                player.reset_combat_input_state();
            }
        }

        self.matches.insert(
            match_id,
            MatchRuntime {
                world: build_world(&roster, &session, &self.content),
                roster,
                participants: participants.clone(),
                session,
                explored_tiles: participants
                    .iter()
                    .copied()
                    .map(|player_id| (player_id, Self::blank_visibility_mask(self.content.map())))
                    .collect(),
                combat_frame_index: 0,
                feedback: Default::default(),
            },
        );
        self.game_lobbies.remove(&lobby_id);
        self.broadcast_lobby_directory_snapshot(transport);

        if let Err(error) = self.append_match_log(
            match_id,
            CombatLogEvent::MatchStarted {
                participant_player_ids: participants
                    .iter()
                    .map(|player_id| player_id.get())
                    .collect(),
            },
        ) {
            self.broadcast_event(
                transport,
                &participants,
                ServerControlEvent::Error {
                    message: error.to_string(),
                },
            );
        }

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

    pub(super) fn apply_match_outcome<T: AppTransport>(
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

    pub(super) fn persist_player_record<T: AppTransport>(
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

    pub(super) fn end_match_as_no_contest<T: AppTransport>(
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
        if let ServerControlEvent::MatchEnded {
            outcome,
            score_a,
            score_b,
            message,
        } = &ended_event
        {
            let _ = self.append_match_log(
                match_id,
                CombatLogEvent::MatchEnded {
                    outcome: super::map_outcome(*outcome),
                    score_a: *score_a,
                    score_b: *score_b,
                    message: message.clone(),
                },
            );
        }
        self.broadcast_event(transport, &recipients, ended_event);
        if let Some(summary) = self.matches.get(&match_id).map(|runtime| {
            runtime.feedback.build_match_summary(
                &runtime.roster,
                runtime
                    .session
                    .score()
                    .team_a
                    .saturating_add(runtime.session.score().team_b),
            )
        }) {
            self.broadcast_event(
                transport,
                &recipients,
                ServerControlEvent::MatchSummary { summary },
            );
        }
    }
}
