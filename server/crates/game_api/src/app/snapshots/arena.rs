use std::fmt::Write as _;

use game_content::{
    ArenaMapDefinition, CombatValueKind, DispelScope, EffectPayload, GameContent, SkillBehavior,
    SkillDefinition, StatusDefinition, StatusKind,
};
use game_domain::{MatchId, PlayerId, TeamAssignment, TeamSide};
use game_match::{MatchPhase, MatchSession};
use game_net::{
    ArenaDeployableKind, ArenaDeployableSnapshot, ArenaEffectKind, ArenaEffectSnapshot,
    ArenaMatchPhase, ArenaObstacleKind, ArenaObstacleSnapshot, ArenaPlayerSnapshot,
    ArenaProjectileSnapshot, ArenaStatusKind, ArenaStatusSnapshot, SkillCatalogEntry,
    TrainingMetricsSnapshot,
};
use game_sim::{ArenaDeployable, ArenaEffect, ArenaObstacle, SimulationEvent, SimulationWorld};

use super::super::{MatchRuntime, ServerApp, TrainingRuntime};

impl ServerApp {
    pub(in super::super) fn arena_obstacle_snapshot(
        obstacle: &ArenaObstacle,
    ) -> ArenaObstacleSnapshot {
        ArenaObstacleSnapshot {
            kind: match obstacle.kind {
                game_sim::ArenaObstacleKind::Pillar => ArenaObstacleKind::Pillar,
                game_sim::ArenaObstacleKind::Shrub => ArenaObstacleKind::Shrub,
                game_sim::ArenaObstacleKind::Barrier => ArenaObstacleKind::Barrier,
            },
            center_x: obstacle.center_x,
            center_y: obstacle.center_y,
            half_width: obstacle.half_width,
            half_height: obstacle.half_height,
        }
    }

    pub(in super::super) fn arena_obstacles_snapshot(
        obstacles: &[ArenaObstacle],
        _map: &ArenaMapDefinition,
        _explored_tiles: &[u8],
    ) -> Vec<ArenaObstacleSnapshot> {
        obstacles
            .iter()
            .map(Self::arena_obstacle_snapshot)
            .collect()
    }

    pub(in super::super) fn arena_player_snapshot(
        assignment: &TeamAssignment,
        state: game_sim::SimPlayerState,
        unlocked_skill_slots: u8,
        equipped_skill_trees: [Option<game_domain::SkillTree>; 5],
        statuses: Vec<game_sim::SimStatusState>,
    ) -> ArenaPlayerSnapshot {
        ArenaPlayerSnapshot {
            player_id: assignment.player_id,
            player_name: assignment.player_name.clone(),
            team: assignment.team,
            x: state.x,
            y: state.y,
            aim_x: state.aim_x,
            aim_y: state.aim_y,
            hit_points: state.hit_points,
            max_hit_points: state.max_hit_points,
            mana: state.mana,
            max_mana: state.max_mana,
            alive: state.alive,
            unlocked_skill_slots,
            primary_cooldown_remaining_ms: state.primary_cooldown_remaining_ms,
            primary_cooldown_total_ms: state.primary_cooldown_total_ms,
            slot_cooldown_remaining_ms: state.slot_cooldown_remaining_ms,
            slot_cooldown_total_ms: state.slot_cooldown_total_ms,
            equipped_skill_trees,
            current_cast_slot: state.current_cast_slot,
            current_cast_remaining_ms: state.current_cast_remaining_ms,
            current_cast_total_ms: state.current_cast_total_ms,
            active_statuses: statuses
                .into_iter()
                .map(Self::arena_status_snapshot)
                .collect(),
        }
    }

    fn arena_match_player_snapshot(
        runtime: &MatchRuntime,
        assignment: &TeamAssignment,
        state: game_sim::SimPlayerState,
        unlocked_skill_slots: u8,
    ) -> ArenaPlayerSnapshot {
        Self::arena_player_snapshot(
            assignment,
            state,
            unlocked_skill_slots,
            std::array::from_fn(|index| {
                runtime
                    .session
                    .equipped_choice(assignment.player_id, u8::try_from(index + 1).ok()?)
                    .map(|choice| choice.tree)
            }),
            runtime
                .world
                .statuses_for(assignment.player_id)
                .unwrap_or_default(),
        )
    }

