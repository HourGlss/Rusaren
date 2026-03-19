use game_domain::{PlayerId, SkillChoice};
use game_match::MatchPhase;
use game_net::{
    ServerControlEvent, ValidatedInputFrame, BUTTON_CAST, BUTTON_PRIMARY, BUTTON_QUIT_TO_LOBBY,
};
use game_sim::MovementIntent;

use super::super::{PlayerLocation, ServerApp};
use super::AppTransport;

impl ServerApp {
    pub(in super::super) fn handle_choose_skill<T: AppTransport>(
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

    pub(in super::super) fn handle_quit_to_central_lobby<T: AppTransport>(
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
    pub(in super::super) fn handle_input_frame<T: AppTransport>(
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
            let Some(skill) = self.content.skills().resolve(&choice) else {
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
            self.broadcast_arena_delta_snapshot(transport, match_id);
        }
    }
}
