use super::{
    arena_effect_kind, normalize_aim, project_from_aim, resolve_movement, round_f32_to_i32,
    saturating_i16, truncate_line_to_obstacles, ActiveCastMode, ArenaDeployableKind, ArenaEffect,
    ArenaEffectKind, CombatValueKind, DeployableBehavior, DeployableState, PendingCast, PlayerId,
    ProjectileState, QueuedActions, SimCastCancelReason, SimCastMode, SimMissReason,
    SimPlayerState, SimTargetKind, SimulationEvent, SimulationWorld, SkillBehavior, StatusKind,
    TargetEntity, PLAYER_RADIUS_UNITS,
};

#[derive(Clone, Copy, Debug)]
struct CastExecution {
    attacker: PlayerId,
    attacker_state: SimPlayerState,
    slot: u8,
    self_target: bool,
}

#[derive(Clone, Debug)]
struct AreaEffectExecution {
    range: u16,
    radius: u16,
    effect_kind: ArenaEffectKind,
    payload: game_content::EffectPayload,
}

#[derive(Clone, Debug)]
struct DashExecution {
    distance: u16,
    effect_kind: ArenaEffectKind,
    impact_radius: Option<u16>,
    payload: Option<game_content::EffectPayload>,
}

fn behavior_label(behavior: &SkillBehavior) -> &'static str {
    match behavior {
        SkillBehavior::Projectile { .. } => "projectile",
        SkillBehavior::Beam { .. } => "beam",
        SkillBehavior::Dash { .. } => "dash",
        SkillBehavior::Burst { .. } => "burst",
        SkillBehavior::Nova { .. } => "nova",
        SkillBehavior::Teleport { .. } => "teleport",
        SkillBehavior::Channel { .. } => "channel",
        SkillBehavior::Passive { .. } => "passive",
        SkillBehavior::Summon { .. } => "summon",
        SkillBehavior::Ward { .. } => "ward",
        SkillBehavior::Trap { .. } => "trap",
        SkillBehavior::Barrier { .. } => "barrier",
        SkillBehavior::Aura { .. } => "aura",
    }
}

impl SimulationWorld {
    fn append_target_outcomes(
        events: &mut Vec<SimulationEvent>,
        source: PlayerId,
        slot: u8,
        targets: &[TargetEntity],
    ) {
        if targets.is_empty() {
            events.push(SimulationEvent::ImpactMiss {
                source,
                slot,
                reason: SimMissReason::NoTarget,
            });
            return;
        }

        for target in targets {
            let (target_kind, target_id) = match target {
                TargetEntity::Player(player_id) => (SimTargetKind::Player, player_id.get()),
                TargetEntity::Deployable(deployable_id) => {
                    (SimTargetKind::Deployable, *deployable_id)
                }
            };
            events.push(SimulationEvent::ImpactHit {
                source,
                slot,
                target_kind,
                target_id,
            });
        }
    }

    pub(super) fn resolve_queued_actions(&mut self, events: &mut Vec<SimulationEvent>) {
        let player_ids = self.players.keys().copied().collect::<Vec<_>>();
        for player_id in player_ids {
            let Some(snapshot) = self.player_state(player_id) else {
                continue;
            };
            if !snapshot.alive {
                continue;
            }
            let (casts_blocked, actions_blocked) =
                self.players
                    .get(&player_id)
                    .map_or((false, false), |player| {
                        (
                            player.statuses.iter().any(|status| {
                                matches!(
                                    status.kind,
                                    StatusKind::Silence
                                        | StatusKind::Stun
                                        | StatusKind::Sleep
                                        | StatusKind::Fear
                                )
                            }),
                            player.statuses.iter().any(|status| {
                                matches!(
                                    status.kind,
                                    StatusKind::Stun | StatusKind::Sleep | StatusKind::Fear
                                )
                            }),
                        )
                    });

            let queued_actions = self
                .players
                .get(&player_id)
                .map_or_else(Default::default, |player| player.queued_actions);

            if queued_actions.primary && !actions_blocked {
                events.extend(self.resolve_primary_attack(player_id, snapshot));
            }
            if let Some(slot) = queued_actions.cast_slot {
                if !casts_blocked {
                    events.extend(self.resolve_cast(
                        player_id,
                        snapshot,
                        slot,
                        queued_actions.cast_self_target,
                    ));
                }
            }

            if let Some(player) = self.players.get_mut(&player_id) {
                player.queued_actions = QueuedActions::default();
            }
        }
    }