    pub(in super::super) fn arena_players_snapshot(
        runtime: &MatchRuntime,
        viewer_id: PlayerId,
        map: &ArenaMapDefinition,
        visible_tiles: &[u8],
    ) -> Vec<ArenaPlayerSnapshot> {
        let unlocked_skill_slots = runtime.session.current_round().get();
        let viewer_team = runtime
            .world
            .player_state(viewer_id)
            .map(|state| state.team);
        runtime
            .roster
            .iter()
            .filter_map(|assignment| {
                runtime
                    .world
                    .player_state(assignment.player_id)
                    .filter(|state| {
                        let visible = assignment.player_id == viewer_id
                            || Self::mask_contains_point(map, visible_tiles, state.x, state.y);
                        visible
                            && !Self::player_hidden_from_viewer(
                                &runtime.world,
                                viewer_id,
                                viewer_team,
                                assignment.player_id,
                            )
                    })
                    .map(|state| {
                        Self::arena_match_player_snapshot(
                            runtime,
                            assignment,
                            state,
                            unlocked_skill_slots,
                        )
                    })
            })
            .collect()
    }

    pub(in super::super) fn arena_training_players_snapshot(
        runtime: &TrainingRuntime,
        viewer_id: PlayerId,
        map: &ArenaMapDefinition,
        visible_tiles: &[u8],
    ) -> Vec<ArenaPlayerSnapshot> {
        runtime
            .world
            .player_state(runtime.participant.player_id)
            .filter(|state| {
                runtime.participant.player_id == viewer_id
                    || Self::mask_contains_point(map, visible_tiles, state.x, state.y)
            })
            .map(|state| {
                Self::arena_player_snapshot(
                    &runtime.participant,
                    state,
                    5,
                    std::array::from_fn(|index| {
                        runtime.loadout[index]
                            .as_ref()
                            .map(|choice| choice.tree.clone())
                    }),
                    runtime
                        .world
                        .statuses_for(runtime.participant.player_id)
                        .unwrap_or_default(),
                )
            })
            .into_iter()
            .collect()
    }

    fn player_hidden_from_viewer(
        world: &SimulationWorld,
        viewer_id: PlayerId,
        viewer_team: Option<TeamSide>,
        target_id: PlayerId,
    ) -> bool {
        if viewer_id == target_id {
            return false;
        }
        let Some(target_state) = world.player_state(target_id) else {
            return true;
        };
        if viewer_team.is_some_and(|team| team == target_state.team) {
            return false;
        }
        let statuses = world.statuses_for(target_id).unwrap_or_default();
        let stealthed = statuses
            .iter()
            .any(|status| status.kind == game_content::StatusKind::Stealth);
        let revealed = statuses
            .iter()
            .any(|status| status.kind == game_content::StatusKind::Reveal);
        stealthed && !revealed
    }

    pub(in super::super) fn arena_deployable_snapshot(
        deployable: ArenaDeployable,
    ) -> ArenaDeployableSnapshot {
        ArenaDeployableSnapshot {
            id: deployable.id,
            owner: deployable.owner,
            team: deployable.team,
            kind: match deployable.kind {
                game_sim::ArenaDeployableKind::Summon => ArenaDeployableKind::Summon,
                game_sim::ArenaDeployableKind::Ward => ArenaDeployableKind::Ward,
                game_sim::ArenaDeployableKind::Trap => ArenaDeployableKind::Trap,
                game_sim::ArenaDeployableKind::Barrier => ArenaDeployableKind::Barrier,
                game_sim::ArenaDeployableKind::Aura => ArenaDeployableKind::Aura,
                game_sim::ArenaDeployableKind::TrainingDummyResetFull => {
                    ArenaDeployableKind::TrainingDummyResetFull
                }
                game_sim::ArenaDeployableKind::TrainingDummyExecute => {
                    ArenaDeployableKind::TrainingDummyExecute
                }
            },
            x: deployable.x,
            y: deployable.y,
            radius: deployable.radius,
            hit_points: deployable.hit_points,
            max_hit_points: deployable.max_hit_points,
            remaining_ms: deployable.remaining_ms,
        }
    }

