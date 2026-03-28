use super::{
    point_distance_sq, point_distance_units, round_f32_to_i32, saturating_i16, segment_distance_sq,
    travel_distance_units, truncate_line_to_obstacles, ArenaEffect, ArenaEffectKind,
    CombatValueKind, MovementIntent, PlayerId, ProjectileState, SimCastCancelReason, SimMissReason,
    SimRemovedStatus, SimStatusRemovedReason, SimTargetKind, SimTriggerReason, SimulationEvent,
    SimulationWorld, StatusDefinition, StatusInstance, StatusKind, TargetEntity,
    PLAYER_RADIUS_UNITS,
};

impl SimulationWorld {
    fn player_overlap_radius(radius: u16) -> u16 {
        radius.saturating_add(PLAYER_RADIUS_UNITS)
    }

    fn deployable_overlap_radius(radius: u16, deployable_radius: u16) -> u16 {
        radius.saturating_add(deployable_radius)
    }

    #[cfg(test)]
    pub(super) fn test_deployable_overlap_radius(radius: u16, deployable_radius: u16) -> u16 {
        Self::deployable_overlap_radius(radius, deployable_radius)
    }

    pub(super) fn removed_status(status: &StatusInstance) -> SimRemovedStatus {
        SimRemovedStatus {
            source: status.source,
            slot: status.slot,
            kind: status.kind,
            stacks: status.stacks,
            remaining_ms: status.remaining_ms,
        }
    }

