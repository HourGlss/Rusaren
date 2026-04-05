use crate::combat_log::{CombatLogCastCancelReason, CombatLogEvent};
use game_domain::{PlayerId, SkillChoice, TeamAssignment, TeamSide};
use game_match::MatchPhase;
use game_net::{
    ServerControlEvent, ValidatedInputFrame, BUTTON_CANCEL, BUTTON_CAST, BUTTON_PRIMARY,
    BUTTON_QUIT_TO_LOBBY, BUTTON_SELF_CAST,
};
use game_sim::{MovementIntent, SimulationWorld};

use super::super::{
    build_training_world, MatchRuntime, PlayerLocation, ServerApp, TrainingMetrics, TrainingRuntime,
};
use super::AppTransport;

struct InputFrameOutcome {
    aim_changed: bool,
    manual_cancel_slot: Option<u8>,
}

impl ServerApp {
    pub(in super::super) fn handle_start_training<T: AppTransport>(
        &mut self,
        transport: &mut T,
        sender_id: PlayerId,
    ) {
        if !self.ensure_location(transport, sender_id, PlayerLocation::CentralLobby) {
            return;
        }
        let Some(training_map) = self.content.training_map().cloned() else {
            self.send_error(
                transport,
                sender_id,
                "training is unavailable because training_arena.txt is missing",
            );
            return;
        };
        let explored_mask = Self::blank_visibility_mask(&training_map);
        let Some((player_name, record)) = self
            .players
            .get(&sender_id)
            .map(|player| (player.player_name.clone(), player.record.clone()))
        else {
            self.send_error(transport, sender_id, "player is not connected");
            return;
        };
        let training_id = self.allocate_match_id();
        let participant = TeamAssignment {
            player_id: sender_id,
            player_name,
            record,
            team: TeamSide::TeamA,
        };
        let loadout = [None, None, None, None, None];
        let mut explored_tiles = std::collections::BTreeMap::new();
        explored_tiles.insert(sender_id, explored_mask);

        self.training_sessions.insert(
            training_id,
            TrainingRuntime {
                participant: participant.clone(),
                loadout: loadout.clone(),
                world: build_training_world(&participant, &loadout, &self.content),
                explored_tiles,
                combat_frame_index: 0,
                metrics: TrainingMetrics::default(),
            },
        );
        if let Some(player) = self.players.get_mut(&sender_id) {
            player.location = PlayerLocation::Training(training_id);
            player.reset_combat_input_state();
        }
        self.send_event(
            transport,
            sender_id,
            ServerControlEvent::TrainingStarted { training_id },
        );
        self.broadcast_training_state_snapshot(transport, training_id);
        self.broadcast_lobby_directory_snapshot(transport);
    }

