use super::{
    adjusted_move_speed, movement_delta, resolve_movement, ArenaEffect, ArenaEffectKind,
    CombatValueKind, DeployableBehavior, MovementIntent, PlayerId, SimCastCancelReason,
    SimMissReason, SimStatusRemovedReason, SimTargetKind, SimTriggerReason, SimulationEvent,
    SimulationWorld, StatusKind, TargetEntity, PLAYER_MANA_REGEN_PER_SECOND,
};

impl SimulationWorld {
    pub(super) fn advance_crowd_control_diminishing_returns(&mut self, delta_ms: u16) {
        for player in self.players.values_mut() {
            for state in [
                &mut player.hard_cc_dr,
                &mut player.movement_cc_dr,
                &mut player.cast_cc_dr,
            ] {
                state.remaining_ms = state.remaining_ms.saturating_sub(delta_ms);
                if state.remaining_ms == 0 {
                    state.stage = 0;
                }
            }
        }
    }

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

    #[allow(clippy::too_many_lines)]
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
            let mut pending_expire_payloads = Vec::new();
            let mut removed_statuses = Vec::new();
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
                                status.slot,
                                status.kind,
                                status.magnitude.saturating_mul(u16::from(status.stacks)),
                            ));
                        }
                    }
                    if status.remaining_ms > 0 {
                        retained_statuses.push(status);
                    } else if let Some(ref payload) = status.expire_payload {
                        let removed = Self::removed_status(&status);
                        removed_statuses.push((removed, true));
                        pending_expire_payloads.push((
                            status.source,
                            status.slot,
                            status.kind,
                            payload.as_ref().clone(),
                        ));
                    } else {
                        removed_statuses.push((Self::removed_status(&status), false));
                    }
                }
                player.statuses = retained_statuses;
            }

            for (removed, _had_expire_payload) in removed_statuses {
                events.push(SimulationEvent::StatusRemoved {
                    source: removed.source,
                    target: player_id,
                    slot: removed.slot,
                    kind: removed.kind,
                    stacks: removed.stacks,
                    remaining_ms: removed.remaining_ms,
                    reason: SimStatusRemovedReason::Expired,
                });
            }

            for (source, slot, kind, amount) in pending_effects {
                match kind {
                    StatusKind::Poison => {
                        events.extend(self.apply_damage_internal_with_context(
                            source,
                            slot,
                            &[TargetEntity::Player(player_id)],
                            amount,
                            Some(kind),
                            None,
                        ));
                    }
                    StatusKind::Hot => {
                        events.extend(self.apply_healing_internal_with_context(
                            source,
                            slot,
                            &[TargetEntity::Player(player_id)],
                            amount,
                            Some(kind),
                            None,
                        ));
                    }
                    StatusKind::Chill
                    | StatusKind::Root
                    | StatusKind::Haste
                    | StatusKind::Silence
                    | StatusKind::Stun
                    | StatusKind::Sleep
                    | StatusKind::Shield
                    | StatusKind::Stealth
                    | StatusKind::Reveal
                    | StatusKind::Fear => {}
                }
            }
            for (source, slot, status_kind, payload) in pending_expire_payloads {
                events.push(SimulationEvent::TriggerResolved {
                    source,
                    slot,
                    status_kind,
                    trigger: SimTriggerReason::Expire,
                    target_kind: SimTargetKind::Player,
                    target_id: player_id.get(),
                    payload_kind: payload.kind,
                    amount: payload.amount,
                });
                events.extend(self.apply_payload(
                    source,
                    slot,
                    &[TargetEntity::Player(player_id)],
                    payload,
                ));
            }
        }
    }

    pub(super) fn move_players(&mut self, delta_ms: u16, events: &mut Vec<SimulationEvent>) {
        let arena_width_units = self.arena_width_units;
        let arena_height_units = self.arena_height_units;
        let arena_width_tiles = self.arena_width_tiles;
        let arena_height_tiles = self.arena_height_tiles;
        let arena_tile_units = self.arena_tile_units;
        let footprint_mask = self.footprint_mask.clone();
        let obstacles = self.combat_obstacles();
        let player_ids = self.players.keys().copied().collect::<Vec<_>>();
        for player_id in player_ids {
            let Some(player_snapshot) = self.players.get(&player_id) else {
                continue;
            };
            if !player_snapshot.alive {
                continue;
            }

            let movement_blocked = player_snapshot.statuses.iter().any(|status| {
                matches!(
                    status.kind,
                    StatusKind::Root | StatusKind::Stun | StatusKind::Sleep
                )
            });
            let feared_source_position = player_snapshot
                .statuses
                .iter()
                .find(|status| status.kind == StatusKind::Fear)
                .and_then(|status| {
                    self.players
                        .get(&status.source)
                        .map(|source_player| (source_player.x, source_player.y))
                });
            let speed_modifier_bps =
                self.effective_move_modifier_bps(player_id, &player_snapshot.statuses);
            let origin_x = player_snapshot.x;
            let origin_y = player_snapshot.y;
            let had_active_cast = player_snapshot.active_cast.is_some();
            let base_intent = player_snapshot.movement_intent;

            let movement = if movement_blocked {
                MovementIntent::zero()
            } else if let Some((source_x, source_y)) = feared_source_position {
                let away_x = (i32::from(origin_x) - i32::from(source_x)).signum();
                let away_y = (i32::from(origin_y) - i32::from(source_y)).signum();
                MovementIntent::new(
                    i8::try_from(away_x).unwrap_or(0),
                    i8::try_from(away_y).unwrap_or(0),
                )
                .unwrap_or(MovementIntent::zero())
            } else {
                base_intent
            };

            if let Some(player) = self.players.get_mut(&player_id) {
                if had_active_cast {
                    if movement == MovementIntent::zero() {
                        player.moving = false;
                        continue;
                    }
                    if let Some(cast) = player.active_cast.take() {
                        events.push(SimulationEvent::CastCanceled {
                            player_id,
                            slot: cast.slot,
                            reason: SimCastCancelReason::Movement,
                        });
                    }
                }
                player.moving = movement != MovementIntent::zero();
                if !player.moving {
                    continue;
                }
            }

            let speed = adjusted_move_speed(delta_ms, speed_modifier_bps);
            if speed == 0 {
                continue;
            }

            let (delta_x, delta_y) = movement_delta(movement, speed);
            let next_x = i32::from(origin_x) + delta_x;
            let next_y = i32::from(origin_y) + delta_y;
            let (resolved_x, resolved_y) = resolve_movement(
                origin_x,
                origin_y,
                next_x,
                next_y,
                arena_width_units,
                arena_height_units,
                arena_width_tiles,
                arena_height_tiles,
                arena_tile_units,
                &footprint_mask,
                &obstacles,
            );

            if let Some(player) = self.players.get_mut(&player_id) {
                if resolved_x != origin_x || resolved_y != origin_y {
                    player.x = resolved_x;
                    player.y = resolved_y;
                    events.push(SimulationEvent::PlayerMoved {
                        player_id,
                        x: player.x,
                        y: player.y,
                    });
                }
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn advance_deployables(&mut self, delta_ms: u16, events: &mut Vec<SimulationEvent>) {
        let mut expired = Vec::new();
        let deployable_ids = self
            .deployables
            .iter()
            .map(|deployable| deployable.id)
            .collect::<Vec<_>>();
        for deployable_id in deployable_ids {
            let Some(index) = self
                .deployables
                .iter()
                .position(|deployable| deployable.id == deployable_id)
            else {
                continue;
            };
            let mut deployable = self.deployables[index].clone();
            let permanent_training_dummy = matches!(
                deployable.behavior,
                DeployableBehavior::TrainingDummyResetFull
                    | DeployableBehavior::TrainingDummyExecute
            );
            let persistent_toggleable_aura = matches!(
                deployable.behavior,
                DeployableBehavior::Aura {
                    toggleable: true,
                    ..
                }
            );
            let persistent_ward = matches!(deployable.behavior, DeployableBehavior::Ward)
                && deployable.remaining_ms == u16::MAX;
            if !permanent_training_dummy && !persistent_toggleable_aura && !persistent_ward {
                deployable.remaining_ms = deployable.remaining_ms.saturating_sub(delta_ms);
            }
            if ((!permanent_training_dummy && !persistent_toggleable_aura && !persistent_ward)
                && deployable.remaining_ms == 0)
                || deployable.hit_points == 0
            {
                events.extend(self.aura_end_events(&deployable));
                expired.push(deployable_id);
                continue;
            }

            match &mut deployable.behavior {
                DeployableBehavior::Summon {
                    range,
                    tick_interval_ms,
                    tick_progress_ms,
                    effect_kind,
                    payload,
                } => {
                    *tick_progress_ms = tick_progress_ms.saturating_add(delta_ms);
                    while *tick_progress_ms >= *tick_interval_ms {
                        *tick_progress_ms = tick_progress_ms.saturating_sub(*tick_interval_ms);
                        if let Some(target) = self.find_closest_target_near_point(
                            deployable.owner,
                            (deployable.x, deployable.y),
                            *range,
                            payload.kind == CombatValueKind::Damage,
                        ) {
                            let (target_x, target_y) = self.target_position(target);
                            events.push(SimulationEvent::EffectSpawned {
                                effect: ArenaEffect {
                                    kind: *effect_kind,
                                    owner: deployable.owner,
                                    slot: 0,
                                    x: deployable.x,
                                    y: deployable.y,
                                    target_x,
                                    target_y,
                                    radius: deployable.radius,
                                },
                            });
                            let (target_kind, target_id) = match target {
                                TargetEntity::Player(player_id) => {
                                    (SimTargetKind::Player, player_id.get())
                                }
                                TargetEntity::Deployable(deployable_id) => {
                                    (SimTargetKind::Deployable, deployable_id)
                                }
                            };
                            events.push(SimulationEvent::ImpactHit {
                                source: deployable.owner,
                                slot: 0,
                                target_kind,
                                target_id,
                            });
                            events.extend(self.apply_payload(
                                deployable.owner,
                                0,
                                &[target],
                                payload.clone(),
                            ));
                        } else {
                            events.push(SimulationEvent::ImpactMiss {
                                source: deployable.owner,
                                slot: 0,
                                reason: SimMissReason::NoTarget,
                            });
                        }
                    }
                }
                DeployableBehavior::Ward
                | DeployableBehavior::Barrier
                | DeployableBehavior::TrainingDummyResetFull
                | DeployableBehavior::TrainingDummyExecute => {}
                DeployableBehavior::Trap { payload } => {
                    if let Some(target) = self.find_enemy_player_near_point(
                        deployable.owner,
                        (deployable.x, deployable.y),
                        deployable.radius,
                    ) {
                        let (target_x, target_y) =
                            self.target_position(TargetEntity::Player(target));
                        events.push(SimulationEvent::EffectSpawned {
                            effect: ArenaEffect {
                                kind: ArenaEffectKind::Burst,
                                owner: deployable.owner,
                                slot: 0,
                                x: deployable.x,
                                y: deployable.y,
                                target_x,
                                target_y,
                                radius: deployable.radius,
                            },
                        });
                        events.push(SimulationEvent::ImpactHit {
                            source: deployable.owner,
                            slot: 0,
                            target_kind: SimTargetKind::Player,
                            target_id: target.get(),
                        });
                        events.extend(self.apply_payload(
                            deployable.owner,
                            0,
                            &[TargetEntity::Player(target)],
                            payload.clone(),
                        ));
                        expired.push(deployable_id);
                    }
                }
                DeployableBehavior::Aura {
                    tick_interval_ms,
                    tick_progress_ms,
                    effect_kind,
                    payload,
                    cast_start_payload: _,
                    cast_end_payload: _,
                    anchor_player,
                    toggleable: _,
                } => {
                    if let Some(anchor_player_id) = *anchor_player {
                        let Some(anchor_state) = self.player_state(anchor_player_id) else {
                            events.extend(self.aura_end_events(&deployable));
                            expired.push(deployable_id);
                            continue;
                        };
                        if !anchor_state.alive {
                            events.extend(self.aura_end_events(&deployable));
                            expired.push(deployable_id);
                            continue;
                        }
                        deployable.x = anchor_state.x;
                        deployable.y = anchor_state.y;
                    }
                    *tick_progress_ms = tick_progress_ms.saturating_add(delta_ms);
                    while *tick_progress_ms >= *tick_interval_ms {
                        *tick_progress_ms = tick_progress_ms.saturating_sub(*tick_interval_ms);
                        events.extend(self.aura_pulse_events(
                            deployable.owner,
                            deployable.slot,
                            (deployable.x, deployable.y),
                            deployable.radius,
                            *effect_kind,
                            payload.clone(),
                            !Self::payload_hides_aura_visuals(payload),
                        ));
                    }
                }
            }

            if let Some(state) = self
                .deployables
                .iter_mut()
                .find(|candidate| candidate.id == deployable.id)
            {
                *state = deployable;
            }
        }

        if !expired.is_empty() {
            self.deployables
                .retain(|deployable| !expired.contains(&deployable.id));
        }
    }

    fn aura_end_events(&mut self, deployable: &super::DeployableState) -> Vec<SimulationEvent> {
        match &deployable.behavior {
            DeployableBehavior::Aura {
                effect_kind,
                cast_end_payload: Some(payload),
                ..
            } => self.aura_pulse_events(
                deployable.owner,
                deployable.slot,
                (deployable.x, deployable.y),
                deployable.radius,
                *effect_kind,
                payload.clone(),
                !Self::payload_hides_aura_visuals(payload),
            ),
            _ => Vec::new(),
        }
    }

    fn target_position(&self, target: TargetEntity) -> (i16, i16) {
        match target {
            TargetEntity::Player(player_id) => self
                .players
                .get(&player_id)
                .map_or((0, 0), |player| (player.x, player.y)),
            TargetEntity::Deployable(deployable_id) => self
                .deployables
                .iter()
                .find(|deployable| deployable.id == deployable_id)
                .map_or((0, 0), |deployable| (deployable.x, deployable.y)),
        }
    }

    #[cfg(test)]
    pub(super) fn test_target_position(&self, target: TargetEntity) -> (i16, i16) {
        self.target_position(target)
    }

    fn find_enemy_player_near_point(
        &self,
        attacker: PlayerId,
        point: (i16, i16),
        radius: u16,
    ) -> Option<PlayerId> {
        self.players
            .iter()
            .filter(|(player_id, player)| **player_id != attacker && player.alive)
            .filter(|(player_id, _)| self.can_enemy_target_player(attacker, **player_id))
            .filter_map(|(player_id, player)| {
                let overlap_radius = i32::from(radius.saturating_add(super::PLAYER_RADIUS_UNITS));
                let distance_sq = super::point_distance_sq(point, (player.x, player.y));
                (distance_sq <= overlap_radius * overlap_radius)
                    .then_some((*player_id, distance_sq))
            })
            .min_by_key(|(_, distance_sq)| *distance_sq)
            .map(|(player_id, _)| player_id)
    }

    #[cfg(test)]
    pub(super) fn test_find_enemy_player_near_point(
        &self,
        attacker: PlayerId,
        point: (i16, i16),
        radius: u16,
    ) -> Option<PlayerId> {
        self.find_enemy_player_near_point(attacker, point, radius)
    }
}