    pub(in super::super) fn arena_deployables_snapshot(
        world: &SimulationWorld,
        viewer_id: PlayerId,
        map: &ArenaMapDefinition,
        visible_tiles: &[u8],
    ) -> Vec<ArenaDeployableSnapshot> {
        world
            .deployables()
            .into_iter()
            .filter(|deployable| {
                deployable.kind != game_sim::ArenaDeployableKind::Aura
                    && (deployable.owner == viewer_id
                        || Self::mask_contains_point(
                            map,
                            visible_tiles,
                            deployable.x,
                            deployable.y,
                        ))
            })
            .map(Self::arena_deployable_snapshot)
            .collect()
    }

    pub(in super::super) fn arena_projectiles_snapshot(
        world: &SimulationWorld,
        viewer_id: PlayerId,
        map: &ArenaMapDefinition,
        visible_tiles: &[u8],
    ) -> Vec<ArenaProjectileSnapshot> {
        world
            .projectiles()
            .into_iter()
            .filter(|projectile| {
                projectile.owner == viewer_id
                    || Self::mask_contains_point(map, visible_tiles, projectile.x, projectile.y)
            })
            .map(|projectile| ArenaProjectileSnapshot {
                owner: projectile.owner,
                slot: projectile.slot,
                kind: match projectile.kind {
                    game_sim::ArenaEffectKind::MeleeSwing => ArenaEffectKind::MeleeSwing,
                    game_sim::ArenaEffectKind::SkillShot => ArenaEffectKind::SkillShot,
                    game_sim::ArenaEffectKind::DashTrail => ArenaEffectKind::DashTrail,
                    game_sim::ArenaEffectKind::Burst => ArenaEffectKind::Burst,
                    game_sim::ArenaEffectKind::Nova => ArenaEffectKind::Nova,
                    game_sim::ArenaEffectKind::Beam => ArenaEffectKind::Beam,
                    game_sim::ArenaEffectKind::HitSpark => ArenaEffectKind::HitSpark,
                    game_sim::ArenaEffectKind::Footstep
                    | game_sim::ArenaEffectKind::BrushRustle
                    | game_sim::ArenaEffectKind::StealthFootstep => {
                        unreachable!("movement audio effects must not appear in projectile state")
                    }
                },
                x: projectile.x,
                y: projectile.y,
                radius: projectile.radius,
            })
            .collect()
    }

    pub(in super::super) fn filter_arena_effects(
        &mut self,
        match_id: MatchId,
        viewer_id: PlayerId,
        effects: &[ArenaEffectSnapshot],
        map: &ArenaMapDefinition,
    ) -> Vec<ArenaEffectSnapshot> {
        let Some(runtime) = self.matches.get_mut(&match_id) else {
            return Vec::new();
        };
        let Some((visible_tiles, _)) = Self::build_visibility_masks(
            &runtime.world,
            &mut runtime.explored_tiles,
            viewer_id,
            map,
        ) else {
            return Vec::new();
        };
        effects
            .iter()
            .filter(|effect| {
                effect.owner == viewer_id
                    || Self::effect_is_audible_without_visibility(runtime, viewer_id, effect)
                    || Self::mask_contains_point(map, &visible_tiles, effect.x, effect.y)
                    || Self::mask_contains_point(
                        map,
                        &visible_tiles,
                        effect.target_x,
                        effect.target_y,
                    )
            })
            .cloned()
            .collect()
    }

