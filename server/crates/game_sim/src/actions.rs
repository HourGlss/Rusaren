use super::*;

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
                                matches!(status.kind, StatusKind::Silence | StatusKind::Stun)
                            }),
                            player
                                .statuses
                                .iter()
                                .any(|status| status.kind == StatusKind::Stun),
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

        if let Some(player) = self.players.get_mut(&attacker) {
            player.primary_cooldown_remaining_ms = melee.cooldown_ms;
        }
        if let Some(target) =
            self.find_closest_player_near_point(attacker, target_point, melee.radius)
        {
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
        match skill.behavior {
            SkillBehavior::Projectile {
                cooldown_ms,
                mana_cost,
                speed,
                range,
                radius,
                effect,
                payload,
            } => {
                if !self.consume_skill_mana(attacker, mana_cost) {
                    return Vec::new();
                }
                if let Some(player) = self.players.get_mut(&attacker) {
                    player.slot_cooldown_remaining_ms[slot_index] = cooldown_ms;
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
            SkillBehavior::Beam {
                cooldown_ms,
                mana_cost,
                range,
                radius,
                effect,
                payload,
            } => {
                if !self.consume_skill_mana(attacker, mana_cost) {
                    return Vec::new();
                }
                if let Some(player) = self.players.get_mut(&attacker) {
                    player.slot_cooldown_remaining_ms[slot_index] = cooldown_ms;
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
            SkillBehavior::Dash {
                cooldown_ms,
                mana_cost,
                distance,
                effect,
                impact_radius,
                payload,
            } => {
                if !self.consume_skill_mana(attacker, mana_cost) {
                    return Vec::new();
                }
                if let Some(player) = self.players.get_mut(&attacker) {
                    player.slot_cooldown_remaining_ms[slot_index] = cooldown_ms;
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
            SkillBehavior::Burst {
                cooldown_ms,
                mana_cost,
                range,
                radius,
                effect,
                payload,
            } => {
                if !self.consume_skill_mana(attacker, mana_cost) {
                    return Vec::new();
                }
                if let Some(player) = self.players.get_mut(&attacker) {
                    player.slot_cooldown_remaining_ms[slot_index] = cooldown_ms;
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
            SkillBehavior::Nova {
                cooldown_ms,
                mana_cost,
                radius,
                effect,
                payload,
            } => {
                if !self.consume_skill_mana(attacker, mana_cost) {
                    return Vec::new();
                }
                if let Some(player) = self.players.get_mut(&attacker) {
                    player.slot_cooldown_remaining_ms[slot_index] = cooldown_ms;
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
        }
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
            speed_units_per_second: speed,
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
            &self.obstacles,
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

        if let Some(target) = self.find_first_player_on_segment(
            attacker,
            (attacker_state.x, attacker_state.y),
            end,
            radius,
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
            &self.obstacles,
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
            let targets =
                self.find_players_in_radius((resolved_x, resolved_y), radius, Some(attacker));
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
            &self.obstacles,
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
        let targets = self.find_players_in_radius(center, radius, None);
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
        let targets = self.find_players_in_radius(center, radius, None);
        events.extend(self.apply_payload(attacker, slot, &targets, payload));
        events
    }
}