    pub(in super::super) fn handle_choose_skill<T: AppTransport>(
        &mut self,
        transport: &mut T,
        sender_id: PlayerId,
        tree: game_domain::SkillTree,
        tier: u8,
    ) {
        let choice = match SkillChoice::new(tree.clone(), tier) {
            Ok(choice) => choice,
            Err(error) => {
                self.send_error(transport, sender_id, &error.to_string());
                return;
            }
        };
        if self.content.skills().resolve(&choice).is_none() {
            self.send_error(
                transport,
                sender_id,
                &format!("no authored skill exists for {tree} tier {tier}"),
            );
            return;
        }

        let Some(location) = self.players.get(&sender_id).map(|player| player.location) else {
            self.send_error(transport, sender_id, "player is not connected");
            return;
        };
        match location {
            PlayerLocation::Match(match_id) => {
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
            PlayerLocation::Training(training_id) => {
                let slot_index = usize::from(tier.saturating_sub(1));
                let Some(runtime) = self.training_sessions.get_mut(&training_id) else {
                    self.send_error(transport, sender_id, "training session does not exist");
                    return;
                };
                if slot_index >= runtime.loadout.len() {
                    self.send_error(transport, sender_id, "skill tier must be between 1 and 5");
                    return;
                }
                runtime.loadout[slot_index] = Some(choice);
                runtime.rebuild_world(&self.content);
                self.send_event(
                    transport,
                    sender_id,
                    ServerControlEvent::SkillChosen {
                        player_id: sender_id,
                        slot: tier,
                        tree,
                        tier,
                    },
                );
                self.broadcast_training_state_snapshot(transport, training_id);
            }
            _ => self.send_error(
                transport,
                sender_id,
                "player is not in a match or training session",
            ),
        }
    }

    pub(in super::super) fn handle_reset_training_session<T: AppTransport>(
        &mut self,
        transport: &mut T,
        sender_id: PlayerId,
    ) {
        let Some(training_id) = self.require_training(transport, sender_id) else {
            return;
        };
        let Some(runtime) = self.training_sessions.get_mut(&training_id) else {
            self.send_error(transport, sender_id, "training session does not exist");
            return;
        };
        runtime.reset_session();
        self.broadcast_training_state_snapshot(transport, training_id);
    }

    pub(in super::super) fn handle_quit_to_central_lobby<T: AppTransport>(
        &mut self,
        transport: &mut T,
        sender_id: PlayerId,
    ) {
        if let Some(training_id) =
            self.players
                .get(&sender_id)
                .and_then(|player| match player.location {
                    PlayerLocation::Training(training_id) => Some(training_id),
                    _ => None,
                })
        {
            if let Some(record) = self
                .players
                .get(&sender_id)
                .map(|player| player.record.clone())
            {
                if let Some(player) = self.players.get_mut(&sender_id) {
                    player.location = PlayerLocation::CentralLobby;
                }
                self.send_event(
                    transport,
                    sender_id,
                    ServerControlEvent::ReturnedToCentralLobby { record },
                );
                self.send_lobby_directory_snapshot(transport, sender_id);
                self.cleanup_finished_training(training_id);
                self.broadcast_lobby_directory_snapshot(transport);
            }
            return;
        }

        let match_id = match self.require_results(transport, sender_id) {
            Some(match_id) => match_id,
            None => return,
        };

        if let Some(record) = self
            .players
            .get(&sender_id)
            .map(|player| player.record.clone())
        {
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

    pub(in super::super) fn handle_input_frame<T: AppTransport>(
        &mut self,
        transport: &mut T,
        sender_id: PlayerId,
        frame: ValidatedInputFrame,
    ) {
        let Some(location) = self.players.get(&sender_id).map(|player| player.location) else {
            self.send_error(transport, sender_id, "player is not connected");
            return;
        };
        let arena_id = match location {
            PlayerLocation::Match(_match_id) => {
                let Some(match_id) =
                    self.require_combat_match_id(transport, sender_id, frame.buttons)
                else {
                    return;
                };
                (true, match_id)
            }
            PlayerLocation::Training(training_id) => {
                if frame.buttons & BUTTON_QUIT_TO_LOBBY != 0 {
                    self.send_error(
                        transport,
                        sender_id,
                        "quit-to-lobby input is not valid during training",
                    );
                    return;
                }
                (false, training_id)
            }
            _ => {
                self.send_error(
                    transport,
                    sender_id,
                    "input frames are only accepted during combat or training",
                );
                return;
            }
        };
        let movement = match Self::decode_movement_input(&frame) {
            Ok(movement) => movement,
            Err(message) => {
                self.send_error(transport, sender_id, &message);
                return;
            }
        };
        let (is_match, arena_id) = arena_id;
        let outcome = if is_match {
            match self.handle_match_input_frame(arena_id, sender_id, &frame, movement) {
                Ok(outcome) => outcome,
                Err(message) => {
                    self.send_error(transport, sender_id, &message);
                    return;
                }
            }
        } else {
            match self.handle_training_input_frame(arena_id, sender_id, &frame, movement) {
                Ok(outcome) => outcome,
                Err(message) => {
                    self.send_error(transport, sender_id, &message);
                    return;
                }
            }
        };

        self.finish_input_frame(transport, sender_id, is_match, arena_id, outcome);
    }

    fn handle_match_input_frame(
        &mut self,
        match_id: game_domain::MatchId,
        sender_id: PlayerId,
        frame: &ValidatedInputFrame,
        movement: MovementIntent,
    ) -> Result<InputFrameOutcome, String> {
        let runtime = self
            .matches
            .get_mut(&match_id)
            .ok_or_else(|| String::from("match does not exist"))?;
        let outcome = Self::apply_world_input(&mut runtime.world, sender_id, frame, movement)?;
        Self::queue_requested_actions(&self.content, runtime, sender_id, frame)?;
        Ok(outcome)
    }

    fn handle_training_input_frame(
        &mut self,
        training_id: game_domain::MatchId,
        sender_id: PlayerId,
        frame: &ValidatedInputFrame,
        movement: MovementIntent,
    ) -> Result<InputFrameOutcome, String> {
        let runtime = self
            .training_sessions
            .get_mut(&training_id)
            .ok_or_else(|| String::from("training session does not exist"))?;
        let outcome = Self::apply_world_input(&mut runtime.world, sender_id, frame, movement)?;
        Self::queue_requested_training_actions(&self.content, runtime, sender_id, frame)?;
        Ok(outcome)
    }

    fn apply_world_input(
        world: &mut SimulationWorld,
        sender_id: PlayerId,
        frame: &ValidatedInputFrame,
        movement: MovementIntent,
    ) -> Result<InputFrameOutcome, String> {
        let aim_changed = world
            .update_aim(sender_id, frame.aim_horizontal_q, frame.aim_vertical_q)
            .map_err(|error| error.to_string())?;
        world
            .submit_input(sender_id, movement)
            .map_err(|error| error.to_string())?;

        let manual_cancel_slot = if frame.buttons & BUTTON_CANCEL != 0 {
            let slot = world
                .player_state(sender_id)
                .and_then(|state| state.current_cast_slot);
            let _ = world
                .cancel_active_cast(sender_id)
                .map_err(|error| error.to_string())?;
            slot
        } else {
            None
        };

        Ok(InputFrameOutcome {
            aim_changed,
            manual_cancel_slot,
        })
    }

    fn finish_input_frame<T: AppTransport>(
        &mut self,
        transport: &mut T,
        sender_id: PlayerId,
        is_match: bool,
        arena_id: game_domain::MatchId,
        outcome: InputFrameOutcome,
    ) {
        if let Some(slot) = outcome.manual_cancel_slot {
            if is_match {
                let _ = self.append_match_log(
                    arena_id,
                    CombatLogEvent::CastCanceled {
                        player_id: sender_id.get(),
                        slot,
                        reason: CombatLogCastCancelReason::Manual,
                    },
                );
            }
        }
        if !outcome.aim_changed {
            return;
        }
        if is_match {
            self.broadcast_arena_delta_snapshot(transport, arena_id);
        } else {
            self.broadcast_training_delta_snapshot(transport, arena_id);
        }
    }

    fn queue_requested_training_actions(
        content: &game_content::GameContent,
        runtime: &mut TrainingRuntime,
        sender_id: PlayerId,
        frame: &ValidatedInputFrame,
    ) -> Result<(), String> {
        if frame.buttons & BUTTON_PRIMARY != 0 {
            runtime
                .world
                .queue_primary_attack(sender_id)
                .map_err(|error| error.to_string())?;
        }

        if frame.buttons & BUTTON_CAST == 0 {
            return Ok(());
        }
        let slot = u8::try_from(frame.ability_or_context).unwrap_or(u8::MAX);
        let self_cast = frame.buttons & BUTTON_SELF_CAST != 0;
        if slot == 0 || slot > 5 {
            return Err(format!("skill slot {slot} is not valid"));
        }
        let Some(choice) = runtime.loadout[usize::from(slot - 1)].as_ref() else {
            return Err(format!("skill slot {slot} is not equipped"));
        };
        if content.skills().resolve(choice).is_none() {
            return Err(format!(
                "authored skill data is missing for {} tier {}",
                choice.tree, choice.tier
            ));
        }
        runtime
            .world
            .queue_cast_with_mode(sender_id, slot, self_cast)
            .map_err(|error| error.to_string())
    }

    fn require_combat_match_id<T: AppTransport>(
        &mut self,
        transport: &mut T,
        sender_id: PlayerId,
        buttons: u16,
    ) -> Option<game_domain::MatchId> {
        let match_id = self.require_match(transport, sender_id)?;
        let Some(runtime) = self.matches.get(&match_id) else {
            self.send_error(transport, sender_id, "match does not exist");
            return None;
        };

        if !matches!(runtime.session.phase(), MatchPhase::Combat) {
            self.send_error(
                transport,
                sender_id,
                "input frames are only accepted during combat",
            );
            return None;
        }
        if buttons & BUTTON_QUIT_TO_LOBBY != 0 {
            self.send_error(
                transport,
                sender_id,
                "quit-to-lobby input is not valid during combat",
            );
            return None;
        }

        Some(match_id)
    }

    fn decode_movement_input(frame: &ValidatedInputFrame) -> Result<MovementIntent, String> {
        let move_x = Self::decode_axis(frame.move_horizontal_q, "move_horizontal_q")?;
        let move_y = Self::decode_axis(frame.move_vertical_q, "move_vertical_q")?;
        MovementIntent::new(move_x, move_y).map_err(|error| error.to_string())
    }

    fn queue_requested_actions(
        content: &game_content::GameContent,
        runtime: &mut MatchRuntime,
        sender_id: PlayerId,
        frame: &ValidatedInputFrame,
    ) -> Result<(), String> {
        if frame.buttons & BUTTON_PRIMARY != 0 {
            runtime
                .world
                .queue_primary_attack(sender_id)
                .map_err(|error| error.to_string())?;
        }

        if frame.buttons & BUTTON_CAST == 0 {
            return Ok(());
        }

        let slot = u8::try_from(frame.ability_or_context).unwrap_or(u8::MAX);
        let self_cast = frame.buttons & BUTTON_SELF_CAST != 0;
        let unlocked_slots = runtime.session.current_round().get();
        if slot == 0 || slot > unlocked_slots {
            return Err(format!(
                "skill slot {slot} is not unlocked for round {unlocked_slots}"
            ));
        }

        let Some(choice) = runtime.session.equipped_choice(sender_id, slot) else {
            return Err(format!("skill slot {slot} is not equipped"));
        };
        if content.skills().resolve(&choice).is_none() {
            return Err(format!(
                "authored skill data is missing for {} tier {}",
                choice.tree, choice.tier
            ));
        }

        runtime
            .world
            .queue_cast_with_mode(sender_id, slot, self_cast)
            .map_err(|error| error.to_string())
    }
}