    pub(super) fn advance_projectiles(&mut self, delta_ms: u16, events: &mut Vec<SimulationEvent>) {
        let mut next_projectiles = Vec::new();
        let combat_obstacles = self.combat_obstacles();
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
                &combat_obstacles,
            );
            let target = self.find_first_target_on_segment(
                projectile.owner,
                (projectile.x, projectile.y),
                clipped_end,
                projectile.radius,
                projectile.payload.kind == CombatValueKind::Damage,
            );
            if let Some(target) = target {
                let (target_kind, target_id) = match target {
                    TargetEntity::Player(player_id) => (SimTargetKind::Player, player_id.get()),
                    TargetEntity::Deployable(deployable_id) => {
                        (SimTargetKind::Deployable, deployable_id)
                    }
                };
                events.push(SimulationEvent::ImpactHit {
                    source: projectile.owner,
                    slot: projectile.slot,
                    target_kind,
                    target_id,
                });
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
                    events.push(SimulationEvent::ImpactMiss {
                        source: projectile.owner,
                        slot: projectile.slot,
                        reason: SimMissReason::Blocked,
                    });
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
                } else {
                    events.push(SimulationEvent::ImpactMiss {
                        source: projectile.owner,
                        slot: projectile.slot,
                        reason: SimMissReason::Expired,
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
        targets: &[TargetEntity],
        payload: game_content::EffectPayload,
    ) -> Vec<SimulationEvent> {
        if targets.is_empty() {
            return Vec::new();
        }

        let mut events = match payload.kind {
            CombatValueKind::Damage => self.apply_damage_internal_with_context(
                source,
                slot,
                targets,
                payload.amount,
                None,
                None,
            ),
            CombatValueKind::Heal => self.apply_healing_internal_with_context(
                source,
                slot,
                targets,
                payload.amount,
                None,
                None,
            ),
        };

        if let Some(silence_duration_ms) = payload.interrupt_silence_duration_ms {
            for target in targets {
                let TargetEntity::Player(target_player_id) = *target else {
                    continue;
                };
                if self
                    .players
                    .get(&target_player_id)
                    .is_some_and(|player| player.active_cast.is_some())
                {
                    if let Some(player) = self.players.get_mut(&target_player_id) {
                        if let Some(cast) = player.active_cast.take() {
                            events.push(SimulationEvent::CastCanceled {
                                player_id: target_player_id,
                                slot: cast.slot,
                                reason: SimCastCancelReason::Interrupt,
                            });
                        }
                    }
                    if let Some(event) = self.apply_status(
                        source,
                        target_player_id,
                        slot,
                        StatusDefinition {
                            kind: StatusKind::Silence,
                            duration_ms: silence_duration_ms,
                            tick_interval_ms: None,
                            magnitude: 0,
                            max_stacks: 1,
                            trigger_duration_ms: None,
                            expire_payload: None,
                            dispel_payload: None,
                        },
                    ) {
                        events.push(event);
                    }
                }
            }
        }

        if let Some(dispel) = payload.dispel {
            events.push(SimulationEvent::DispelCast {
                source,
                slot,
                scope: dispel.scope,
                max_statuses: dispel.max_statuses,
            });
            events.extend(self.apply_dispel_internal(source, slot, targets, dispel));
        }

        if let Some(status) = payload.status {
            for target in targets {
                let TargetEntity::Player(target_player_id) = *target else {
                    continue;
                };
                if let Some(event) =
                    self.apply_status(source, target_player_id, slot, status.clone())
                {
                    events.push(event);
                }
            }
        }

        events
    }

    #[cfg(test)]
    pub(super) fn apply_damage_internal(
        &mut self,
        attacker: PlayerId,
        targets: &[TargetEntity],
        amount: u16,
    ) -> Vec<SimulationEvent> {
        self.apply_damage_internal_with_context(attacker, 0, targets, amount, None, None)
    }

    pub(super) fn apply_damage_internal_with_context(
        &mut self,
        attacker: PlayerId,
        slot: u8,
        targets: &[TargetEntity],
        amount: u16,
        status_kind: Option<StatusKind>,
        trigger: Option<SimTriggerReason>,
    ) -> Vec<SimulationEvent> {
        if amount == 0 {
            return Vec::new();
        }

        let mut events = Vec::new();
        let mut destroyed_deployables = Vec::new();
        for target in targets {
            match *target {
                TargetEntity::Player(player_id) => {
                    let Some(player) = self.players.get_mut(&player_id) else {
                        continue;
                    };
                    if !player.alive {
                        continue;
                    }

                    let mut removed_statuses = Vec::new();
                    player.statuses.retain(|status| {
                        let keep =
                            status.kind != StatusKind::Sleep && status.kind != StatusKind::Stealth;
                        if !keep {
                            removed_statuses.push(Self::removed_status(status));
                        }
                        keep
                    });
                    let damage = self.consume_shields(player_id, amount, &mut events);
                    if damage == 0 {
                        for removed in removed_statuses {
                            events.push(SimulationEvent::StatusRemoved {
                                source: removed.source,
                                target: player_id,
                                slot: removed.slot,
                                kind: removed.kind,
                                stacks: removed.stacks,
                                remaining_ms: removed.remaining_ms,
                                reason: SimStatusRemovedReason::DamageBroken,
                            });
                        }
                        continue;
                    }
                    let Some(player) = self.players.get_mut(&player_id) else {
                        continue;
                    };
                    let applied_damage = damage.min(player.hit_points);
                    player.hit_points = player.hit_points.saturating_sub(applied_damage);
                    let defeated = player.hit_points == 0;
                    if defeated {
                        if let Some(cast) = player.active_cast.take() {
                            events.push(SimulationEvent::CastCanceled {
                                player_id,
                                slot: cast.slot,
                                reason: SimCastCancelReason::Defeat,
                            });
                        }
                        for removed in player
                            .statuses
                            .iter()
                            .map(Self::removed_status)
                            .collect::<Vec<_>>()
                        {
                            events.push(SimulationEvent::StatusRemoved {
                                source: removed.source,
                                target: player_id,
                                slot: removed.slot,
                                kind: removed.kind,
                                stacks: removed.stacks,
                                remaining_ms: removed.remaining_ms,
                                reason: SimStatusRemovedReason::Defeat,
                            });
                        }
                        player.alive = false;
                        player.moving = false;
                        player.movement_intent = MovementIntent::zero();
                        player.statuses.clear();
                    }

                    for removed in removed_statuses {
                        events.push(SimulationEvent::StatusRemoved {
                            source: removed.source,
                            target: player_id,
                            slot: removed.slot,
                            kind: removed.kind,
                            stacks: removed.stacks,
                            remaining_ms: removed.remaining_ms,
                            reason: SimStatusRemovedReason::DamageBroken,
                        });
                    }
                    events.push(SimulationEvent::DamageApplied {
                        attacker,
                        target: player_id,
                        slot,
                        amount: applied_damage,
                        remaining_hit_points: player.hit_points,
                        defeated,
                        status_kind,
                        trigger,
                    });
                    if defeated {
                        events.push(SimulationEvent::Defeat {
                            attacker: Some(attacker),
                            target: player_id,
                        });
                    }
                }
                TargetEntity::Deployable(deployable_id) => {
                    let Some(deployable) = self
                        .deployables
                        .iter_mut()
                        .find(|deployable| deployable.id == deployable_id)
                    else {
                        continue;
                    };
                    let applied_damage = amount.min(deployable.hit_points);
                    deployable.hit_points = deployable.hit_points.saturating_sub(applied_damage);
                    let destroyed = deployable.hit_points == 0;
                    events.push(SimulationEvent::DeployableDamaged {
                        attacker,
                        deployable_id,
                        amount: applied_damage,
                        remaining_hit_points: deployable.hit_points,
                        destroyed,
                    });
                    if destroyed {
                        destroyed_deployables.push(deployable_id);
                    }
                }
            }
        }

        if !destroyed_deployables.is_empty() {
            self.deployables
                .retain(|deployable| !destroyed_deployables.contains(&deployable.id));
        }

        events
    }

    fn consume_shields(
        &mut self,
        player_id: PlayerId,
        amount: u16,
        events: &mut Vec<SimulationEvent>,
    ) -> u16 {
        let Some(player) = self.players.get_mut(&player_id) else {
            return amount;
        };
        let mut remaining_damage = amount;
        let mut depleted = Vec::new();
        for shield in player
            .statuses
            .iter_mut()
            .filter(|status| status.kind == StatusKind::Shield && status.shield_remaining > 0)
        {
            if remaining_damage == 0 {
                break;
            }
            let absorbed = remaining_damage.min(shield.shield_remaining);
            shield.shield_remaining = shield.shield_remaining.saturating_sub(absorbed);
            remaining_damage = remaining_damage.saturating_sub(absorbed);
            if shield.magnitude > 0 {
                let stacks = u16::from(shield.stacks);
                let consumed_stacks = shield.shield_remaining.div_ceil(shield.magnitude);
                shield.stacks = u8::try_from(consumed_stacks.min(stacks)).unwrap_or(shield.stacks);
            }
            if shield.shield_remaining == 0 {
                depleted.push(Self::removed_status(shield));
            }
        }
        player
            .statuses
            .retain(|status| status.kind != StatusKind::Shield || status.shield_remaining > 0);
        for shield in depleted {
            events.push(SimulationEvent::StatusRemoved {
                source: shield.source,
                target: player_id,
                slot: shield.slot,
                kind: shield.kind,
                stacks: shield.stacks,
                remaining_ms: shield.remaining_ms,
                reason: SimStatusRemovedReason::ShieldConsumed,
            });
        }
        remaining_damage
    }

    #[cfg(test)]
    pub(super) fn test_consume_shields(&mut self, player_id: PlayerId, amount: u16) -> u16 {
        self.consume_shields(player_id, amount, &mut Vec::new())
    }

    #[cfg(test)]
    pub(super) fn apply_healing_internal(
        &mut self,
        source: PlayerId,
        targets: &[TargetEntity],
        amount: u16,
    ) -> Vec<SimulationEvent> {
        self.apply_healing_internal_with_context(source, 0, targets, amount, None, None)
    }

    pub(super) fn apply_healing_internal_with_context(
        &mut self,
        source: PlayerId,
        slot: u8,
        targets: &[TargetEntity],
        amount: u16,
        status_kind: Option<StatusKind>,
        trigger: Option<SimTriggerReason>,
    ) -> Vec<SimulationEvent> {
        if amount == 0 {
            return Vec::new();
        }

        let mut events = Vec::new();
        for target in targets {
            let TargetEntity::Player(player_id) = *target else {
                continue;
            };
            let Some(player) = self.players.get_mut(&player_id) else {
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
                target: player_id,
                slot,
                amount: healed,
                resulting_hit_points: player.hit_points,
                status_kind,
                trigger,
            });
        }
        events
    }

    fn apply_dispel_internal(
        &mut self,
        source: PlayerId,
        slot: u8,
        targets: &[TargetEntity],
        dispel: game_content::DispelDefinition,
    ) -> Vec<SimulationEvent> {
        let mut pending_payloads = Vec::new();
        let mut dispel_results = Vec::new();
        for target in targets {
            let TargetEntity::Player(player_id) = *target else {
                continue;
            };
            let Some(player) = self.players.get_mut(&player_id) else {
                continue;
            };
            if !player.alive {
                continue;
            }

            let mut eligible = player
                .statuses
                .iter()
                .enumerate()
                .filter(|(_, status)| Self::status_matches_dispel(status.kind, dispel.scope))
                .map(|(index, status)| (index, status.remaining_ms))
                .collect::<Vec<_>>();
            eligible.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
            let remove_indices = eligible
                .into_iter()
                .take(usize::from(dispel.max_statuses))
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            if remove_indices.is_empty() {
                dispel_results.push(SimulationEvent::DispelResult {
                    source,
                    slot,
                    target: player_id,
                    removed_statuses: Vec::new(),
                    triggered_payload_count: 0,
                });
                continue;
            }

            let mut retained = Vec::with_capacity(player.statuses.len());
            let mut removed_statuses = Vec::new();
            let mut triggered_payload_count = 0_u8;
            for (index, status) in std::mem::take(&mut player.statuses).into_iter().enumerate() {
                if remove_indices.contains(&index) {
                    removed_statuses.push(Self::removed_status(&status));
                    if let Some(payload) = status.dispel_payload {
                        triggered_payload_count = triggered_payload_count.saturating_add(1);
                        pending_payloads.push((
                            status.source,
                            player_id,
                            status.slot,
                            status.kind,
                            *payload,
                        ));
                    }
                } else {
                    retained.push(status);
                }
            }
            player.statuses = retained;
            for removed in &removed_statuses {
                dispel_results.push(SimulationEvent::StatusRemoved {
                    source: removed.source,
                    target: player_id,
                    slot: removed.slot,
                    kind: removed.kind,
                    stacks: removed.stacks,
                    remaining_ms: removed.remaining_ms,
                    reason: SimStatusRemovedReason::Dispelled,
                });
            }
            dispel_results.push(SimulationEvent::DispelResult {
                source,
                slot,
                target: player_id,
                removed_statuses,
                triggered_payload_count,
            });
        }

        let mut events = dispel_results;
        for (payload_source, target, slot, status_kind, payload) in pending_payloads {
            events.push(SimulationEvent::TriggerResolved {
                source: payload_source,
                slot,
                status_kind,
                trigger: SimTriggerReason::Dispel,
                target_kind: SimTargetKind::Player,
                target_id: target.get(),
                payload_kind: payload.kind,
                amount: payload.amount,
            });
            events.extend(self.apply_payload(
                payload_source,
                slot,
                &[TargetEntity::Player(target)],
                payload,
            ));
        }
        events
    }

    #[allow(clippy::needless_pass_by_value)]
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
        let mut stack_delta = 1_u8;
        if let Some(existing) = player.statuses.iter_mut().find(|status| {
            status.source == source && status.slot == slot && status.kind == definition.kind
        }) {
            let before = existing.stacks;
            existing.stacks = existing.stacks.saturating_add(1).min(existing.max_stacks);
            existing.remaining_ms = definition.duration_ms;
            existing.tick_progress_ms = 0;
            existing.magnitude = definition.magnitude;
            existing.trigger_duration_ms = definition.trigger_duration_ms;
            existing
                .expire_payload
                .clone_from(&definition.expire_payload);
            existing
                .dispel_payload
                .clone_from(&definition.dispel_payload);
            if definition.kind == StatusKind::Shield {
                existing.shield_remaining = existing
                    .shield_remaining
                    .saturating_add(definition.magnitude);
            }
            stacks_after = existing.stacks;
            stack_delta = existing.stacks.saturating_sub(before).max(1);
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
                shield_remaining: if definition.kind == StatusKind::Shield {
                    definition.magnitude
                } else {
                    0
                },
                expire_payload: definition.expire_payload.clone(),
                dispel_payload: definition.dispel_payload.clone(),
            });
        }

        if definition.kind == StatusKind::Chill
            && stacks_after >= definition.max_stacks
            && definition.trigger_duration_ms.is_some()
        {
            let root_duration = definition.trigger_duration_ms.unwrap_or(0);
            let _ = self.apply_status(
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
                    expire_payload: None,
                    dispel_payload: None,
                },
            );
        }

        Some(SimulationEvent::StatusApplied {
            source,
            target,
            slot,
            kind: definition.kind,
            stacks: stacks_after,
            stack_delta,
            remaining_ms: definition.duration_ms,
        })
    }

    pub(super) fn find_closest_target_near_point(
        &self,
        attacker: PlayerId,
        point: (i16, i16),
        radius: u16,
        include_deployables: bool,
    ) -> Option<TargetEntity> {
        let mut candidates = Vec::new();
        let effective_radius = i32::from(Self::player_overlap_radius(radius));
        let max_distance_sq = effective_radius * effective_radius;
        for (player_id, player) in &self.players {
            if *player_id == attacker || !player.alive {
                continue;
            }
            if !self.can_enemy_target_player(attacker, *player_id) {
                continue;
            }
            let distance_sq = point_distance_sq(point, (player.x, player.y));
            if distance_sq <= max_distance_sq {
                candidates.push((TargetEntity::Player(*player_id), distance_sq));
            }
        }
        if include_deployables {
            for deployable in &self.deployables {
                let effective_radius =
                    i32::from(Self::deployable_overlap_radius(radius, deployable.radius));
                let distance_sq = point_distance_sq(point, (deployable.x, deployable.y));
                if distance_sq <= effective_radius * effective_radius {
                    candidates.push((TargetEntity::Deployable(deployable.id), distance_sq));
                }
            }
        }
        candidates
            .into_iter()
            .min_by_key(|(_, distance_sq)| *distance_sq)
            .map(|(target, _)| target)
    }

    pub(super) fn find_first_target_on_segment(
        &self,
        attacker: PlayerId,
        start: (i16, i16),
        end: (i16, i16),
        radius: u16,
        include_deployables: bool,
    ) -> Option<TargetEntity> {
        let mut candidates = Vec::new();
        let threshold_sq = {
            let overlap = Self::player_overlap_radius(radius);
            f32::from(overlap) * f32::from(overlap)
        };
        for (player_id, player) in &self.players {
            if *player_id == attacker || !player.alive {
                continue;
            }
            if !self.can_enemy_target_player(attacker, *player_id) {
                continue;
            }
            let point = (player.x, player.y);
            let distance_sq = segment_distance_sq(start, end, point);
            if distance_sq <= threshold_sq {
                candidates.push((
                    TargetEntity::Player(*player_id),
                    point_distance_sq(start, point),
                ));
            }
        }
        if include_deployables {
            for deployable in &self.deployables {
                let overlap = Self::deployable_overlap_radius(radius, deployable.radius);
                let distance_sq = segment_distance_sq(start, end, (deployable.x, deployable.y));
                if distance_sq <= f32::from(overlap) * f32::from(overlap) {
                    candidates.push((
                        TargetEntity::Deployable(deployable.id),
                        point_distance_sq(start, (deployable.x, deployable.y)),
                    ));
                }
            }
        }
        candidates
            .into_iter()
            .min_by_key(|(_, distance_sq)| *distance_sq)
            .map(|(target, _)| target)
    }

    pub(super) fn find_targets_in_radius(
        &self,
        center: (i16, i16),
        radius: u16,
        exclude: Option<PlayerId>,
        include_deployables: bool,
    ) -> Vec<TargetEntity> {
        let mut targets = Vec::new();
        let effective_radius = i32::from(Self::player_overlap_radius(radius));
        let max_distance_sq = effective_radius * effective_radius;
        for (player_id, player) in &self.players {
            if Some(*player_id) == exclude || !player.alive {
                continue;
            }
            if let Some(excluding_player) = exclude {
                if !self.can_enemy_target_player(excluding_player, *player_id) {
                    continue;
                }
            }
            let distance_sq = point_distance_sq(center, (player.x, player.y));
            if distance_sq <= max_distance_sq {
                targets.push(TargetEntity::Player(*player_id));
            }
        }
        if include_deployables {
            for deployable in &self.deployables {
                let effective_radius =
                    i32::from(Self::deployable_overlap_radius(radius, deployable.radius));
                let distance_sq = point_distance_sq(center, (deployable.x, deployable.y));
                if distance_sq <= effective_radius * effective_radius {
                    targets.push(TargetEntity::Deployable(deployable.id));
                }
            }
        }
        targets
    }

    #[cfg(test)]
    pub(super) fn find_closest_player_near_point(
        &self,
        attacker: PlayerId,
        point: (i16, i16),
        radius: u16,
    ) -> Option<PlayerId> {
        match self.find_closest_target_near_point(attacker, point, radius, false) {
            Some(TargetEntity::Player(player_id)) => Some(player_id),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(super) fn find_first_player_on_segment(
        &self,
        attacker: PlayerId,
        start: (i16, i16),
        end: (i16, i16),
        radius: u16,
    ) -> Option<PlayerId> {
        match self.find_first_target_on_segment(attacker, start, end, radius, false) {
            Some(TargetEntity::Player(player_id)) => Some(player_id),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(super) fn find_players_in_radius(
        &self,
        center: (i16, i16),
        radius: u16,
        exclude: Option<PlayerId>,
    ) -> Vec<PlayerId> {
        self.find_targets_in_radius(center, radius, exclude, false)
            .into_iter()
            .filter_map(|target| match target {
                TargetEntity::Player(player_id) => Some(player_id),
                TargetEntity::Deployable(_) => None,
            })
            .collect()
    }
}