    fn effect_is_audible_without_visibility(
        runtime: &MatchRuntime,
        viewer_id: PlayerId,
        effect: &ArenaEffectSnapshot,
    ) -> bool {
        if !matches!(
            effect.kind,
            ArenaEffectKind::Footstep
                | ArenaEffectKind::BrushRustle
                | ArenaEffectKind::StealthFootstep
        ) {
            return false;
        }
        let Some(viewer) = runtime.world.player_state(viewer_id) else {
            return false;
        };
        let delta_x = i32::from(effect.x) - i32::from(viewer.x);
        let delta_y = i32::from(effect.y) - i32::from(viewer.y);
        let hearing_radius = i32::from(effect.radius);
        delta_x.saturating_mul(delta_x) + delta_y.saturating_mul(delta_y)
            <= hearing_radius.saturating_mul(hearing_radius)
    }

    pub(in super::super) fn build_skill_catalog(content: &GameContent) -> Vec<SkillCatalogEntry> {
        content
            .skills()
            .all()
            .map(|skill| SkillCatalogEntry {
                tree: skill.tree.clone(),
                tier: skill.tier,
                skill_id: skill.id.clone(),
                skill_name: skill.name.clone(),
                skill_description: skill.description.clone(),
                skill_summary: build_skill_summary(skill),
                ui_category: skill_ui_category(skill).to_string(),
                audio_cue_id: skill
                    .audio_cue_id
                    .clone()
                    .unwrap_or_else(|| skill.id.clone()),
            })
            .collect()
    }

    pub(in super::super) fn arena_status_snapshot(
        status: game_sim::SimStatusState,
    ) -> ArenaStatusSnapshot {
        ArenaStatusSnapshot {
            source: status.source,
            slot: status.slot,
            kind: match status.kind {
                game_content::StatusKind::Poison => ArenaStatusKind::Poison,
                game_content::StatusKind::Hot => ArenaStatusKind::Hot,
                game_content::StatusKind::Chill => ArenaStatusKind::Chill,
                game_content::StatusKind::Root => ArenaStatusKind::Root,
                game_content::StatusKind::Haste => ArenaStatusKind::Haste,
                game_content::StatusKind::Silence => ArenaStatusKind::Silence,
                game_content::StatusKind::Stun => ArenaStatusKind::Stun,
                game_content::StatusKind::Sleep => ArenaStatusKind::Sleep,
                game_content::StatusKind::Shield => ArenaStatusKind::Shield,
                game_content::StatusKind::Stealth => ArenaStatusKind::Stealth,
                game_content::StatusKind::Reveal => ArenaStatusKind::Reveal,
                game_content::StatusKind::Fear => ArenaStatusKind::Fear,
                game_content::StatusKind::HealingReduction => ArenaStatusKind::HealingReduction,
            },
            stacks: status.stacks,
            remaining_ms: status.remaining_ms,
        }
    }

    pub(in super::super) fn arena_match_phase_snapshot(
        session: &MatchSession,
    ) -> (ArenaMatchPhase, Option<u8>) {
        match session.phase() {
            MatchPhase::SkillPick { seconds_remaining } => {
                (ArenaMatchPhase::SkillPick, Some(*seconds_remaining))
            }
            MatchPhase::PreCombat { seconds_remaining } => {
                (ArenaMatchPhase::PreCombat, Some(*seconds_remaining))
            }
            MatchPhase::Combat => (ArenaMatchPhase::Combat, None),
            MatchPhase::MatchEnd { .. } => (ArenaMatchPhase::MatchEnd, None),
        }
    }

    pub(in super::super) fn training_metrics_snapshot(
        runtime: &TrainingRuntime,
    ) -> TrainingMetricsSnapshot {
        TrainingMetricsSnapshot {
            damage_done: runtime.metrics.damage_done,
            healing_done: runtime.metrics.healing_done,
            elapsed_ms: runtime.metrics.elapsed_ms,
        }
    }

    pub(in super::super) fn collect_defeated_targets(events: &[SimulationEvent]) -> Vec<PlayerId> {
        let mut defeated_targets = Vec::new();
        for event in events {
            if let SimulationEvent::DamageApplied {
                target, defeated, ..
            } = event
            {
                if *defeated && !defeated_targets.contains(target) {
                    defeated_targets.push(*target);
                }
            }
        }
        defeated_targets
    }