    pub(super) fn resolve_primary_attack(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
    ) -> Vec<SimulationEvent> {
        let Some(player) = self.players.get(&attacker) else {
            return Vec::new();
        };
        if player.primary_cooldown_remaining_ms > 0 {
            return Vec::new();
        }

        let melee = player.melee.clone();
        let effective_cooldown_ms = self.effective_primary_cooldown_ms(attacker, melee.cooldown_ms);
        let target_point = project_from_aim(
            attacker_state.x,
            attacker_state.y,
            attacker_state.aim_x,
            attacker_state.aim_y,
            melee.range,
        );
        let mut events = vec![SimulationEvent::EffectSpawned {
            effect: ArenaEffect {
                kind: arena_effect_kind(melee.effect),
                owner: attacker,
                slot: 0,
                x: attacker_state.x,
                y: attacker_state.y,
                target_x: target_point.0,
                target_y: target_point.1,
                radius: melee.radius,
            },
        }];

        self.break_stealth(attacker);
        if let Some(player) = self.players.get_mut(&attacker) {
            player.primary_cooldown_remaining_ms = effective_cooldown_ms;
        }
        if let Some(target) = self.find_closest_target_near_point(
            attacker,
            target_point,
            melee.radius,
            melee.payload.kind == game_content::CombatValueKind::Damage,
        ) {
            let (target_kind, target_id) = match target {
                super::TargetEntity::Player(player_id) => (SimTargetKind::Player, player_id.get()),
                super::TargetEntity::Deployable(deployable_id) => {
                    (SimTargetKind::Deployable, deployable_id)
                }
            };
            events.push(SimulationEvent::ImpactHit {
                source: attacker,
                slot: 0,
                target_kind,
                target_id,
            });
            events.extend(self.apply_payload(attacker, 0, &[target], melee.payload));
        } else {
            events.push(SimulationEvent::ImpactMiss {
                source: attacker,
                slot: 0,
                reason: SimMissReason::NoTarget,
            });
        }

        events
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn resolve_cast(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
        self_target: bool,
    ) -> Vec<SimulationEvent> {
        let slot_index = usize::from(slot - 1);
        let Some((cooldown_remaining_ms, skill)) = self.players.get(&attacker).map(|player| {
            (
                player.slot_cooldown_remaining_ms[slot_index],
                player.skills[slot_index].clone(),
            )
        }) else {
            return Vec::new();
        };
        let Some(skill) = skill else {
            return Vec::new();
        };
        let toggle_cancel_on_cooldown = cooldown_remaining_ms > 0
            && matches!(
                &skill.behavior,
                SkillBehavior::Aura {
                    toggleable: true,
                    ..
                }
            )
            && self.active_toggleable_aura_index(attacker, slot).is_some();
        if cooldown_remaining_ms > 0 && !toggle_cancel_on_cooldown {
            return Vec::new();
        }
        if matches!(skill.behavior, SkillBehavior::Passive { .. }) {
            return Vec::new();
        }
        let behavior_name = behavior_label(&skill.behavior);
        if matches!(skill.behavior, SkillBehavior::Channel { .. }) {
            if self.effective_cast_time_ms(attacker, skill.behavior.cast_time_ms()) > 0 {
                if let Some(event) = self.start_pending_cast(
                    attacker,
                    attacker_state,
                    slot,
                    slot_index,
                    self_target,
                    &skill.behavior,
                ) {
                    return vec![event];
                }
                return Vec::new();
            }
            if let Some(event) = self.start_channel(
                attacker,
                attacker_state,
                slot,
                slot_index,
                self_target,
                skill.behavior,
            ) {
                return vec![event];
            }
            return Vec::new();
        }
        if self.effective_cast_time_ms(attacker, skill.behavior.cast_time_ms()) > 0 {
            if let Some(event) = self.start_pending_cast(
                attacker,
                attacker_state,
                slot,
                slot_index,
                self_target,
                &skill.behavior,
            ) {
                return vec![event];
            }
            return Vec::new();
        }

        let mut events = vec![SimulationEvent::CastStarted {
            player_id: attacker,
            slot,
            behavior: behavior_name,
            mode: SimCastMode::Windup,
            total_ms: 0,
        }];
        events.extend(self.execute_skill_behavior(
            attacker,
            attacker_state,
            slot,
            slot_index,
            self_target,
            skill.behavior,
        ));
        events.push(SimulationEvent::CastCompleted {
            player_id: attacker,
            slot,
            behavior: behavior_name,
        });
        events
    }

    fn start_pending_cast(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
        slot_index: usize,
        self_target: bool,
        behavior: &SkillBehavior,
    ) -> Option<SimulationEvent> {
        if attacker_state.moving {
            return None;
        }
        let mana_cost = behavior.mana_cost();
        let cast_time_ms = self.effective_cast_time_ms(attacker, behavior.cast_time_ms());
        let player = self.players.get_mut(&attacker)?;
        if player.active_cast.is_some() || player.mana < mana_cost {
            return None;
        }
        player.active_cast = Some(PendingCast {
            slot,
            slot_index,
            self_target,
            remaining_ms: cast_time_ms,
            total_ms: cast_time_ms,
            just_started: true,
            mode: ActiveCastMode::Windup,
        });
        self.break_stealth(attacker);
        Some(SimulationEvent::CastStarted {
            player_id: attacker,
            slot,
            behavior: behavior_label(behavior),
            mode: SimCastMode::Windup,
            total_ms: cast_time_ms,
        })
    }

    fn start_channel(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
        slot_index: usize,
        self_target: bool,
        behavior: SkillBehavior,
    ) -> Option<SimulationEvent> {
        let SkillBehavior::Channel {
            cooldown_ms,
            mana_cost,
            range,
            radius,
            duration_ms,
            tick_interval_ms,
            effect,
            payload,
            ..
        } = behavior
        else {
            return None;
        };
        if attacker_state.moving {
            return None;
        }
        let player = self.players.get(&attacker)?;
        if player.active_cast.is_some() || player.mana < mana_cost {
            return None;
        }
        if !self.commit_skill_cast(attacker, slot_index, cooldown_ms, mana_cost) {
            return None;
        }
        if let Some(player) = self.players.get_mut(&attacker) {
            player.active_cast = Some(PendingCast {
                slot,
                slot_index,
                self_target,
                remaining_ms: duration_ms,
                total_ms: duration_ms,
                just_started: true,
                mode: ActiveCastMode::Channel {
                    self_target,
                    range,
                    radius,
                    tick_interval_ms,
                    tick_progress_ms: 0,
                    effect_kind: arena_effect_kind(effect),
                    payload,
                },
            });
        }
        Some(SimulationEvent::CastStarted {
            player_id: attacker,
            slot,
            behavior: "channel",
            mode: SimCastMode::Channel,
            total_ms: duration_ms,
        })
    }

    #[cfg(test)]
    #[allow(clippy::needless_pass_by_value)]
    pub(super) fn test_start_pending_cast(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
        slot_index: usize,
        behavior: SkillBehavior,
    ) -> bool {
        self.start_pending_cast(attacker, attacker_state, slot, slot_index, false, &behavior)
            .is_some()
    }

    #[allow(clippy::too_many_lines)]
    fn execute_skill_behavior(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
        slot_index: usize,
        self_target: bool,
        behavior: SkillBehavior,
    ) -> Vec<SimulationEvent> {
        match behavior {
            SkillBehavior::Projectile {
                cooldown_ms,
                cast_time_ms: _,
                mana_cost,
                speed,
                range,
                radius,
                effect,
                payload,
            } => self.resolve_projectile_cast(
                attacker,
                attacker_state,
                slot,
                slot_index,
                self_target,
                cooldown_ms,
                mana_cost,
                speed,
                range,
                radius,
                effect,
                payload,
            ),
            SkillBehavior::Beam {
                cooldown_ms,
                cast_time_ms: _,
                mana_cost,
                range,
                radius,
                effect,
                payload,
            } => self.resolve_beam_cast(
                attacker,
                attacker_state,
                slot,
                slot_index,
                self_target,
                cooldown_ms,
                mana_cost,
                range,
                radius,
                effect,
                payload,
            ),
            SkillBehavior::Dash {
                cooldown_ms,
                cast_time_ms: _,
                mana_cost,
                distance,
                effect,
                impact_radius,
                payload,
            } => self.resolve_dash_cast(
                attacker,
                attacker_state,
                slot,
                slot_index,
                self_target,
                cooldown_ms,
                mana_cost,
                distance,
                effect,
                impact_radius,
                payload,
            ),
            SkillBehavior::Burst {
                cooldown_ms,
                cast_time_ms: _,
                mana_cost,
                range,
                radius,
                effect,
                payload,
            } => self.resolve_burst_cast(
                attacker,
                attacker_state,
                slot,
                slot_index,
                self_target,
                cooldown_ms,
                mana_cost,
                range,
                radius,
                effect,
                payload,
            ),
            SkillBehavior::Nova {
                cooldown_ms,
                cast_time_ms: _,
                mana_cost,
                radius,
                effect,
                payload,
            } => self.resolve_nova_cast(
                attacker,
                attacker_state,
                slot,
                slot_index,
                self_target,
                cooldown_ms,
                mana_cost,
                radius,
                effect,
                payload,
            ),
            SkillBehavior::Teleport {
                cooldown_ms,
                cast_time_ms: _,
                mana_cost,
                distance,
                effect,
            } => self.resolve_teleport_cast(
                attacker,
                attacker_state,
                slot,
                slot_index,
                self_target,
                cooldown_ms,
                mana_cost,
                distance,
                effect,
            ),
            SkillBehavior::Channel { .. } | SkillBehavior::Passive { .. } => Vec::new(),
            SkillBehavior::Summon {
                cooldown_ms,
                cast_time_ms: _,
                mana_cost,
                distance,
                radius,
                duration_ms,
                hit_points,
                range,
                tick_interval_ms,
                effect,
                payload,
            } => self.resolve_summon_cast(
                attacker,
                attacker_state,
                slot,
                slot_index,
                self_target,
                cooldown_ms,
                mana_cost,
                distance,
                radius,
                duration_ms,
                hit_points,
                range,
                tick_interval_ms,
                effect,
                payload,
            ),
            SkillBehavior::Ward {
                cooldown_ms,
                cast_time_ms: _,
                mana_cost,
                distance,
                radius,
                duration_ms,
                hit_points,
                effect,
            } => self.resolve_ward_cast(
                attacker,
                attacker_state,
                slot,
                slot_index,
                self_target,
                cooldown_ms,
                mana_cost,
                distance,
                radius,
                duration_ms,
                hit_points,
                effect,
            ),
            SkillBehavior::Trap {
                cooldown_ms,
                cast_time_ms: _,
                mana_cost,
                distance,
                radius,
                duration_ms,
                hit_points,
                effect,
                payload,
            } => self.resolve_trap_cast(
                attacker,
                attacker_state,
                slot,
                slot_index,
                self_target,
                cooldown_ms,
                mana_cost,
                distance,
                radius,
                duration_ms,
                hit_points,
                effect,
                payload,
            ),
            SkillBehavior::Barrier {
                cooldown_ms,
                cast_time_ms: _,
                mana_cost,
                distance,
                radius,
                duration_ms,
                hit_points,
                effect,
            } => self.resolve_barrier_cast(
                attacker,
                attacker_state,
                slot,
                slot_index,
                self_target,
                cooldown_ms,
                mana_cost,
                distance,
                radius,
                duration_ms,
                hit_points,
                effect,
            ),
            SkillBehavior::Aura {
                cooldown_ms,
                cast_time_ms: _,
                mana_cost,
                distance,
                radius,
                duration_ms,
                hit_points,
                toggleable,
                tick_interval_ms,
                cast_start_payload,
                cast_end_payload,
                effect,
                payload,
            } => self.resolve_aura_cast(
                attacker,
                attacker_state,
                slot,
                slot_index,
                self_target,
                cooldown_ms,
                mana_cost,
                distance,
                radius,
                duration_ms,
                hit_points,
                toggleable,
                tick_interval_ms,
                cast_start_payload,
                cast_end_payload,
                effect,
                payload,
            ),
        }
    }

    fn commit_skill_cast(
        &mut self,
        attacker: PlayerId,
        slot_index: usize,
        cooldown_ms: u16,
        mana_cost: u16,
    ) -> bool {
        if !self.consume_skill_mana(attacker, mana_cost) {
            return false;
        }
        self.break_stealth(attacker);
        let effective_cooldown_ms = self.effective_skill_cooldown_ms(attacker, cooldown_ms);
        if let Some(player) = self.players.get_mut(&attacker) {
            player.slot_cooldown_remaining_ms[slot_index] = effective_cooldown_ms;
        }
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_projectile_cast(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
        slot_index: usize,
        self_target: bool,
        cooldown_ms: u16,
        mana_cost: u16,
        speed: u16,
        range: u16,
        radius: u16,
        effect: game_content::SkillEffectKind,
        payload: game_content::EffectPayload,
    ) -> Vec<SimulationEvent> {
        if !self.commit_skill_cast(attacker, slot_index, cooldown_ms, mana_cost) {
            return Vec::new();
        }

        self.spawn_projectile(
            attacker,
            slot,
            attacker_state,
            self_target,
            speed,
            range,
            radius,
            arena_effect_kind(effect),
            payload,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_beam_cast(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
        slot_index: usize,
        self_target: bool,
        cooldown_ms: u16,
        mana_cost: u16,
        range: u16,
        radius: u16,
        effect: game_content::SkillEffectKind,
        payload: game_content::EffectPayload,
    ) -> Vec<SimulationEvent> {
        if !self.commit_skill_cast(attacker, slot_index, cooldown_ms, mana_cost) {
            return Vec::new();
        }

        self.cast_beam_skill(
            CastExecution {
                attacker,
                attacker_state,
                slot,
                self_target,
            },
            AreaEffectExecution {
                range,
                radius,
                effect_kind: arena_effect_kind(effect),
                payload,
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_dash_cast(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
        slot_index: usize,
        self_target: bool,
        cooldown_ms: u16,
        mana_cost: u16,
        distance: u16,
        effect: game_content::SkillEffectKind,
        impact_radius: Option<u16>,
        payload: Option<game_content::EffectPayload>,
    ) -> Vec<SimulationEvent> {
        if !self.commit_skill_cast(attacker, slot_index, cooldown_ms, mana_cost) {
            return Vec::new();
        }

        self.cast_dash_skill(
            CastExecution {
                attacker,
                attacker_state,
                slot,
                self_target,
            },
            DashExecution {
                distance,
                effect_kind: arena_effect_kind(effect),
                impact_radius,
                payload,
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_burst_cast(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
        slot_index: usize,
        self_target: bool,
        cooldown_ms: u16,
        mana_cost: u16,
        range: u16,
        radius: u16,
        effect: game_content::SkillEffectKind,
        payload: game_content::EffectPayload,
    ) -> Vec<SimulationEvent> {
        if !self.commit_skill_cast(attacker, slot_index, cooldown_ms, mana_cost) {
            return Vec::new();
        }

        self.cast_burst_skill(
            CastExecution {
                attacker,
                attacker_state,
                slot,
                self_target,
            },
            AreaEffectExecution {
                range,
                radius,
                effect_kind: arena_effect_kind(effect),
                payload,
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_nova_cast(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
        slot_index: usize,
        self_target: bool,
        cooldown_ms: u16,
        mana_cost: u16,
        radius: u16,
        effect: game_content::SkillEffectKind,
        payload: game_content::EffectPayload,
    ) -> Vec<SimulationEvent> {
        if !self.commit_skill_cast(attacker, slot_index, cooldown_ms, mana_cost) {
            return Vec::new();
        }

        self.cast_nova_skill(
            attacker,
            attacker_state,
            slot,
            self_target,
            radius,
            arena_effect_kind(effect),
            payload,
        )
    }

    pub(super) fn consume_skill_mana(&mut self, player_id: PlayerId, mana_cost: u16) -> bool {
        if mana_cost == 0 {
            return true;
        }

        let Some(player) = self.players.get_mut(&player_id) else {
            return false;
        };
        if player.mana < mana_cost {
            return false;
        }

        player.mana = player.mana.saturating_sub(mana_cost);
        true
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn advance_active_casts(
        &mut self,
        delta_ms: u16,
        events: &mut Vec<SimulationEvent>,
    ) {
        let player_ids = self.players.keys().copied().collect::<Vec<_>>();
        for player_id in player_ids {
            let Some(attacker_state) = self.player_state(player_id) else {
                continue;
            };
            if !attacker_state.alive {
                if let Some(player) = self.players.get_mut(&player_id) {
                    if let Some(cast) = player.active_cast.take() {
                        events.push(SimulationEvent::CastCanceled {
                            player_id,
                            slot: cast.slot,
                            reason: SimCastCancelReason::Defeat,
                        });
                    }
                }
                continue;
            }

            let should_cancel = self.players.get(&player_id).and_then(|player| {
                player.active_cast.as_ref()?;
                player.statuses.iter().find_map(|status| {
                    matches!(
                        status.kind,
                        StatusKind::Silence
                            | StatusKind::Stun
                            | StatusKind::Sleep
                            | StatusKind::Fear
                    )
                    .then_some(SimCastCancelReason::ControlLoss)
                })
            });
            if let Some(reason) = should_cancel {
                if let Some(player) = self.players.get_mut(&player_id) {
                    if let Some(cast) = player.active_cast.take() {
                        events.push(SimulationEvent::CastCanceled {
                            player_id,
                            slot: cast.slot,
                            reason,
                        });
                    }
                }
                continue;
            }

            let mut completed_skill: Option<(u8, usize, bool, SkillBehavior)> = None;
            let mut channel_ticks: Option<(
                u8,
                bool,
                u16,
                u16,
                ArenaEffectKind,
                game_content::EffectPayload,
                u16,
            )> = None;
            {
                let Some(player) = self.players.get_mut(&player_id) else {
                    continue;
                };
                let Some(active_cast) = player.active_cast.as_mut() else {
                    continue;
                };
                if active_cast.just_started {
                    active_cast.just_started = false;
                    continue;
                }
                match &mut active_cast.mode {
                    ActiveCastMode::Windup => {
                        active_cast.remaining_ms =
                            active_cast.remaining_ms.saturating_sub(delta_ms);
                        if active_cast.remaining_ms > 0 {
                            continue;
                        }
                        let Some(skill) = player.skills[active_cast.slot_index].clone() else {
                            player.active_cast = None;
                            continue;
                        };
                        completed_skill = Some((
                            active_cast.slot,
                            active_cast.slot_index,
                            active_cast.self_target,
                            skill.behavior,
                        ));
                        player.active_cast = None;
                    }
                    ActiveCastMode::Channel {
                        self_target,
                        range,
                        radius,
                        tick_interval_ms,
                        tick_progress_ms,
                        effect_kind,
                        payload,
                    } => {
                        active_cast.remaining_ms =
                            active_cast.remaining_ms.saturating_sub(delta_ms);
                        *tick_progress_ms = tick_progress_ms.saturating_add(delta_ms);
                        let tick_count = *tick_progress_ms / *tick_interval_ms;
                        *tick_progress_ms %= *tick_interval_ms;
                        channel_ticks = Some((
                            active_cast.slot,
                            *self_target,
                            *range,
                            *radius,
                            *effect_kind,
                            payload.clone(),
                            tick_count,
                        ));
                        if active_cast.remaining_ms == 0 {
                            player.active_cast = None;
                        }
                    }
                }
            }

            if let Some((slot, slot_index, self_target, behavior)) = completed_skill {
                if matches!(behavior, SkillBehavior::Channel { .. }) {
                    if let Some(event) = self.start_channel(
                        player_id,
                        attacker_state,
                        slot,
                        slot_index,
                        self_target,
                        behavior,
                    ) {
                        events.push(event);
                    }
                } else {
                    let behavior_name = behavior_label(&behavior);
                    let cast_events = self.execute_skill_behavior(
                        player_id,
                        attacker_state,
                        slot,
                        slot_index,
                        self_target,
                        behavior,
                    );
                    events.extend(cast_events);
                    events.push(SimulationEvent::CastCompleted {
                        player_id,
                        slot,
                        behavior: behavior_name,
                    });
                }
            }

            if let Some((slot, self_target, range, radius, effect_kind, payload, tick_count)) =
                channel_ticks
            {
                for tick_index in 0..tick_count {
                    events.push(SimulationEvent::ChannelTick {
                        player_id,
                        slot,
                        tick_index: tick_index.saturating_add(1),
                        behavior: "channel",
                    });
                    events.extend(self.execute_channel_tick(
                        CastExecution {
                            attacker: player_id,
                            attacker_state,
                            slot,
                            self_target,
                        },
                        AreaEffectExecution {
                            range,
                            radius,
                            effect_kind,
                            payload: payload.clone(),
                        },
                    ));
                }
                let channel_finished = self
                    .players
                    .get(&player_id)
                    .is_none_or(|player| player.active_cast.is_none());
                if channel_finished {
                    events.push(SimulationEvent::CastCompleted {
                        player_id,
                        slot,
                        behavior: "channel",
                    });
                }
            }
        }
    }

    fn execute_channel_tick(
        &mut self,
        cast: CastExecution,
        effect: AreaEffectExecution,
    ) -> Vec<SimulationEvent> {
        let center = if cast.self_target || effect.range == 0 {
            (cast.attacker_state.x, cast.attacker_state.y)
        } else {
            let combat_obstacles = self.combat_obstacles();
            let desired_center = project_from_aim(
                cast.attacker_state.x,
                cast.attacker_state.y,
                cast.attacker_state.aim_x,
                cast.attacker_state.aim_y,
                effect.range,
            );
            truncate_line_to_obstacles(
                (cast.attacker_state.x, cast.attacker_state.y),
                desired_center,
                &combat_obstacles,
            )
        };
        let mut events = vec![SimulationEvent::EffectSpawned {
            effect: ArenaEffect {
                kind: effect.effect_kind,
                owner: cast.attacker,
                slot: cast.slot,
                x: center.0,
                y: center.1,
                target_x: center.0,
                target_y: center.1,
                radius: effect.radius,
            },
        }];
        let targets = self.find_targets_in_radius(
            center,
            effect.radius,
            None,
            effect.payload.kind == game_content::CombatValueKind::Damage,
        );
        if targets.is_empty() {
            events.push(SimulationEvent::ImpactMiss {
                source: cast.attacker,
                slot: cast.slot,
                reason: SimMissReason::NoTarget,
            });
        } else {
            for target in &targets {
                let (target_kind, target_id) = match target {
                    super::TargetEntity::Player(player_id) => {
                        (SimTargetKind::Player, player_id.get())
                    }
                    super::TargetEntity::Deployable(deployable_id) => {
                        (SimTargetKind::Deployable, *deployable_id)
                    }
                };
                events.push(SimulationEvent::ImpactHit {
                    source: cast.attacker,
                    slot: cast.slot,
                    target_kind,
                    target_id,
                });
            }
        }
        events.extend(self.apply_payload(cast.attacker, cast.slot, &targets, effect.payload));
        events
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn spawn_projectile(
        &mut self,
        attacker: PlayerId,
        slot: u8,
        attacker_state: SimPlayerState,
        self_target: bool,
        speed: u16,
        range: u16,
        radius: u16,
        effect_kind: ArenaEffectKind,
        payload: game_content::EffectPayload,
    ) -> Vec<SimulationEvent> {
        if self_target {
            let target = TargetEntity::Player(attacker);
            let mut events = vec![SimulationEvent::EffectSpawned {
                effect: ArenaEffect {
                    kind: effect_kind,
                    owner: attacker,
                    slot,
                    x: attacker_state.x,
                    y: attacker_state.y,
                    target_x: attacker_state.x,
                    target_y: attacker_state.y,
                    radius,
                },
            }];
            Self::append_target_outcomes(&mut events, attacker, slot, &[target]);
            events.extend(self.apply_payload(attacker, slot, &[target], payload));
            return events;
        }
        let direction = normalize_aim(attacker_state.aim_x, attacker_state.aim_y);
        let spawn_distance =
            i16::try_from(i32::from(PLAYER_RADIUS_UNITS) + i32::from(radius)).unwrap_or(i16::MAX);
        let start_x = saturating_i16(
            i32::from(attacker_state.x) + round_f32_to_i32(direction.0 * f32::from(spawn_distance)),
        );
        let start_y = saturating_i16(
            i32::from(attacker_state.y) + round_f32_to_i32(direction.1 * f32::from(spawn_distance)),
        );
        let projectile = ProjectileState {
            owner: attacker,
            slot,
            kind: effect_kind,
            x: start_x,
            y: start_y,
            direction_x: direction.0,
            direction_y: direction.1,
            speed_units_per_second: self.effective_projectile_speed(attacker, speed),
            remaining_range_units: i32::from(range),
            radius,
            payload,
        };
        self.projectiles.push(projectile);
        vec![SimulationEvent::EffectSpawned {
            effect: ArenaEffect {
                kind: effect_kind,
                owner: attacker,
                slot,
                x: start_x,
                y: start_y,
                target_x: start_x,
                target_y: start_y,
                radius,
            },
        }]
    }

    fn cast_beam_skill(
        &mut self,
        cast: CastExecution,
        effect: AreaEffectExecution,
    ) -> Vec<SimulationEvent> {
        if cast.self_target {
            let target = TargetEntity::Player(cast.attacker);
            let mut events = vec![SimulationEvent::EffectSpawned {
                effect: ArenaEffect {
                    kind: effect.effect_kind,
                    owner: cast.attacker,
                    slot: cast.slot,
                    x: cast.attacker_state.x,
                    y: cast.attacker_state.y,
                    target_x: cast.attacker_state.x,
                    target_y: cast.attacker_state.y,
                    radius: effect.radius,
                },
            }];
            Self::append_target_outcomes(&mut events, cast.attacker, cast.slot, &[target]);
            events.extend(self.apply_payload(cast.attacker, cast.slot, &[target], effect.payload));
            return events;
        }
        let combat_obstacles = self.combat_obstacles();
        let desired_end = project_from_aim(
            cast.attacker_state.x,
            cast.attacker_state.y,
            cast.attacker_state.aim_x,
            cast.attacker_state.aim_y,
            effect.range,
        );
        let end = truncate_line_to_obstacles(
            (cast.attacker_state.x, cast.attacker_state.y),
            desired_end,
            &combat_obstacles,
        );
        let mut events = vec![SimulationEvent::EffectSpawned {
            effect: ArenaEffect {
                kind: effect.effect_kind,
                owner: cast.attacker,
                slot: cast.slot,
                x: cast.attacker_state.x,
                y: cast.attacker_state.y,
                target_x: end.0,
                target_y: end.1,
                radius: effect.radius,
            },
        }];

        if let Some(target) = self.find_first_target_on_segment(
            cast.attacker,
            (cast.attacker_state.x, cast.attacker_state.y),
            end,
            effect.radius,
            effect.payload.kind == game_content::CombatValueKind::Damage,
        ) {
            Self::append_target_outcomes(&mut events, cast.attacker, cast.slot, &[target]);
            events.extend(self.apply_payload(cast.attacker, cast.slot, &[target], effect.payload));
        } else {
            events.push(SimulationEvent::ImpactMiss {
                source: cast.attacker,
                slot: cast.slot,
                reason: SimMissReason::NoTarget,
            });
        }
        events
    }

    fn cast_dash_skill(
        &mut self,
        cast: CastExecution,
        dash: DashExecution,
    ) -> Vec<SimulationEvent> {
        let combat_obstacles = self.combat_obstacles();
        let desired = if cast.self_target {
            (cast.attacker_state.x, cast.attacker_state.y)
        } else {
            project_from_aim(
                cast.attacker_state.x,
                cast.attacker_state.y,
                cast.attacker_state.aim_x,
                cast.attacker_state.aim_y,
                dash.distance,
            )
        };
        let (resolved_x, resolved_y) = resolve_movement(
            cast.attacker_state.x,
            cast.attacker_state.y,
            i32::from(desired.0),
            i32::from(desired.1),
            self.arena_width_units,
            self.arena_height_units,
            self.arena_width_tiles,
            self.arena_height_tiles,
            self.arena_tile_units,
            &self.footprint_mask,
            &combat_obstacles,
        );
        if let Some(player) = self.players.get_mut(&cast.attacker) {
            player.x = resolved_x;
            player.y = resolved_y;
            player.moving = false;
        }

        let mut events = vec![
            SimulationEvent::EffectSpawned {
                effect: ArenaEffect {
                    kind: dash.effect_kind,
                    owner: cast.attacker,
                    slot: cast.slot,
                    x: cast.attacker_state.x,
                    y: cast.attacker_state.y,
                    target_x: resolved_x,
                    target_y: resolved_y,
                    radius: PLAYER_RADIUS_UNITS,
                },
            },
            SimulationEvent::PlayerMoved {
                player_id: cast.attacker,
                x: resolved_x,
                y: resolved_y,
            },
        ];

        if let (Some(radius), Some(payload)) = (dash.impact_radius, dash.payload) {
            let targets = self.find_targets_in_radius(
                (resolved_x, resolved_y),
                radius,
                if cast.self_target {
                    None
                } else {
                    Some(cast.attacker)
                },
                payload.kind == game_content::CombatValueKind::Damage,
            );
            Self::append_target_outcomes(&mut events, cast.attacker, cast.slot, &targets);
            events.extend(self.apply_payload(cast.attacker, cast.slot, &targets, payload));
        }

        events
    }

    fn cast_burst_skill(
        &mut self,
        cast: CastExecution,
        effect: AreaEffectExecution,
    ) -> Vec<SimulationEvent> {
        let center = if cast.self_target {
            (cast.attacker_state.x, cast.attacker_state.y)
        } else {
            let combat_obstacles = self.combat_obstacles();
            let desired_center = project_from_aim(
                cast.attacker_state.x,
                cast.attacker_state.y,
                cast.attacker_state.aim_x,
                cast.attacker_state.aim_y,
                effect.range,
            );
            truncate_line_to_obstacles(
                (cast.attacker_state.x, cast.attacker_state.y),
                desired_center,
                &combat_obstacles,
            )
        };
        let mut events = vec![SimulationEvent::EffectSpawned {
            effect: ArenaEffect {
                kind: effect.effect_kind,
                owner: cast.attacker,
                slot: cast.slot,
                x: center.0,
                y: center.1,
                target_x: center.0,
                target_y: center.1,
                radius: effect.radius,
            },
        }];
        let targets = self.find_targets_in_radius(
            center,
            effect.radius,
            None,
            effect.payload.kind == game_content::CombatValueKind::Damage,
        );
        Self::append_target_outcomes(&mut events, cast.attacker, cast.slot, &targets);
        events.extend(self.apply_payload(cast.attacker, cast.slot, &targets, effect.payload));
        events
    }

    pub(super) fn cast_nova_skill(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
        _self_target: bool,
        radius: u16,
        effect_kind: ArenaEffectKind,
        payload: game_content::EffectPayload,
    ) -> Vec<SimulationEvent> {
        let center = (attacker_state.x, attacker_state.y);
        let mut events = vec![SimulationEvent::EffectSpawned {
            effect: ArenaEffect {
                kind: effect_kind,
                owner: attacker,
                slot,
                x: center.0,
                y: center.1,
                target_x: center.0,
                target_y: center.1,
                radius,
            },
        }];
        let targets = self.find_targets_in_radius(
            center,
            radius,
            None,
            payload.kind == game_content::CombatValueKind::Damage,
        );
        Self::append_target_outcomes(&mut events, attacker, slot, &targets);
        events.extend(self.apply_payload(attacker, slot, &targets, payload));
        events
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_teleport_cast(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
        slot_index: usize,
        self_target: bool,
        cooldown_ms: u16,
        mana_cost: u16,
        distance: u16,
        effect: game_content::SkillEffectKind,
    ) -> Vec<SimulationEvent> {
        if !self.commit_skill_cast(attacker, slot_index, cooldown_ms, mana_cost) {
            return Vec::new();
        }

        let desired = if self_target {
            (attacker_state.x, attacker_state.y)
        } else {
            project_from_aim(
                attacker_state.x,
                attacker_state.y,
                attacker_state.aim_x,
                attacker_state.aim_y,
                distance,
            )
        };
        let (resolved_x, resolved_y) = self.resolve_teleport_destination(
            attacker_state.x,
            attacker_state.y,
            desired.0,
            desired.1,
        );
        if let Some(player) = self.players.get_mut(&attacker) {
            player.x = resolved_x;
            player.y = resolved_y;
            player.moving = false;
        }

        vec![
            SimulationEvent::EffectSpawned {
                effect: ArenaEffect {
                    kind: arena_effect_kind(effect),
                    owner: attacker,
                    slot,
                    x: attacker_state.x,
                    y: attacker_state.y,
                    target_x: resolved_x,
                    target_y: resolved_y,
                    radius: PLAYER_RADIUS_UNITS,
                },
            },
            SimulationEvent::PlayerMoved {
                player_id: attacker,
                x: resolved_x,
                y: resolved_y,
            },
        ]
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_summon_cast(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
        slot_index: usize,
        self_target: bool,
        cooldown_ms: u16,
        mana_cost: u16,
        distance: u16,
        radius: u16,
        duration_ms: u16,
        hit_points: u16,
        range: u16,
        tick_interval_ms: u16,
        effect: game_content::SkillEffectKind,
        payload: game_content::EffectPayload,
    ) -> Vec<SimulationEvent> {
        if !self.commit_skill_cast(attacker, slot_index, cooldown_ms, mana_cost) {
            return Vec::new();
        }
        let deployable_id = self.spawn_deployable_entity(
            attacker,
            attacker_state,
            slot,
            self_target,
            distance,
            radius,
            duration_ms,
            hit_points,
            ArenaDeployableKind::Summon,
            false,
            false,
            DeployableBehavior::Summon {
                range,
                tick_interval_ms,
                tick_progress_ms: 0,
                effect_kind: arena_effect_kind(effect),
                payload,
            },
        );
        self.spawn_deployable_events(attacker, slot, attacker_state, deployable_id)
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_ward_cast(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
        slot_index: usize,
        self_target: bool,
        cooldown_ms: u16,
        mana_cost: u16,
        distance: u16,
        radius: u16,
        duration_ms: u16,
        hit_points: u16,
        _effect: game_content::SkillEffectKind,
    ) -> Vec<SimulationEvent> {
        if !self.commit_skill_cast(attacker, slot_index, cooldown_ms, mana_cost) {
            return Vec::new();
        }
        let deployable_id = self.spawn_deployable_entity(
            attacker,
            attacker_state,
            slot,
            self_target,
            distance,
            radius,
            duration_ms,
            hit_points,
            ArenaDeployableKind::Ward,
            false,
            false,
            DeployableBehavior::Ward,
        );
        self.spawn_deployable_events(attacker, slot, attacker_state, deployable_id)
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_trap_cast(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
        slot_index: usize,
        self_target: bool,
        cooldown_ms: u16,
        mana_cost: u16,
        distance: u16,
        radius: u16,
        duration_ms: u16,
        hit_points: u16,
        _effect: game_content::SkillEffectKind,
        payload: game_content::EffectPayload,
    ) -> Vec<SimulationEvent> {
        if !self.commit_skill_cast(attacker, slot_index, cooldown_ms, mana_cost) {
            return Vec::new();
        }
        let deployable_id = self.spawn_deployable_entity(
            attacker,
            attacker_state,
            slot,
            self_target,
            distance,
            radius,
            duration_ms,
            hit_points,
            ArenaDeployableKind::Trap,
            false,
            false,
            DeployableBehavior::Trap { payload },
        );
        self.spawn_deployable_events(attacker, slot, attacker_state, deployable_id)
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_barrier_cast(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
        slot_index: usize,
        self_target: bool,
        cooldown_ms: u16,
        mana_cost: u16,
        distance: u16,
        radius: u16,
        duration_ms: u16,
        hit_points: u16,
        _effect: game_content::SkillEffectKind,
    ) -> Vec<SimulationEvent> {
        if !self.commit_skill_cast(attacker, slot_index, cooldown_ms, mana_cost) {
            return Vec::new();
        }
        let deployable_id = self.spawn_deployable_entity(
            attacker,
            attacker_state,
            slot,
            self_target,
            distance,
            radius,
            duration_ms,
            hit_points,
            ArenaDeployableKind::Barrier,
            true,
            true,
            DeployableBehavior::Barrier,
        );
        self.spawn_deployable_events(attacker, slot, attacker_state, deployable_id)
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_aura_cast(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
        slot_index: usize,
        self_target: bool,
        cooldown_ms: u16,
        mana_cost: u16,
        distance: u16,
        radius: u16,
        duration_ms: u16,
        hit_points: Option<u16>,
        toggleable: bool,
        tick_interval_ms: u16,
        cast_start_payload: Option<game_content::EffectPayload>,
        cast_end_payload: Option<game_content::EffectPayload>,
        effect: game_content::SkillEffectKind,
        payload: game_content::EffectPayload,
    ) -> Vec<SimulationEvent> {
        if let Some(events) =
            self.cancel_existing_toggleable_aura(attacker, slot, toggleable, attacker_state)
        {
            return events;
        }
        if !self.commit_skill_cast(attacker, slot_index, cooldown_ms, mana_cost) {
            return Vec::new();
        }

        let effect_kind = arena_effect_kind(effect);
        let hidden_visuals = Self::aura_visuals_are_hidden(&payload, cast_start_payload.as_ref());
        if let Some(hit_points) = hit_points {
            let deployable_id = self.spawn_deployable_entity(
                attacker,
                attacker_state,
                slot,
                self_target,
                distance,
                radius,
                duration_ms,
                hit_points,
                ArenaDeployableKind::Aura,
                false,
                false,
                DeployableBehavior::Aura {
                    tick_interval_ms,
                    tick_progress_ms: 0,
                    effect_kind,
                    payload,
                    cast_start_payload: cast_start_payload.clone(),
                    cast_end_payload,
                    anchor_player: None,
                    toggleable,
                },
            );
            let mut events = self.spawn_deployable_events_with_visual(
                attacker,
                slot,
                attacker_state,
                deployable_id,
                !hidden_visuals,
            );
            let center = self
                .deployables
                .iter()
                .find(|deployable| deployable.id == deployable_id)
                .map_or((attacker_state.x, attacker_state.y), |deployable| {
                    (deployable.x, deployable.y)
                });
            events.extend(self.aura_cast_start_events(
                attacker,
                slot,
                center,
                radius,
                effect_kind,
                cast_start_payload,
            ));
            return events;
        }

        let deployable_id = self.next_deployable_id();
        self.deployables.push(DeployableState {
            id: deployable_id,
            owner: attacker,
            slot,
            team: attacker_state.team,
            kind: ArenaDeployableKind::Aura,
            x: attacker_state.x,
            y: attacker_state.y,
            radius,
            hit_points: 1,
            max_hit_points: 1,
            remaining_ms: duration_ms,
            blocks_movement: false,
            blocks_projectiles: false,
            behavior: DeployableBehavior::Aura {
                tick_interval_ms,
                tick_progress_ms: 0,
                effect_kind,
                payload,
                cast_start_payload: cast_start_payload.clone(),
                cast_end_payload,
                anchor_player: Some(attacker),
                toggleable,
            },
        });
        let mut events = self.spawn_deployable_events_with_visual(
            attacker,
            slot,
            attacker_state,
            deployable_id,
            !hidden_visuals,
        );
        events.extend(self.aura_cast_start_events(
            attacker,
            slot,
            (attacker_state.x, attacker_state.y),
            radius,
            effect_kind,
            cast_start_payload,
        ));
        events
    }

    fn cancel_existing_toggleable_aura(
        &mut self,
        attacker: PlayerId,
        slot: u8,
        toggleable: bool,
        attacker_state: SimPlayerState,
    ) -> Option<Vec<SimulationEvent>> {
        if !toggleable {
            return None;
        }
        self.active_toggleable_aura_index(attacker, slot)
            .map(|index| self.cancel_toggleable_aura(index, attacker_state))
    }

    fn aura_cast_start_events(
        &mut self,
        attacker: PlayerId,
        slot: u8,
        center: (i16, i16),
        radius: u16,
        effect_kind: ArenaEffectKind,
        cast_start_payload: Option<game_content::EffectPayload>,
    ) -> Vec<SimulationEvent> {
        cast_start_payload.map_or_else(Vec::new, |payload| {
            self.aura_pulse_events(attacker, slot, center, radius, effect_kind, payload, false)
        })
    }

    fn aura_visuals_are_hidden(
        payload: &game_content::EffectPayload,
        cast_start_payload: Option<&game_content::EffectPayload>,
    ) -> bool {
        Self::payload_hides_aura_visuals(payload)
            || cast_start_payload.is_some_and(Self::payload_hides_aura_visuals)
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_deployable_entity(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
        self_target: bool,
        distance: u16,
        radius: u16,
        duration_ms: u16,
        hit_points: u16,
        kind: ArenaDeployableKind,
        blocks_movement: bool,
        blocks_projectiles: bool,
        behavior: DeployableBehavior,
    ) -> u32 {
        let desired = if self_target {
            (attacker_state.x, attacker_state.y)
        } else {
            project_from_aim(
                attacker_state.x,
                attacker_state.y,
                attacker_state.aim_x,
                attacker_state.aim_y,
                distance,
            )
        };
        let (resolved_x, resolved_y) = self.resolve_teleport_destination(
            attacker_state.x,
            attacker_state.y,
            desired.0,
            desired.1,
        );
        let deployable_id = self.next_deployable_id();
        self.deployables.push(DeployableState {
            id: deployable_id,
            owner: attacker,
            slot,
            team: attacker_state.team,
            kind,
            x: resolved_x,
            y: resolved_y,
            radius,
            hit_points,
            max_hit_points: hit_points,
            remaining_ms: duration_ms,
            blocks_movement,
            blocks_projectiles,
            behavior,
        });
        deployable_id
    }

    fn spawn_deployable_events(
        &self,
        attacker: PlayerId,
        slot: u8,
        attacker_state: SimPlayerState,
        deployable_id: u32,
    ) -> Vec<SimulationEvent> {
        self.spawn_deployable_events_with_visual(
            attacker,
            slot,
            attacker_state,
            deployable_id,
            true,
        )
    }

    fn spawn_deployable_events_with_visual(
        &self,
        attacker: PlayerId,
        slot: u8,
        attacker_state: SimPlayerState,
        deployable_id: u32,
        emit_visual: bool,
    ) -> Vec<SimulationEvent> {
        let Some(deployable) = self
            .deployables
            .iter()
            .find(|deployable| deployable.id == deployable_id)
        else {
            return Vec::new();
        };
        let mut events = Vec::new();
        if emit_visual {
            events.push(SimulationEvent::EffectSpawned {
                effect: ArenaEffect {
                    kind: match deployable.kind {
                        ArenaDeployableKind::Summon => ArenaEffectKind::SkillShot,
                        ArenaDeployableKind::Ward | ArenaDeployableKind::Aura => {
                            ArenaEffectKind::Nova
                        }
                        ArenaDeployableKind::Trap | ArenaDeployableKind::Barrier => {
                            ArenaEffectKind::Burst
                        }
                        ArenaDeployableKind::TrainingDummyResetFull
                        | ArenaDeployableKind::TrainingDummyExecute => ArenaEffectKind::Burst,
                    },
                    owner: attacker,
                    slot,
                    x: attacker_state.x,
                    y: attacker_state.y,
                    target_x: deployable.x,
                    target_y: deployable.y,
                    radius: deployable.radius,
                },
            });
        }
        events.push(SimulationEvent::DeployableSpawned {
            deployable_id,
            owner: attacker,
            kind: deployable.kind,
            x: deployable.x,
            y: deployable.y,
            radius: deployable.radius,
        });
        events
    }

    fn active_toggleable_aura_index(&self, owner: PlayerId, slot: u8) -> Option<usize> {
        self.deployables.iter().position(|deployable| {
            deployable.owner == owner
                && deployable.slot == slot
                && matches!(
                    deployable.behavior,
                    DeployableBehavior::Aura {
                        toggleable: true,
                        ..
                    }
                )
        })
    }

    fn cancel_toggleable_aura(
        &mut self,
        index: usize,
        attacker_state: SimPlayerState,
    ) -> Vec<SimulationEvent> {
        let Some(deployable) = self.deployables.get(index).cloned() else {
            return Vec::new();
        };
        self.deployables.remove(index);
        if Self::deployable_tracks_toggleable_stealth(&deployable) {
            if let Some(player) = self.players.get_mut(&deployable.owner) {
                player.statuses.retain(|status| {
                    !(status.source == deployable.owner
                        && status.slot == deployable.slot
                        && status.kind == StatusKind::Stealth)
                });
            }
        }
        match deployable.behavior {
            DeployableBehavior::Aura {
                effect_kind,
                cast_end_payload: Some(payload),
                ..
            } => {
                let emit_visual = !Self::payload_hides_aura_visuals(&payload);
                self.aura_pulse_events(
                    deployable.owner,
                    deployable.slot,
                    (attacker_state.x, attacker_state.y),
                    deployable.radius,
                    effect_kind,
                    payload,
                    emit_visual,
                )
            }
            _ => Vec::new(),
        }
    }

    pub(super) fn aura_pulse_events(
        &mut self,
        owner: PlayerId,
        slot: u8,
        center: (i16, i16),
        radius: u16,
        effect_kind: ArenaEffectKind,
        payload: game_content::EffectPayload,
        emit_visual: bool,
    ) -> Vec<SimulationEvent> {
        let mut events = Vec::new();
        if emit_visual {
            events.push(SimulationEvent::EffectSpawned {
                effect: ArenaEffect {
                    kind: effect_kind,
                    owner,
                    slot,
                    x: center.0,
                    y: center.1,
                    target_x: center.0,
                    target_y: center.1,
                    radius,
                },
            });
        }
        let targets = self.find_targets_in_radius(
            center,
            radius,
            None,
            payload.kind == CombatValueKind::Damage,
        );
        if targets.is_empty() {
            events.push(SimulationEvent::ImpactMiss {
                source: owner,
                slot,
                reason: SimMissReason::NoTarget,
            });
        } else {
            for target in &targets {
                let (target_kind, target_id) = match target {
                    TargetEntity::Player(player_id) => (SimTargetKind::Player, player_id.get()),
                    TargetEntity::Deployable(deployable_id) => {
                        (SimTargetKind::Deployable, *deployable_id)
                    }
                };
                events.push(SimulationEvent::ImpactHit {
                    source: owner,
                    slot,
                    target_kind,
                    target_id,
                });
            }
        }
        events.extend(self.apply_payload(owner, slot, &targets, payload));
        events
    }
}
