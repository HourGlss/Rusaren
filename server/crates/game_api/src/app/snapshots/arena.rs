use game_content::{ArenaMapDefinition, GameContent};
use game_domain::{MatchId, PlayerId, TeamAssignment};
use game_match::{MatchPhase, MatchSession};
use game_net::{
    ArenaEffectKind, ArenaEffectSnapshot, ArenaMatchPhase, ArenaObstacleKind,
    ArenaObstacleSnapshot, ArenaPlayerSnapshot, ArenaProjectileSnapshot, ArenaStatusKind,
    ArenaStatusSnapshot, SkillCatalogEntry,
};
use game_sim::{ArenaEffect, ArenaObstacle, SimulationEvent, SimulationWorld};

use super::super::{MatchRuntime, ServerApp};

impl ServerApp {
    pub(in super::super) fn arena_obstacle_snapshot(
        obstacle: &ArenaObstacle,
    ) -> ArenaObstacleSnapshot {
        ArenaObstacleSnapshot {
            kind: match obstacle.kind {
                game_sim::ArenaObstacleKind::Pillar => ArenaObstacleKind::Pillar,
                game_sim::ArenaObstacleKind::Shrub => ArenaObstacleKind::Shrub,
            },
            center_x: obstacle.center_x,
            center_y: obstacle.center_y,
            half_width: obstacle.half_width,
            half_height: obstacle.half_height,
        }
    }

    pub(in super::super) fn arena_obstacles_snapshot(
        obstacles: &[ArenaObstacle],
        map: &ArenaMapDefinition,
        explored_tiles: &[u8],
    ) -> Vec<ArenaObstacleSnapshot> {
        obstacles
            .iter()
            .filter(|obstacle| Self::mask_intersects_obstacle(map, explored_tiles, obstacle))
            .map(Self::arena_obstacle_snapshot)
            .collect()
    }

    pub(in super::super) fn arena_player_snapshot(
        world: &SimulationWorld,
        assignment: &TeamAssignment,
        state: game_sim::SimPlayerState,
        unlocked_skill_slots: u8,
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
            active_statuses: world
                .statuses_for(assignment.player_id)
                .unwrap_or_default()
                .into_iter()
                .map(Self::arena_status_snapshot)
                .collect(),
        }
    }

    pub(in super::super) fn arena_players_snapshot(
        runtime: &MatchRuntime,
        viewer_id: PlayerId,
        map: &ArenaMapDefinition,
        visible_tiles: &[u8],
    ) -> Vec<ArenaPlayerSnapshot> {
        let unlocked_skill_slots = runtime.session.current_round().get();
        runtime
            .roster
            .iter()
            .filter_map(|assignment| {
                runtime
                    .world
                    .player_state(assignment.player_id)
                    .filter(|state| {
                        assignment.player_id == viewer_id
                            || Self::mask_contains_point(map, visible_tiles, state.x, state.y)
                    })
                    .map(|state| {
                        Self::arena_player_snapshot(
                            &runtime.world,
                            assignment,
                            state,
                            unlocked_skill_slots,
                        )
                    })
            })
            .collect()
    }

    pub(in super::super) fn arena_projectiles_snapshot(
        runtime: &MatchRuntime,
        viewer_id: PlayerId,
        map: &ArenaMapDefinition,
        visible_tiles: &[u8],
    ) -> Vec<ArenaProjectileSnapshot> {
        runtime
            .world
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
        let Some((visible_tiles, _)) = Self::build_visibility_masks(runtime, viewer_id, map) else {
            return Vec::new();
        };
        effects
            .iter()
            .copied()
            .filter(|effect| {
                effect.owner == viewer_id
                    || Self::mask_contains_point(map, &visible_tiles, effect.x, effect.y)
                    || Self::mask_contains_point(
                        map,
                        &visible_tiles,
                        effect.target_x,
                        effect.target_y,
                    )
            })
            .collect()
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
        events: &[SimulationEvent],
    ) -> Vec<ArenaEffectSnapshot> {
        events
            .iter()
            .filter_map(|event| match event {
                SimulationEvent::EffectSpawned { effect } => {
                    Some(Self::arena_effect_snapshot(effect))
                }
                _ => None,
            })
            .collect()
    }

    pub(in super::super) fn arena_effect_snapshot(effect: &ArenaEffect) -> ArenaEffectSnapshot {
        ArenaEffectSnapshot {
            kind: match effect.kind {
                game_sim::ArenaEffectKind::MeleeSwing => ArenaEffectKind::MeleeSwing,
                game_sim::ArenaEffectKind::SkillShot => ArenaEffectKind::SkillShot,
                game_sim::ArenaEffectKind::DashTrail => ArenaEffectKind::DashTrail,
                game_sim::ArenaEffectKind::Burst => ArenaEffectKind::Burst,
                game_sim::ArenaEffectKind::Nova => ArenaEffectKind::Nova,
                game_sim::ArenaEffectKind::Beam => ArenaEffectKind::Beam,
                game_sim::ArenaEffectKind::HitSpark => ArenaEffectKind::HitSpark,
            },
            owner: effect.owner,
            slot: effect.slot,
            x: effect.x,
            y: effect.y,
            target_x: effect.target_x,
            target_y: effect.target_y,
            radius: effect.radius,
        }
    }
}