    pub(in super::super) fn collect_effect_batch(
        world: &game_sim::SimulationWorld,
        events: &[SimulationEvent],
    ) -> Vec<ArenaEffectSnapshot> {
        events
            .iter()
            .filter_map(|event| match event {
                SimulationEvent::EffectSpawned { effect } => {
                    Some(Self::arena_effect_snapshot(world, effect))
                }
                _ => None,
            })
            .collect()
    }

    pub(in super::super) fn arena_effect_snapshot(
        world: &game_sim::SimulationWorld,
        effect: &ArenaEffect,
    ) -> ArenaEffectSnapshot {
        ArenaEffectSnapshot {
            kind: match effect.kind {
                game_sim::ArenaEffectKind::MeleeSwing => ArenaEffectKind::MeleeSwing,
                game_sim::ArenaEffectKind::SkillShot => ArenaEffectKind::SkillShot,
                game_sim::ArenaEffectKind::DashTrail => ArenaEffectKind::DashTrail,
                game_sim::ArenaEffectKind::Burst => ArenaEffectKind::Burst,
                game_sim::ArenaEffectKind::Nova => ArenaEffectKind::Nova,
                game_sim::ArenaEffectKind::Beam => ArenaEffectKind::Beam,
                game_sim::ArenaEffectKind::HitSpark => ArenaEffectKind::HitSpark,
                game_sim::ArenaEffectKind::Footstep => ArenaEffectKind::Footstep,
                game_sim::ArenaEffectKind::BrushRustle => ArenaEffectKind::BrushRustle,
                game_sim::ArenaEffectKind::StealthFootstep => ArenaEffectKind::StealthFootstep,
            },
            owner: effect.owner,
            slot: effect.slot,
            x: effect.x,
            y: effect.y,
            target_x: effect.target_x,
            target_y: effect.target_y,
            radius: effect.radius,
            audio_cue_id: world
                .effect_audio_cue_id(effect.owner, effect.slot, effect.kind)
                .unwrap_or_default(),
        }
    }
}

fn build_skill_summary(skill: &SkillDefinition) -> String {
    let mut lines = Vec::new();
    append_skill_header(&mut lines, skill);
    append_behavior_lines(&mut lines, &skill.behavior);
    lines.join("\n")
}

fn append_skill_header(lines: &mut Vec<String>, skill: &SkillDefinition) {
    if let SkillBehavior::Passive { .. } = skill.behavior {
        lines.push(String::from("Passive"));
        return;
    }

    let mut parts = vec![format!(
        "CD {}",
        format_duration_ms(skill.behavior.cooldown_ms())
    )];
    let cast_time_ms = skill.behavior.cast_time_ms();
    if cast_time_ms == 0 {
        parts.push(String::from("Cast instant"));
    } else {
        parts.push(format!("Cast {}", format_duration_ms(cast_time_ms)));
    }
    let mana_cost = skill.behavior.mana_cost();
    if mana_cost > 0 {
        parts.push(format!("Mana {mana_cost}"));
    }
    lines.push(parts.join(" | "));
}

