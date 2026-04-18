use super::{
    point_distance_sq, point_distance_units, round_f32_to_i32, saturating_i16, segment_distance_sq,
    travel_distance_units, truncate_line_to_obstacles, ArenaEffect, ArenaEffectKind,
    CombatValueKind, MovementIntent, PlayerId, ProjectileState, SimCastCancelReason, SimMissReason,
    SimPlayer, SimRemovedStatus, SimStatusRemovedReason, SimTargetKind, SimTriggerReason,
    SimulationEvent, SimulationWorld, StatusDefinition, StatusInstance, StatusKind, TargetEntity,
};
use game_content::{ProcResetDefinition, ProcTriggerKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CombatProcEventKind {
    DamageHit,
    HealHit,
    Tick,
}

impl SimulationWorld {
    fn player_overlap_radius(&self, radius: u16) -> u16 {
        radius.saturating_add(self.configuration.player_radius_units)
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

    #[allow(clippy::too_many_lines)]
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

        let mut events = self.apply_direct_payload_values(source, slot, targets, &payload);

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

    fn apply_direct_payload_values(
        &mut self,
        source: PlayerId,
        slot: u8,
        targets: &[TargetEntity],
        payload: &game_content::EffectPayload,
    ) -> Vec<SimulationEvent> {
        match payload.kind {
            CombatValueKind::Damage => {
                let mut events = Vec::new();
                for target in targets {
                    let (amount, critical) = self.resolve_direct_payload_amount(payload);
                    events.extend(self.apply_damage_internal_with_context(
                        source,
                        slot,
                        &[*target],
                        amount,
                        None,
                        None,
                        critical,
                    ));
                }
                events
            }
            CombatValueKind::Heal => {
                let mut events = Vec::new();
                for target in targets {
                    let (amount, critical) = self.resolve_direct_payload_amount(payload);
                    events.extend(self.apply_healing_internal_with_context(
                        source,
                        slot,
                        &[*target],
                        amount,
                        None,
                        None,
                        critical,
                    ));
                }
                events
            }
        }
    }

    fn resolve_direct_payload_amount(
        &mut self,
        payload: &game_content::EffectPayload,
    ) -> (u16, bool) {
        let mut amount = self.roll_u16_inclusive(payload.amount_min(), payload.amount_max());
        let critical = payload.can_crit() && self.roll_bps(payload.crit_chance_bps);
        if critical {
            amount = Self::scale_amount_with_bps(amount, payload.crit_multiplier_bps);
        }
        (amount, critical)
    }

    fn scale_amount_with_bps(amount: u16, scale_bps: u16) -> u16 {
        let scaled = u32::from(amount).saturating_mul(u32::from(scale_bps)) / 10_000;
        u16::try_from(scaled).unwrap_or(u16::MAX)
    }

    fn try_trigger_proc_resets(
        &mut self,
        player_id: PlayerId,
        slot: u8,
        value_kind: CombatValueKind,
        critical: bool,
        periodic: bool,
    ) {
        let event_kind = if periodic {
            CombatProcEventKind::Tick
        } else if value_kind == CombatValueKind::Heal {
            CombatProcEventKind::HealHit
        } else {
            CombatProcEventKind::DamageHit
        };
        let matching = self.collect_matching_proc_resets(player_id, slot, event_kind, critical);
        for (passive_slot_index, proc_reset) in matching {
            self.apply_proc_reset(player_id, passive_slot_index, &proc_reset);
        }
    }

    fn collect_matching_proc_resets(
        &self,
        player_id: PlayerId,
        slot: u8,
        event_kind: CombatProcEventKind,
        critical: bool,
    ) -> Vec<(usize, ProcResetDefinition)> {
        let Some(player) = self.players.get(&player_id) else {
            return Vec::new();
        };
        player
            .skills
            .iter()
            .enumerate()
            .filter_map(|(index, skill)| {
                let Some(game_content::SkillBehavior::Passive {
                    proc_reset: Some(proc_reset),
                    ..
                }) = skill.as_ref().map(|skill| &skill.behavior)
                else {
                    return None;
                };
                if player.proc_cooldown_remaining_ms[index] > 0
                    || !Self::proc_trigger_matches(proc_reset.trigger, event_kind, critical)
                    || !Self::proc_source_matches(player, slot, proc_reset)
                {
                    return None;
                }
                Some((index, proc_reset.clone()))
            })
            .collect()
    }

    fn proc_trigger_matches(
        trigger: ProcTriggerKind,
        event_kind: CombatProcEventKind,
        critical: bool,
    ) -> bool {
        match trigger {
            ProcTriggerKind::Hit => event_kind == CombatProcEventKind::DamageHit,
            ProcTriggerKind::Crit => critical,
            ProcTriggerKind::Heal => event_kind == CombatProcEventKind::HealHit,
            ProcTriggerKind::Tick => event_kind == CombatProcEventKind::Tick,
        }
    }

    fn proc_source_matches(player: &SimPlayer, slot: u8, proc_reset: &ProcResetDefinition) -> bool {
        if proc_reset.source_skill_ids.is_empty() {
            return true;
        }
        Self::player_slot_reference_id(player, slot)
            .is_some_and(|skill_id| proc_reset.source_skill_ids.iter().any(|id| id == skill_id))
    }

    fn player_slot_reference_id(player: &SimPlayer, slot: u8) -> Option<&str> {
        if slot == 0 {
            return Some(player.melee.id.as_str());
        }
        let slot_index = usize::from(slot.saturating_sub(1));
        player
            .skills
            .get(slot_index)
            .and_then(|skill| skill.as_ref())
            .map(|skill| skill.id.as_str())
    }

    fn apply_proc_reset(
        &mut self,
        player_id: PlayerId,
        passive_slot_index: usize,
        proc_reset: &ProcResetDefinition,
    ) {
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        for (skill_index, skill) in player.skills.iter().enumerate() {
            let Some(skill) = skill.as_ref() else {
                continue;
            };
            if proc_reset
                .reset_skill_ids
                .iter()
                .any(|skill_id| skill_id == &skill.id)
            {
                player.slot_cooldown_remaining_ms[skill_index] = 0;
            }
        }
        if !proc_reset.instacast_skill_ids.is_empty() {
            let passive_slot = u8::try_from(passive_slot_index + 1).unwrap_or(0);
            player
                .next_cast_procs
                .retain(|proc_state| proc_state.passive_slot != passive_slot);
            player.next_cast_procs.push(super::NextCastProc {
                passive_slot,
                skill_ids: proc_reset.instacast_skill_ids.clone(),
                costs_mana: proc_reset.instacast_costs_mana,
                starts_cooldown: proc_reset.instacast_starts_cooldown,
            });
        }
        player.proc_cooldown_remaining_ms[passive_slot_index] =
            proc_reset.internal_cooldown_ms.unwrap_or(0);
    }

    #[cfg(test)]
    pub(super) fn apply_damage_internal(
        &mut self,
        attacker: PlayerId,
        targets: &[TargetEntity],
        amount: u16,
    ) -> Vec<SimulationEvent> {
        self.apply_damage_internal_with_context(attacker, 0, targets, amount, None, None, false)
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn apply_damage_internal_with_context(
        &mut self,
        attacker: PlayerId,
        slot: u8,
        targets: &[TargetEntity],
        amount: u16,
        status_kind: Option<StatusKind>,
        trigger: Option<SimTriggerReason>,
        critical: bool,
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
                    self.cancel_toggleable_stealth_auras(player_id);
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
                    let (applied_damage, defeated, remaining_hit_points, proc_should_fire) = {
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
                        (
                            applied_damage,
                            defeated,
                            player.hit_points,
                            applied_damage > 0,
                        )
                    };
                    let periodic_damage = matches!(status_kind, Some(StatusKind::Poison));

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
                        critical,
                        remaining_hit_points,
                        defeated,
                        status_kind,
                        trigger,
                    });
                    if proc_should_fire {
                        self.try_trigger_proc_resets(
                            attacker,
                            slot,
                            CombatValueKind::Damage,
                            critical,
                            periodic_damage,
                        );
                    }
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
                    let (applied_damage, destroyed, remaining_hit_points) = {
                        let execute_threshold_bps =
                            self.configuration.training_dummy.execute_threshold_bps;
                        let applied_damage = amount.min(deployable.hit_points);
                        deployable.hit_points =
                            deployable.hit_points.saturating_sub(applied_damage);
                        let threshold_hit_points = Self::training_dummy_execute_hit_points(
                            deployable.max_hit_points,
                            execute_threshold_bps,
                        );
                        let destroyed = match deployable.behavior {
                            super::DeployableBehavior::TrainingDummyResetFull => {
                                if deployable.hit_points <= threshold_hit_points {
                                    deployable.hit_points = deployable.max_hit_points;
                                }
                                false
                            }
                            super::DeployableBehavior::TrainingDummyExecute => {
                                if deployable.hit_points <= threshold_hit_points {
                                    deployable.hit_points = threshold_hit_points;
                                }
                                false
                            }
                            _ => deployable.hit_points == 0,
                        };
                        (applied_damage, destroyed, deployable.hit_points)
                    };
                    events.push(SimulationEvent::DeployableDamaged {
                        attacker,
                        deployable_id,
                        amount: applied_damage,
                        remaining_hit_points,
                        destroyed,
                    });
                    if applied_damage > 0 {
                        self.try_trigger_proc_resets(
                            attacker,
                            slot,
                            CombatValueKind::Damage,
                            critical,
                            false,
                        );
                    }
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

    pub(super) fn apply_healing_internal_with_context(
        &mut self,
        source: PlayerId,
        slot: u8,
        targets: &[TargetEntity],
        amount: u16,
        status_kind: Option<StatusKind>,
        trigger: Option<SimTriggerReason>,
        critical: bool,
    ) -> Vec<SimulationEvent> {
        if amount == 0 {
            return Vec::new();
        }

        let mut events = Vec::new();
        for target in targets {
            let TargetEntity::Player(player_id) = *target else {
                continue;
            };
            let (healed, resulting_hit_points, proc_should_fire) = {
                let Some(player) = self.players.get_mut(&player_id) else {
                    continue;
                };
                if !player.alive {
                    continue;
                }

                let reduction_bps = player
                    .statuses
                    .iter()
                    .filter(|status| status.kind == StatusKind::HealingReduction)
                    .map(|status| status.magnitude.saturating_mul(u16::from(status.stacks)))
                    .max()
                    .unwrap_or(0)
                    .min(10_000);
                let missing = player.max_hit_points.saturating_sub(player.hit_points);
                let reduced_amount =
                    Self::scale_amount_with_bps(amount, 10_000_u16.saturating_sub(reduction_bps));
                let healed = reduced_amount.min(missing);
                player.hit_points = player.hit_points.saturating_add(healed);
                (healed, player.hit_points, healed > 0)
            };
            let periodic_heal = matches!(status_kind, Some(StatusKind::Hot));
            events.push(SimulationEvent::HealingApplied {
                source,
                target: player_id,
                slot,
                amount: healed,
                critical: critical && healed > 0,
                resulting_hit_points,
                status_kind,
                trigger,
            });
            if proc_should_fire {
                self.try_trigger_proc_resets(
                    source,
                    slot,
                    CombatValueKind::Heal,
                    critical,
                    periodic_heal,
                );
            }
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
        let dr = self.configuration.crowd_control_diminishing_returns;
        let effective_duration_ms =
            Self::apply_crowd_control_dr(player, definition.kind, definition.duration_ms, dr)?;

        let mut stacks_after = 1_u8;
        let mut stack_delta = 1_u8;
        if let Some(existing) = player.statuses.iter_mut().find(|status| {
            status.source == source && status.slot == slot && status.kind == definition.kind
        }) {
            let before = existing.stacks;
            existing.stacks = existing.stacks.saturating_add(1).min(existing.max_stacks);
            existing.remaining_ms = effective_duration_ms;
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
                remaining_ms: effective_duration_ms,
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
            remaining_ms: effective_duration_ms,
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
        let effective_radius = i32::from(self.player_overlap_radius(radius));
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
            let overlap = self.player_overlap_radius(radius);
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
        let effective_radius = i32::from(self.player_overlap_radius(radius));
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
