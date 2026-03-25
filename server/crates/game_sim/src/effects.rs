use super::{
    point_distance_sq, point_distance_units, round_f32_to_i32, saturating_i16, segment_distance_sq,
    travel_distance_units, truncate_line_to_obstacles, ArenaEffect, ArenaEffectKind,
    CombatValueKind, MovementIntent, PlayerId, ProjectileState, SimulationEvent, SimulationWorld,
    StatusDefinition, StatusInstance, StatusKind, PLAYER_RADIUS_UNITS,
};

impl SimulationWorld {
    fn player_overlap_radius(radius: u16) -> u16 {
        radius.saturating_add(PLAYER_RADIUS_UNITS)
    }

    pub(super) fn advance_projectiles(&mut self, delta_ms: u16, events: &mut Vec<SimulationEvent>) {
        let mut next_projectiles = Vec::new();
        let projectiles = std::mem::take(&mut self.projectiles);
        for projectile in projectiles {
            let step_distance = travel_distance_units(projectile.speed_units_per_second, delta_ms);
            if step_distance == 0 {
                next_projectiles.push(projectile);
                continue;
            }

            let desired_x =
                f32::from(projectile.x) + projectile.direction_x * f32::from(step_distance);
            let desired_y =
                f32::from(projectile.y) + projectile.direction_y * f32::from(step_distance);
            let desired_end = (
                saturating_i16(round_f32_to_i32(desired_x)),
                saturating_i16(round_f32_to_i32(desired_y)),
            );
            let clipped_end = truncate_line_to_obstacles(
                (projectile.x, projectile.y),
                desired_end,
                &self.obstacles,
            );
            let target = self.find_first_player_on_segment(
                projectile.owner,
                (projectile.x, projectile.y),
                clipped_end,
                projectile.radius,
            );
            if let Some(target) = target {
                events.extend(self.apply_payload(
                    projectile.owner,
                    projectile.slot,
                    &[target],
                    projectile.payload,
                ));
                events.push(SimulationEvent::EffectSpawned {
                    effect: ArenaEffect {
                        kind: ArenaEffectKind::HitSpark,
                        owner: projectile.owner,
                        slot: projectile.slot,
                        x: clipped_end.0,
                        y: clipped_end.1,
                        target_x: clipped_end.0,
                        target_y: clipped_end.1,
                        radius: projectile.radius.saturating_mul(2),
                    },
                });
                continue;
            }

            let traveled = point_distance_units((projectile.x, projectile.y), clipped_end);
            let remaining_range = projectile
                .remaining_range_units
                .saturating_sub(i32::from(traveled));
            let blocked = clipped_end != desired_end;
            if remaining_range <= 0 || blocked {
                if blocked {
                    events.push(SimulationEvent::EffectSpawned {
                        effect: ArenaEffect {
                            kind: ArenaEffectKind::HitSpark,
                            owner: projectile.owner,
                            slot: projectile.slot,
                            x: clipped_end.0,
                            y: clipped_end.1,
                            target_x: clipped_end.0,
                            target_y: clipped_end.1,
                            radius: projectile.radius.saturating_mul(2),
                        },
                    });
                }
                continue;
            }

            next_projectiles.push(ProjectileState {
                x: clipped_end.0,
                y: clipped_end.1,
                remaining_range_units: remaining_range,
                ..projectile
            });
        }

        self.projectiles = next_projectiles;
    }

    pub(super) fn apply_payload(
        &mut self,
        source: PlayerId,
        slot: u8,
        targets: &[PlayerId],
        payload: game_content::EffectPayload,
    ) -> Vec<SimulationEvent> {
        if targets.is_empty() {
            return Vec::new();
        }

        let mut events = match payload.kind {
            CombatValueKind::Damage => self.apply_damage_internal(source, targets, payload.amount),
            CombatValueKind::Heal => self.apply_healing_internal(source, targets, payload.amount),
        };

        if let Some(status) = payload.status {
            for target in targets {
                if let Some(event) = self.apply_status(source, *target, slot, status) {
                    events.push(event);
                }
            }
        }

        events
    }

    pub(super) fn apply_damage_internal(
        &mut self,
        attacker: PlayerId,
        targets: &[PlayerId],
        amount: u16,
    ) -> Vec<SimulationEvent> {
        if amount == 0 {
            return Vec::new();
        }

        let mut events = Vec::new();
        for target in targets {
            let Some(player) = self.players.get_mut(target) else {
                continue;
            };
            if !player.alive {
                continue;
            }

            let damage = amount.min(player.hit_points);
            player.hit_points = player.hit_points.saturating_sub(damage);
            let defeated = player.hit_points == 0;
            if defeated {
                player.alive = false;
                player.moving = false;
                player.movement_intent = MovementIntent::zero();
                player.statuses.clear();
            }

            events.push(SimulationEvent::DamageApplied {
                attacker,
                target: *target,
                amount: damage,
                remaining_hit_points: player.hit_points,
                defeated,
            });
        }

        events
    }