#[allow(clippy::too_many_lines)]
fn append_behavior_lines(lines: &mut Vec<String>, behavior: &SkillBehavior) {
    match behavior {
        SkillBehavior::Projectile {
            range,
            radius,
            speed,
            payload,
            ..
        } => {
            lines.push(format!(
                "Projectile: range {range}, radius {radius}, speed {speed}"
            ));
            append_payload_lines(lines, payload);
        }
        SkillBehavior::Beam {
            range,
            radius,
            payload,
            ..
        } => {
            lines.push(format!("Beam: range {range}, radius {radius}"));
            append_payload_lines(lines, payload);
        }
        SkillBehavior::Dash {
            distance,
            impact_radius,
            payload,
            ..
        } => {
            let mut line = format!("Dash: distance {distance}");
            if let Some(radius) = impact_radius {
                let _ = write!(line, ", impact radius {radius}");
            }
            lines.push(line);
            if let Some(payload) = payload {
                append_payload_lines(lines, payload);
            }
        }
        SkillBehavior::Burst {
            range,
            radius,
            payload,
            ..
        } => {
            lines.push(format!("Burst: cast range {range}, radius {radius}"));
            append_payload_lines(lines, payload);
        }
        SkillBehavior::Nova {
            radius, payload, ..
        } => {
            lines.push(format!("Nova: radius {radius}"));
            append_payload_lines(lines, payload);
        }
        SkillBehavior::Teleport { distance, .. } => {
            lines.push(format!(
                "Teleport: up to {distance}, ignores walls, lands on the nearest valid point"
            ));
        }
        SkillBehavior::Channel {
            range,
            radius,
            duration_ms,
            tick_interval_ms,
            payload,
            ..
        } => {
            let mut line = format!(
                "Channel: radius {radius}, lasts {}, ticks every {}",
                format_duration_ms(*duration_ms),
                format_duration_ms(*tick_interval_ms)
            );
            if *range > 0 {
                let _ = write!(line, ", target range {range}");
            }
            lines.push(line);
            append_payload_lines(lines, payload);
        }
        SkillBehavior::Passive {
            player_speed_bps,
            projectile_speed_bps,
            cooldown_bps,
            cast_time_bps,
            proc_reset,
        } => {
            let mut parts = Vec::new();
            if *player_speed_bps > 0 {
                parts.push(format!("move +{}", format_bps_percent(*player_speed_bps)));
            }
            if *projectile_speed_bps > 0 {
                parts.push(format!(
                    "projectiles +{}",
                    format_bps_percent(*projectile_speed_bps)
                ));
            }
            if *cooldown_bps > 0 {
                parts.push(format!("cooldowns -{}", format_bps_percent(*cooldown_bps)));
            }
            if *cast_time_bps > 0 {
                parts.push(format!("cast time -{}", format_bps_percent(*cast_time_bps)));
            }
            if let Some(proc_reset) = proc_reset {
                let trigger = match proc_reset.trigger {
                    game_content::ProcTriggerKind::Hit => "proc on hit",
                    game_content::ProcTriggerKind::Crit => "proc on crit",
                    game_content::ProcTriggerKind::Heal => "proc on heal",
                    game_content::ProcTriggerKind::Tick => "proc on tick",
                };
                parts.push(String::from(trigger));
            }
            lines.push(if parts.is_empty() {
                String::from("No passive modifiers")
            } else {
                parts.join(" | ")
            });
        }
        SkillBehavior::Summon {
            distance,
            radius,
            duration_ms,
            hit_points,
            range,
            tick_interval_ms,
            payload,
            ..
        } => {
            lines.push(format!(
                "Summon: place {distance} away, radius {radius}, {hit_points} HP, lasts {}, attacks {range} range every {}",
                format_duration_ms(*duration_ms),
                format_duration_ms(*tick_interval_ms)
            ));
            append_payload_lines(lines, payload);
        }
        SkillBehavior::Ward {
            distance,
            radius,
            duration_ms,
            hit_points,
            ..
        } => {
            if *duration_ms == 0 {
                lines.push(format!(
                    "Ward: place {distance} away, vision radius {radius}, {hit_points} HP, lasts until killed"
                ));
            } else {
                lines.push(format!(
                    "Ward: place {distance} away, vision radius {radius}, {hit_points} HP, lasts {}",
                    format_duration_ms(*duration_ms)
                ));
            }
        }
        SkillBehavior::Trap {
            distance,
            radius,
            duration_ms,
            hit_points,
            payload,
            ..
        } => {
            lines.push(format!(
                "Trap: place {distance} away, trigger radius {radius}, {hit_points} HP, lasts {}",
                format_duration_ms(*duration_ms)
            ));
            append_payload_lines(lines, payload);
        }
        SkillBehavior::Barrier {
            distance,
            radius,
            duration_ms,
            hit_points,
            ..
        } => {
            lines.push(format!(
                "Barrier: place {distance} away, radius {radius}, {hit_points} HP, lasts {}",
                format_duration_ms(*duration_ms)
            ));
        }
        SkillBehavior::Aura {
            distance,
            radius,
            duration_ms,
            hit_points,
            tick_interval_ms,
            payload,
            ..
        } => {
            let mut line = format!(
                "Aura: radius {radius}, lasts {}, ticks every {}",
                format_duration_ms(*duration_ms),
                format_duration_ms(*tick_interval_ms)
            );
            if *distance > 0 {
                let _ = write!(line, ", place {distance} away");
            }
            if let Some(hit_points) = hit_points {
                let _ = write!(line, ", {hit_points} HP");
            }
            lines.push(line);
            append_payload_lines(lines, payload);
        }
    }
}

