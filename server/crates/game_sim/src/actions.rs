use super::{
    arena_effect_kind, normalize_aim, project_from_aim, resolve_movement, round_f32_to_i32,
    saturating_i16, truncate_line_to_obstacles, ArenaDeployableKind, ArenaEffect,
    ArenaEffectKind, DeployableBehavior, DeployableState, PendingCast, PlayerId, ProjectileState,
    SimPlayerState, SimulationEvent, SimulationWorld, SkillBehavior, StatusKind,
    PLAYER_RADIUS_UNITS,
};

impl SimulationWorld {
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
                            player
                                .statuses
                                .iter()
                                .any(|status| {
                                    matches!(
                                        status.kind,
                                        StatusKind::Stun
                                            | StatusKind::Sleep
                                            | StatusKind::Fear
                                    )
                                }),
                        )
                    });

            let queued_primary = self
                .players
                .get(&player_id)
                .is_some_and(|player| player.queued_primary);
            let queued_cast_slot = self
                .players
                .get(&player_id)
                .and_then(|player| player.queued_cast_slot);

            if queued_primary && !actions_blocked {
                events.extend(self.resolve_primary_attack(player_id, snapshot));
            }
            if let Some(slot) = queued_cast_slot {
                if !casts_blocked {
                    events.extend(self.resolve_cast(player_id, snapshot, slot));
                }
            }

            if let Some(player) = self.players.get_mut(&player_id) {
                player.queued_primary = false;
                player.queued_cast_slot = None;
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
            events.extend(self.apply_payload(attacker, 0, &[target], melee.payload));
        }

        events
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn resolve_cast(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
    ) -> Vec<SimulationEvent> {
        let slot_index = usize::from(slot - 1);
        let Some(player) = self.players.get(&attacker) else {
            return Vec::new();
        };
        if player.slot_cooldown_remaining_ms[slot_index] > 0 {
            return Vec::new();
        }

        let Some(skill) = player.skills[slot_index].clone() else {
            return Vec::new();
        };
        if matches!(skill.behavior, SkillBehavior::Passive { .. }) {
            return Vec::new();
        }
        if self.effective_cast_time_ms(attacker, skill.behavior.cast_time_ms()) > 0 {
            if self.start_pending_cast(attacker, attacker_state, slot, slot_index, skill.behavior) {
                return Vec::new();
            }
            return Vec::new();
        }

        self.execute_skill_behavior(attacker, attacker_state, slot, slot_index, skill.behavior)
    }

    fn start_pending_cast(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
        slot_index: usize,
        behavior: SkillBehavior,
    ) -> bool {
        if attacker_state.moving {
            return false;
        }
        let mana_cost = behavior.mana_cost();
        let cast_time_ms = self.effective_cast_time_ms(attacker, behavior.cast_time_ms());
        let Some(player) = self.players.get_mut(&attacker) else {
            return false;
        };
        if player.active_cast.is_some() || player.mana < mana_cost {
            return false;
        }
        player.active_cast = Some(PendingCast {
            slot,
            slot_index,
            remaining_ms: cast_time_ms,
            total_ms: cast_time_ms,
            just_started: true,
        });
        self.break_stealth(attacker);
        true
    }

    #[allow(clippy::too_many_lines)]
    fn execute_skill_behavior(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
        slot_index: usize,
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
                cooldown_ms,
                mana_cost,
                distance,
                effect,
            ),
            SkillBehavior::Passive { .. } => Vec::new(),
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
                tick_interval_ms,
                effect,
                payload,
            } => self.resolve_aura_cast(
                attacker,
                attacker_state,
                slot,
                slot_index,
                cooldown_ms,
                mana_cost,
                distance,
                radius,
                duration_ms,
                hit_points,
                tick_interval_ms,
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
            attacker,
            attacker_state,
            slot,
            range,
            radius,
            arena_effect_kind(effect),
            payload,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_dash_cast(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
        slot_index: usize,
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
            attacker,
            attacker_state,
            slot,
            distance,
            arena_effect_kind(effect),
            impact_radius,
            payload,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_burst_cast(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
        slot_index: usize,
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
            attacker,
            attacker_state,
            slot,
            range,
            radius,
            arena_effect_kind(effect),
            payload,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_nova_cast(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
        slot_index: usize,
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
                    player.active_cast = None;
                }
                continue;
            }

            let should_cancel = self.players.get(&player_id).is_some_and(|player| {
                player.active_cast.is_some()
                    && player
                        .statuses
                        .iter()
                        .any(|status| {
                            matches!(
                                status.kind,
                                StatusKind::Silence
                                    | StatusKind::Stun
                                    | StatusKind::Sleep
                                    | StatusKind::Fear
                            )
                        })
            });
            if should_cancel {
                if let Some(player) = self.players.get_mut(&player_id) {
                    player.active_cast = None;
                }
                continue;
            }

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
            active_cast.remaining_ms = active_cast.remaining_ms.saturating_sub(delta_ms);
            if active_cast.remaining_ms > 0 {
                continue;
            }
            let Some(skill) = player.skills[active_cast.slot_index].clone() else {
                player.active_cast = None;
                continue;
            };
            let slot = active_cast.slot;
            let slot_index = active_cast.slot_index;
            player.active_cast = None;
            let cast_events = self.execute_skill_behavior(
                player_id,
                attacker_state,
                slot,
                slot_index,
                skill.behavior,
            );
            events.extend(cast_events);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn spawn_projectile(
        &mut self,
        attacker: PlayerId,
        slot: u8,
        attacker_state: SimPlayerState,
        speed: u16,
        range: u16,
        radius: u16,
        effect_kind: ArenaEffectKind,
        payload: game_content::EffectPayload,
    ) -> Vec<SimulationEvent> {
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

    pub(super) fn cast_beam_skill(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
        range: u16,
        radius: u16,
        effect_kind: ArenaEffectKind,
        payload: game_content::EffectPayload,
    ) -> Vec<SimulationEvent> {
        let combat_obstacles = self.combat_obstacles();
        let desired_end = project_from_aim(
            attacker_state.x,
            attacker_state.y,
            attacker_state.aim_x,
            attacker_state.aim_y,
            range,
        );
        let end = truncate_line_to_obstacles(
            (attacker_state.x, attacker_state.y),
            desired_end,
            &combat_obstacles,
        );
        let mut events = vec![SimulationEvent::EffectSpawned {
            effect: ArenaEffect {
                kind: effect_kind,
                owner: attacker,
                slot,
                x: attacker_state.x,
                y: attacker_state.y,
                target_x: end.0,
                target_y: end.1,
                radius,
            },
        }];

        if let Some(target) = self.find_first_target_on_segment(
            attacker,
            (attacker_state.x, attacker_state.y),
            end,
            radius,
            payload.kind == game_content::CombatValueKind::Damage,
        ) {
            events.extend(self.apply_payload(attacker, slot, &[target], payload));
        }
        events
    }

    pub(super) fn cast_dash_skill(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
        distance: u16,
        effect_kind: ArenaEffectKind,
        impact_radius: Option<u16>,
        payload: Option<game_content::EffectPayload>,
    ) -> Vec<SimulationEvent> {
        let combat_obstacles = self.combat_obstacles();
        let desired = project_from_aim(
            attacker_state.x,
            attacker_state.y,
            attacker_state.aim_x,
            attacker_state.aim_y,
            distance,
        );
        let (resolved_x, resolved_y) = resolve_movement(
            attacker_state.x,
            attacker_state.y,
            i32::from(desired.0),
            i32::from(desired.1),
            self.arena_width_units,
            self.arena_height_units,
            &combat_obstacles,
        );
        if let Some(player) = self.players.get_mut(&attacker) {
            player.x = resolved_x;
            player.y = resolved_y;
            player.moving = false;
        }

        let mut events = vec![
            SimulationEvent::EffectSpawned {
                effect: ArenaEffect {
                    kind: effect_kind,
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
        ];

        if let (Some(radius), Some(payload)) = (impact_radius, payload) {
            let targets = self.find_targets_in_radius(
                (resolved_x, resolved_y),
                radius,
                Some(attacker),
                payload.kind == game_content::CombatValueKind::Damage,
            );
            events.extend(self.apply_payload(attacker, slot, &targets, payload));
        }

        events
    }

    pub(super) fn cast_burst_skill(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
        range: u16,
        radius: u16,
        effect_kind: ArenaEffectKind,
        payload: game_content::EffectPayload,
    ) -> Vec<SimulationEvent> {
        let combat_obstacles = self.combat_obstacles();
        let desired_center = project_from_aim(
            attacker_state.x,
            attacker_state.y,
            attacker_state.aim_x,
            attacker_state.aim_y,
            range,
        );
        let center = truncate_line_to_obstacles(
            (attacker_state.x, attacker_state.y),
            desired_center,
            &combat_obstacles,
        );
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
        events.extend(self.apply_payload(attacker, slot, &targets, payload));
        events
    }

    pub(super) fn cast_nova_skill(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
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
        cooldown_ms: u16,
        mana_cost: u16,
        distance: u16,
        effect: game_content::SkillEffectKind,
    ) -> Vec<SimulationEvent> {
        if !self.commit_skill_cast(attacker, slot_index, cooldown_ms, mana_cost) {
            return Vec::new();
        }

        let desired = project_from_aim(
            attacker_state.x,
            attacker_state.y,
            attacker_state.aim_x,
            attacker_state.aim_y,
            distance,
        );
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
        cooldown_ms: u16,
        mana_cost: u16,
        distance: u16,
        radius: u16,
        duration_ms: u16,
        hit_points: Option<u16>,
        tick_interval_ms: u16,
        effect: game_content::SkillEffectKind,
        payload: game_content::EffectPayload,
    ) -> Vec<SimulationEvent> {
        if !self.commit_skill_cast(attacker, slot_index, cooldown_ms, mana_cost) {
            return Vec::new();
        }

        if let Some(hit_points) = hit_points {
            let deployable_id = self.spawn_deployable_entity(
                attacker,
                attacker_state,
                slot,
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
                    effect_kind: arena_effect_kind(effect),
                    payload,
                    anchor_player: None,
                },
            );
            return self.spawn_deployable_events(attacker, slot, attacker_state, deployable_id);
        }

        let deployable_id = self.next_deployable_id();
        self.deployables.push(DeployableState {
            id: deployable_id,
            owner: attacker,
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
                effect_kind: arena_effect_kind(effect),
                payload,
                anchor_player: Some(attacker),
            },
        });
        self.spawn_deployable_events(attacker, slot, attacker_state, deployable_id)
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_deployable_entity(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        _slot: u8,
        distance: u16,
        radius: u16,
        duration_ms: u16,
        hit_points: u16,
        kind: ArenaDeployableKind,
        blocks_movement: bool,
        blocks_projectiles: bool,
        behavior: DeployableBehavior,
    ) -> u32 {
        let desired = project_from_aim(
            attacker_state.x,
            attacker_state.y,
            attacker_state.aim_x,
            attacker_state.aim_y,
            distance,
        );
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
        let Some(deployable) = self.deployables.iter().find(|deployable| deployable.id == deployable_id)
        else {
            return Vec::new();
        };
        vec![
            SimulationEvent::EffectSpawned {
                effect: ArenaEffect {
                    kind: match deployable.kind {
                        ArenaDeployableKind::Summon => ArenaEffectKind::SkillShot,
                        ArenaDeployableKind::Ward | ArenaDeployableKind::Aura => {
                            ArenaEffectKind::Nova
                        }
                        ArenaDeployableKind::Trap | ArenaDeployableKind::Barrier => {
                            ArenaEffectKind::Burst
                        }
                    },
                    owner: attacker,
                    slot,
                    x: attacker_state.x,
                    y: attacker_state.y,
                    target_x: deployable.x,
                    target_y: deployable.y,
                    radius: deployable.radius,
                },
            },
            SimulationEvent::DeployableSpawned {
                deployable_id,
                owner: attacker,
                kind: deployable.kind,
                x: deployable.x,
                y: deployable.y,
                radius: deployable.radius,
            },
        ]
    }
}