    pub(super) fn apply_healing_internal(
        &mut self,
        source: PlayerId,
        targets: &[PlayerId],
        amount: u16,
    ) -> Vec<SimulationEvent> {
        if amount == 0 {
            return Vec::new();
        }

        let mut events = Vec::new();
        for target in targets {
            let Some(player) = self.players.get_mut(target) else {
                continue;
            };
            if !player.alive {
                continue;
            }

            let missing = player.max_hit_points.saturating_sub(player.hit_points);
            let healed = amount.min(missing);
            player.hit_points = player.hit_points.saturating_add(healed);
            events.push(SimulationEvent::HealingApplied {
                source,
                target: *target,
                amount: healed,
                resulting_hit_points: player.hit_points,
            });
        }
        events
    }

    pub(super) fn apply_status(
        &mut self,
        source: PlayerId,
        target: PlayerId,
        slot: u8,
        definition: StatusDefinition,
    ) -> Option<SimulationEvent> {
        let player = self.players.get_mut(&target)?;
        if !player.alive {
            return None;
        }

        let mut stacks_after = 1_u8;
        if let Some(existing) = player.statuses.iter_mut().find(|status| {
            status.source == source && status.slot == slot && status.kind == definition.kind
        }) {
            existing.stacks = existing.stacks.saturating_add(1).min(existing.max_stacks);
            existing.remaining_ms = definition.duration_ms;
            existing.tick_progress_ms = 0;
            existing.magnitude = definition.magnitude;
            existing.trigger_duration_ms = definition.trigger_duration_ms;
            stacks_after = existing.stacks;
        } else {
            player.statuses.push(StatusInstance {
                source,
                slot,
                kind: definition.kind,
                stacks: 1,
                remaining_ms: definition.duration_ms,
                tick_interval_ms: definition.tick_interval_ms,
                tick_progress_ms: 0,
                magnitude: definition.magnitude,
                max_stacks: definition.max_stacks,
                trigger_duration_ms: definition.trigger_duration_ms,
            });
        }

        if definition.kind == StatusKind::Chill
            && stacks_after >= definition.max_stacks
            && definition.trigger_duration_ms.is_some()
        {
            let root_duration = definition.trigger_duration_ms.unwrap_or(0);
            self.apply_status(
                source,
                target,
                slot,
                StatusDefinition {
                    kind: StatusKind::Root,
                    duration_ms: root_duration,
                    tick_interval_ms: None,
                    magnitude: 0,
                    max_stacks: 1,
                    trigger_duration_ms: None,
                },
            );
        }

        Some(SimulationEvent::StatusApplied {
            source,
            target,
            slot,
            kind: definition.kind,
            stacks: stacks_after,
            remaining_ms: definition.duration_ms,
        })
    }

    pub(super) fn find_closest_player_near_point(
        &self,
        attacker: PlayerId,
        point: (i16, i16),
        radius: u16,
    ) -> Option<PlayerId> {
        let effective_radius = i32::from(Self::player_overlap_radius(radius));
        let max_distance_sq = effective_radius * effective_radius;
        self.players
            .iter()
            .filter(|(player_id, player)| **player_id != attacker && player.alive)
            .filter_map(|(player_id, player)| {
                let distance_sq = point_distance_sq(point, (player.x, player.y));
                (distance_sq <= max_distance_sq).then_some((*player_id, distance_sq))
            })
            .min_by_key(|(_, distance_sq)| *distance_sq)
            .map(|(player_id, _)| player_id)
    }

    pub(super) fn find_first_player_on_segment(
        &self,
        attacker: PlayerId,
        start: (i16, i16),
        end: (i16, i16),
        radius: u16,
    ) -> Option<PlayerId> {
        let effective_radius = Self::player_overlap_radius(radius);
        let threshold_sq = f32::from(effective_radius) * f32::from(effective_radius);
        self.players
            .iter()
            .filter(|(player_id, player)| **player_id != attacker && player.alive)
            .filter_map(|(player_id, player)| {
                let point = (player.x, player.y);
                let distance_sq = segment_distance_sq(start, end, point);
                (distance_sq <= threshold_sq)
                    .then_some((*player_id, point_distance_sq(start, point)))
            })
            .min_by_key(|(_, distance_sq)| *distance_sq)
            .map(|(player_id, _)| player_id)
    }

    pub(super) fn find_players_in_radius(
        &self,
        center: (i16, i16),
        radius: u16,
        exclude: Option<PlayerId>,
    ) -> Vec<PlayerId> {
        let effective_radius = i32::from(Self::player_overlap_radius(radius));
        let max_distance_sq = effective_radius * effective_radius;
        self.players
            .iter()
            .filter(|(player_id, player)| Some(**player_id) != exclude && player.alive)
            .filter_map(|(player_id, player)| {
                let distance_sq = point_distance_sq(center, (player.x, player.y));
                (distance_sq <= max_distance_sq).then_some(*player_id)
            })
            .collect()
    }
}