fn append_payload_lines(lines: &mut Vec<String>, payload: &EffectPayload) {
    if payload.amount > 0 || payload.amount_max.is_some() {
        let value_kind = match payload.kind {
            CombatValueKind::Damage => "damage",
            CombatValueKind::Heal => "heal",
        };
        let amount_label = if payload.has_amount_range() {
            format!("{}-{}", payload.amount_min(), payload.amount_max())
        } else {
            payload.amount.to_string()
        };
        let mut effect_line = format!("Effect: {amount_label} {value_kind}");
        if payload.can_crit() {
            let _ = write!(
                effect_line,
                ", crit {} for {}",
                format_bps_percent(payload.crit_chance_bps),
                format_bps_percent(payload.crit_multiplier_bps)
            );
        }
        lines.push(effect_line);
    }

    if let Some(status) = &payload.status {
        lines.push(format!("Status: {}", format_status_summary(status)));
    }

    if let Some(duration_ms) = payload.interrupt_silence_duration_ms {
        lines.push(format!(
            "Interrupt: cancels a cast and silences for {}",
            format_duration_ms(duration_ms)
        ));
    }

    if let Some(dispel) = payload.dispel {
        let scope = match dispel.scope {
            DispelScope::Positive => "positive",
            DispelScope::Negative => "negative",
            DispelScope::All => "any",
        };
        lines.push(format!(
            "Dispel: remove up to {} {scope} effect{}",
            dispel.max_statuses,
            if dispel.max_statuses == 1 { "" } else { "s" }
        ));
    }
}

fn format_status_summary(status: &StatusDefinition) -> String {
    let base = match status.kind {
        StatusKind::Poison => format!(
            "Poison {} every {} for {} (max {})",
            status.magnitude,
            format_duration_ms(status.tick_interval_ms.unwrap_or(0)),
            format_duration_ms(status.duration_ms),
            status.max_stacks
        ),
        StatusKind::Hot => format!(
            "Heal over time {} every {} for {} (max {})",
            status.magnitude,
            format_duration_ms(status.tick_interval_ms.unwrap_or(0)),
            format_duration_ms(status.duration_ms),
            status.max_stacks
        ),
        StatusKind::Chill => {
            let mut text = format!(
                "Chill {} for {} (max {})",
                status.magnitude,
                format_duration_ms(status.duration_ms),
                status.max_stacks
            );
            if let Some(trigger_duration_ms) = status.trigger_duration_ms {
                let _ = write!(
                    text,
                    ", follow-up after {}",
                    format_duration_ms(trigger_duration_ms)
                );
            }
            text
        }
        StatusKind::Root => format!("Root for {}", format_duration_ms(status.duration_ms)),
        StatusKind::Haste => format!(
            "Haste {} for {}",
            status.magnitude,
            format_duration_ms(status.duration_ms)
        ),
        StatusKind::Silence => format!("Silence for {}", format_duration_ms(status.duration_ms)),
        StatusKind::Stun => format!("Stun for {}", format_duration_ms(status.duration_ms)),
        StatusKind::Sleep => format!(
            "Sleep for {} (breaks on damage)",
            format_duration_ms(status.duration_ms)
        ),
        StatusKind::Shield => format!(
            "Shield {} for {} (max {})",
            status.magnitude,
            format_duration_ms(status.duration_ms),
            status.max_stacks
        ),
        StatusKind::Stealth => format!("Stealth for {}", format_duration_ms(status.duration_ms)),
        StatusKind::Reveal => format!("Reveal for {}", format_duration_ms(status.duration_ms)),
        StatusKind::Fear => format!("Fear for {}", format_duration_ms(status.duration_ms)),
        StatusKind::HealingReduction => format!(
            "Healing received -{} for {}",
            format_bps_percent(status.magnitude),
            format_duration_ms(status.duration_ms)
        ),
    };

    let mut extras = Vec::new();
    if status.expire_payload.is_some() {
        extras.push("blooms on expire");
    }
    if status.dispel_payload.is_some() {
        extras.push("blooms on dispel");
    }
    if extras.is_empty() {
        base
    } else {
        format!("{base}; {}", extras.join(", "))
    }
}

