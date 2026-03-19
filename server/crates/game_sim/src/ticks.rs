use super::*;

impl SimulationWorld {
    pub(super) fn advance_cooldowns(&mut self, delta_ms: u16) {
        for player in self.players.values_mut() {
            player.primary_cooldown_remaining_ms = player
                .primary_cooldown_remaining_ms
                .saturating_sub(delta_ms);
            for remaining in &mut player.slot_cooldown_remaining_ms {
                *remaining = remaining.saturating_sub(delta_ms);
            }
        }
    }

    pub(super) fn advance_mana(&mut self, delta_ms: u16) {
        for player in self.players.values_mut() {
            if player.mana >= player.max_mana {
                player.mana_regen_progress = 0;
                continue;
            }

            let total_progress = u32::from(player.mana_regen_progress)
                + (u32::from(delta_ms) * u32::from(PLAYER_MANA_REGEN_PER_SECOND));
            let gained = u16::try_from(total_progress / 1000).unwrap_or(u16::MAX);
            player.mana_regen_progress = u16::try_from(total_progress % 1000).unwrap_or(0);
            if gained == 0 {
                continue;
            }

            player.mana = player.mana.saturating_add(gained).min(player.max_mana);
        }
    }

    pub(super) fn apply_status_ticks(&mut self, delta_ms: u16, events: &mut Vec<SimulationEvent>) {
        let player_ids = self.players.keys().copied().collect::<Vec<_>>();
        for player_id in player_ids {
            if !self
                .players
                .get(&player_id)
                .is_some_and(|player| player.alive)
            {
                continue;
            }

            let mut pending_effects = Vec::new();
            {
                let Some(player) = self.players.get_mut(&player_id) else {
                    continue;
                };
                let mut retained_statuses = Vec::with_capacity(player.statuses.len());
                for mut status in std::mem::take(&mut player.statuses) {
                    status.remaining_ms = status.remaining_ms.saturating_sub(delta_ms);
                    if let Some(interval_ms) = status.tick_interval_ms {
                        status.tick_progress_ms = status.tick_progress_ms.saturating_add(delta_ms);
                        while status.tick_progress_ms >= interval_ms {
                            status.tick_progress_ms =
                                status.tick_progress_ms.saturating_sub(interval_ms);
                            pending_effects.push((
                                status.source,
                                status.kind,
                                status.magnitude.saturating_mul(u16::from(status.stacks)),
                            ));
                        }
                    }
                    if status.remaining_ms > 0 {
                        retained_statuses.push(status);
                    }
                }
                player.statuses = retained_statuses;
            }

            for (source, kind, amount) in pending_effects {
                match kind {
                    StatusKind::Poison => {
                        events.extend(self.apply_damage_internal(source, &[player_id], amount));
                    }
                    StatusKind::Hot => {
                        events.extend(self.apply_healing_internal(source, &[player_id], amount));
                    }
                    StatusKind::Chill
                    | StatusKind::Root
                    | StatusKind::Haste
                    | StatusKind::Silence
                    | StatusKind::Stun => {}
                }
            }
        }
    }

    pub(super) fn move_players(&mut self, delta_ms: u16, events: &mut Vec<SimulationEvent>) {
        let arena_width_units = self.arena_width_units;
        let arena_height_units = self.arena_height_units;
        let obstacles = self.obstacles.clone();

        for (player_id, player) in &mut self.players {
            if !player.alive {
                continue;
            }

            let movement_blocked = player
                .statuses
                .iter()
                .any(|status| matches!(status.kind, StatusKind::Root | StatusKind::Stun));
            let movement = if movement_blocked {
                MovementIntent::zero()
            } else {
                player.movement_intent
            };
            player.moving = movement != MovementIntent::zero();
            if !player.moving {
                continue;
            }

            let speed_modifier_bps = total_move_modifier_bps(&player.statuses);
            let speed = adjusted_move_speed(delta_ms, speed_modifier_bps);
            if speed == 0 {
                continue;
            }

            let (delta_x, delta_y) = movement_delta(movement, speed);
            let next_x = i32::from(player.x) + delta_x;
            let next_y = i32::from(player.y) + delta_y;
            let (resolved_x, resolved_y) = resolve_movement(
                player.x,
                player.y,
                next_x,
                next_y,
                arena_width_units,
                arena_height_units,
                &obstacles,
            );

            if resolved_x != player.x || resolved_y != player.y {
                player.x = resolved_x;
                player.y = resolved_y;
                events.push(SimulationEvent::PlayerMoved {
                    player_id: *player_id,
                    x: player.x,
                    y: player.y,
                });
            }
        }
    }
}