fn format_duration_ms(duration_ms: u16) -> String {
    if duration_ms == 0 {
        return String::from("0s");
    }
    if duration_ms.is_multiple_of(1000) {
        return format!("{}s", duration_ms / 1000);
    }
    let seconds = f32::from(duration_ms) / 1000.0;
    format!("{seconds:.1}s")
}

fn format_bps_percent(value: u16) -> String {
    let percent = f32::from(value) / 100.0;
    if value.is_multiple_of(100) {
        format!("{percent:.0}%")
    } else {
        format!("{percent:.1}%")
    }
}

fn skill_ui_category(skill: &SkillDefinition) -> &'static str {
    if let Some(payload) = primary_payload(&skill.behavior) {
        if payload.kind == CombatValueKind::Heal || status_kind(payload) == Some(StatusKind::Hot) {
            return "heal";
        }
        if status_kind(payload) == Some(StatusKind::Poison) {
            return "dot";
        }
        if payload.interrupt_silence_duration_ms.is_some()
            || matches!(
                status_kind(payload),
                Some(
                    StatusKind::Chill
                        | StatusKind::Root
                        | StatusKind::Silence
                        | StatusKind::Stun
                        | StatusKind::Sleep
                        | StatusKind::Reveal
                        | StatusKind::Fear
                        | StatusKind::HealingReduction
                )
            )
        {
            return "control";
        }
        if payload.dispel.is_some() {
            return "utility";
        }
        if matches!(
            status_kind(payload),
            Some(StatusKind::Shield | StatusKind::Haste | StatusKind::Stealth)
        ) {
            return "buff";
        }
    }

    match skill.behavior {
        SkillBehavior::Dash { .. }
        | SkillBehavior::Teleport { .. }
        | SkillBehavior::Passive { .. } => "mobility",
        SkillBehavior::Summon { .. }
        | SkillBehavior::Ward { .. }
        | SkillBehavior::Trap { .. }
        | SkillBehavior::Barrier { .. }
        | SkillBehavior::Aura { .. } => "utility",
        _ => "damage",
    }
}

fn primary_payload(behavior: &SkillBehavior) -> Option<&EffectPayload> {
    match behavior {
        SkillBehavior::Projectile { payload, .. }
        | SkillBehavior::Beam { payload, .. }
        | SkillBehavior::Burst { payload, .. }
        | SkillBehavior::Nova { payload, .. }
        | SkillBehavior::Channel { payload, .. }
        | SkillBehavior::Summon { payload, .. }
        | SkillBehavior::Trap { payload, .. }
        | SkillBehavior::Aura { payload, .. } => Some(payload),
        SkillBehavior::Dash { payload, .. } => payload.as_ref(),
        SkillBehavior::Teleport { .. }
        | SkillBehavior::Passive { .. }
        | SkillBehavior::Ward { .. }
        | SkillBehavior::Barrier { .. } => None,
    }
}

fn status_kind(payload: &EffectPayload) -> Option<StatusKind> {
    payload.status.as_ref().map(|status| status.kind)
}
