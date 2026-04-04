use game_domain::{LobbyId, PlayerRecord};

use crate::PacketError;

use super::codec::{
    encode_arena_combat_text_style, encode_arena_deployable_kind, encode_arena_effect_kind,
    encode_arena_match_phase, encode_arena_obstacle_kind, encode_arena_session_mode,
    encode_arena_status_kind, encode_bytes, encode_lobby_snapshot_phase, encode_optional_team,
    encode_optional_u8, encode_ready_state, encode_team, push_len_prefixed_string,
};
use super::server_types::{
    ArenaCombatTextEntry, ArenaDeltaSnapshot, ArenaDeployableSnapshot, ArenaEffectSnapshot,
    ArenaObstacleSnapshot, ArenaPlayerSnapshot, ArenaProjectileSnapshot, ArenaStateSnapshot,
    LobbyDirectoryEntry, LobbySnapshotPhase, LobbySnapshotPlayer, SkillCatalogEntry,
};
use super::{
    MAX_MESSAGE_BYTES, MAX_SKILL_AUDIO_CUE_BYTES, MAX_SKILL_DESCRIPTION_BYTES, MAX_SKILL_ID_BYTES,
    MAX_SKILL_NAME_BYTES, MAX_SKILL_SUMMARY_BYTES, MAX_SKILL_TREE_NAME_BYTES,
    MAX_SKILL_UI_CATEGORY_BYTES,
};

pub(super) fn encode_player_record(
    payload: &mut Vec<u8>,
    record: &PlayerRecord,
) -> Result<(), PacketError> {
    payload.extend_from_slice(&record.wins.to_le_bytes());
    payload.extend_from_slice(&record.losses.to_le_bytes());
    payload.extend_from_slice(&record.no_contests.to_le_bytes());
    payload.extend_from_slice(&record.round_wins.to_le_bytes());
    payload.extend_from_slice(&record.round_losses.to_le_bytes());
    payload.extend_from_slice(&record.total_damage_done.to_le_bytes());
    payload.extend_from_slice(&record.total_healing_done.to_le_bytes());
    payload.extend_from_slice(&record.total_combat_ms.to_le_bytes());
    payload.extend_from_slice(&record.cc_used.to_le_bytes());
    payload.extend_from_slice(&record.cc_hits.to_le_bytes());
    let skill_pick_count = u16::try_from(record.skill_pick_counts.len()).unwrap_or(u16::MAX);
    payload.extend_from_slice(&skill_pick_count.to_le_bytes());
    for (skill_id, count) in record
        .skill_pick_counts
        .iter()
        .take(usize::from(skill_pick_count))
    {
        push_len_prefixed_string(payload, "skill_id", skill_id, MAX_SKILL_ID_BYTES)?;
        payload.extend_from_slice(&count.to_le_bytes());
    }
    Ok(())
}

pub(super) fn encode_skill_catalog(
    payload: &mut Vec<u8>,
    catalog: &[SkillCatalogEntry],
) -> Result<(), PacketError> {
    let entry_count = u16::try_from(catalog.len()).map_err(|_| PacketError::PayloadTooLarge {
        actual: catalog.len(),
        maximum: usize::from(u16::MAX),
    })?;
    payload.extend_from_slice(&entry_count.to_le_bytes());
    for entry in catalog {
        push_len_prefixed_string(
            payload,
            "skill_tree",
            entry.tree.as_str(),
            MAX_SKILL_TREE_NAME_BYTES,
        )?;
        payload.push(entry.tier);
        push_len_prefixed_string(payload, "skill_id", &entry.skill_id, MAX_SKILL_ID_BYTES)?;
        push_len_prefixed_string(
            payload,
            "skill_name",
            &entry.skill_name,
            MAX_SKILL_NAME_BYTES,
        )?;
        push_len_prefixed_string(
            payload,
            "skill_description",
            &entry.skill_description,
            MAX_SKILL_DESCRIPTION_BYTES,
        )?;
        push_len_prefixed_string(
            payload,
            "skill_summary",
            &entry.skill_summary,
            MAX_SKILL_SUMMARY_BYTES,
        )?;
        push_len_prefixed_string(
            payload,
            "ui_category",
            &entry.ui_category,
            MAX_SKILL_UI_CATEGORY_BYTES,
        )?;
        push_len_prefixed_string(
            payload,
            "audio_cue_id",
            &entry.audio_cue_id,
            MAX_SKILL_AUDIO_CUE_BYTES,
        )?;
    }
    Ok(())
}

pub(super) fn encode_lobby_directory_snapshot(
    payload: &mut Vec<u8>,
    lobbies: &[LobbyDirectoryEntry],
) -> Result<(), PacketError> {
    let lobby_count = u16::try_from(lobbies.len()).map_err(|_| PacketError::PayloadTooLarge {
        actual: lobbies.len(),
        maximum: usize::from(u16::MAX),
    })?;
    payload.extend_from_slice(&lobby_count.to_le_bytes());
    for lobby in lobbies {
        payload.extend_from_slice(&lobby.lobby_id.get().to_le_bytes());
        payload.extend_from_slice(&lobby.player_count.to_le_bytes());
        payload.extend_from_slice(&lobby.team_a_count.to_le_bytes());
        payload.extend_from_slice(&lobby.team_b_count.to_le_bytes());
        payload.extend_from_slice(&lobby.ready_count.to_le_bytes());
        encode_lobby_snapshot_phase(payload, lobby.phase);
    }

    Ok(())
}

pub(super) fn encode_game_lobby_snapshot(
    payload: &mut Vec<u8>,
    lobby_id: LobbyId,
    phase: LobbySnapshotPhase,
    players: &[LobbySnapshotPlayer],
) -> Result<(), PacketError> {
    payload.extend_from_slice(&lobby_id.get().to_le_bytes());
    encode_lobby_snapshot_phase(payload, phase);

    let player_count = u16::try_from(players.len()).map_err(|_| PacketError::PayloadTooLarge {
        actual: players.len(),
        maximum: usize::from(u16::MAX),
    })?;
    payload.extend_from_slice(&player_count.to_le_bytes());
    for player in players {
        payload.extend_from_slice(&player.player_id.get().to_le_bytes());
        push_len_prefixed_string(
            payload,
            "player_name",
            player.player_name.as_str(),
            game_domain::MAX_PLAYER_NAME_LEN,
        )?;
        encode_player_record(payload, &player.record)?;
        payload.push(encode_optional_team(player.team));
        payload.push(encode_ready_state(player.ready));
    }

    Ok(())
}

pub(super) fn encode_arena_state_snapshot(
    payload: &mut Vec<u8>,
    snapshot: &ArenaStateSnapshot,
) -> Result<(), PacketError> {
    payload.push(encode_arena_session_mode(snapshot.mode));
    payload.push(encode_arena_match_phase(snapshot.phase));
    encode_optional_u8(payload, snapshot.phase_seconds_remaining);
    payload.extend_from_slice(&snapshot.width.to_le_bytes());
    payload.extend_from_slice(&snapshot.height.to_le_bytes());
    payload.extend_from_slice(&snapshot.tile_units.to_le_bytes());
    encode_bytes(payload, "footprint_tiles", &snapshot.footprint_tiles)?;
    encode_bytes(payload, "objective_tiles", &snapshot.objective_tiles)?;
    encode_bytes(payload, "visible_tiles", &snapshot.visible_tiles)?;
    encode_bytes(payload, "explored_tiles", &snapshot.explored_tiles)?;
    payload.extend_from_slice(&snapshot.objective_target_ms.to_le_bytes());
    payload.extend_from_slice(&snapshot.objective_team_a_ms.to_le_bytes());
    payload.extend_from_slice(&snapshot.objective_team_b_ms.to_le_bytes());

    let obstacle_count =
        u16::try_from(snapshot.obstacles.len()).map_err(|_| PacketError::PayloadTooLarge {
            actual: snapshot.obstacles.len(),
            maximum: usize::from(u16::MAX),
        })?;
    payload.extend_from_slice(&obstacle_count.to_le_bytes());
    for obstacle in &snapshot.obstacles {
        payload.push(encode_arena_obstacle_kind(obstacle.kind));
        payload.extend_from_slice(&obstacle.center_x.to_le_bytes());
        payload.extend_from_slice(&obstacle.center_y.to_le_bytes());
        payload.extend_from_slice(&obstacle.half_width.to_le_bytes());
        payload.extend_from_slice(&obstacle.half_height.to_le_bytes());
    }

    encode_arena_deployables(payload, &snapshot.deployables)?;
    encode_arena_players(payload, &snapshot.players)?;
    encode_arena_projectiles(payload, &snapshot.projectiles)?;
    encode_training_metrics(payload, snapshot.training_metrics);

    Ok(())
}

pub(super) fn encode_arena_delta_snapshot(
    payload: &mut Vec<u8>,
    snapshot: &ArenaDeltaSnapshot,
) -> Result<(), PacketError> {
    payload.push(encode_arena_session_mode(snapshot.mode));
    payload.push(encode_arena_match_phase(snapshot.phase));
    encode_optional_u8(payload, snapshot.phase_seconds_remaining);
    payload.extend_from_slice(&snapshot.tile_units.to_le_bytes());
    encode_bytes(payload, "footprint_tiles", &snapshot.footprint_tiles)?;
    encode_bytes(payload, "objective_tiles", &snapshot.objective_tiles)?;
    encode_bytes(payload, "visible_tiles", &snapshot.visible_tiles)?;
    encode_bytes(payload, "explored_tiles", &snapshot.explored_tiles)?;
    payload.extend_from_slice(&snapshot.objective_target_ms.to_le_bytes());
    payload.extend_from_slice(&snapshot.objective_team_a_ms.to_le_bytes());
    payload.extend_from_slice(&snapshot.objective_team_b_ms.to_le_bytes());
    encode_arena_obstacles(payload, &snapshot.obstacles)?;
    encode_arena_deployables(payload, &snapshot.deployables)?;
    encode_arena_players(payload, &snapshot.players)?;
    encode_arena_projectiles(payload, &snapshot.projectiles)?;
    encode_training_metrics(payload, snapshot.training_metrics);
    Ok(())
}

fn encode_training_metrics(
    payload: &mut Vec<u8>,
    metrics: Option<super::server_types::TrainingMetricsSnapshot>,
) {
    match metrics {
        Some(metrics) => {
            payload.push(1);
            payload.extend_from_slice(&metrics.damage_done.to_le_bytes());
            payload.extend_from_slice(&metrics.healing_done.to_le_bytes());
            payload.extend_from_slice(&metrics.elapsed_ms.to_le_bytes());
        }
        None => payload.push(0),
    }
}

pub(super) fn encode_arena_obstacles(
    payload: &mut Vec<u8>,
    obstacles: &[ArenaObstacleSnapshot],
) -> Result<(), PacketError> {
    let obstacle_count =
        u16::try_from(obstacles.len()).map_err(|_| PacketError::PayloadTooLarge {
            actual: obstacles.len(),
            maximum: usize::from(u16::MAX),
        })?;
    payload.extend_from_slice(&obstacle_count.to_le_bytes());
    for obstacle in obstacles {
        payload.push(encode_arena_obstacle_kind(obstacle.kind));
        payload.extend_from_slice(&obstacle.center_x.to_le_bytes());
        payload.extend_from_slice(&obstacle.center_y.to_le_bytes());
        payload.extend_from_slice(&obstacle.half_width.to_le_bytes());
        payload.extend_from_slice(&obstacle.half_height.to_le_bytes());
    }
    Ok(())
}

pub(super) fn encode_arena_players(
    payload: &mut Vec<u8>,
    players: &[ArenaPlayerSnapshot],
) -> Result<(), PacketError> {
    let player_count = u16::try_from(players.len()).map_err(|_| PacketError::PayloadTooLarge {
        actual: players.len(),
        maximum: usize::from(u16::MAX),
    })?;
    payload.extend_from_slice(&player_count.to_le_bytes());
    for player in players {
        encode_arena_player(payload, player)?;
    }
    Ok(())
}

pub(super) fn encode_arena_deployables(
    payload: &mut Vec<u8>,
    deployables: &[ArenaDeployableSnapshot],
) -> Result<(), PacketError> {
    let deployable_count =
        u16::try_from(deployables.len()).map_err(|_| PacketError::PayloadTooLarge {
            actual: deployables.len(),
            maximum: usize::from(u16::MAX),
        })?;
    payload.extend_from_slice(&deployable_count.to_le_bytes());
    for deployable in deployables {
        payload.extend_from_slice(&deployable.id.to_le_bytes());
        payload.extend_from_slice(&deployable.owner.get().to_le_bytes());
        payload.push(encode_team(deployable.team));
        payload.push(encode_arena_deployable_kind(deployable.kind));
        payload.extend_from_slice(&deployable.x.to_le_bytes());
        payload.extend_from_slice(&deployable.y.to_le_bytes());
        payload.extend_from_slice(&deployable.radius.to_le_bytes());
        payload.extend_from_slice(&deployable.hit_points.to_le_bytes());
        payload.extend_from_slice(&deployable.max_hit_points.to_le_bytes());
        payload.extend_from_slice(&deployable.remaining_ms.to_le_bytes());
    }
    Ok(())
}

pub(super) fn encode_arena_player(
    payload: &mut Vec<u8>,
    player: &ArenaPlayerSnapshot,
) -> Result<(), PacketError> {
    payload.extend_from_slice(&player.player_id.get().to_le_bytes());
    push_len_prefixed_string(
        payload,
        "player_name",
        player.player_name.as_str(),
        game_domain::MAX_PLAYER_NAME_LEN,
    )?;
    payload.push(encode_team(player.team));
    payload.extend_from_slice(&player.x.to_le_bytes());
    payload.extend_from_slice(&player.y.to_le_bytes());
    payload.extend_from_slice(&player.aim_x.to_le_bytes());
    payload.extend_from_slice(&player.aim_y.to_le_bytes());
    payload.extend_from_slice(&player.hit_points.to_le_bytes());
    payload.extend_from_slice(&player.max_hit_points.to_le_bytes());
    payload.extend_from_slice(&player.mana.to_le_bytes());
    payload.extend_from_slice(&player.max_mana.to_le_bytes());
    payload.push(u8::from(player.alive));
    payload.push(player.unlocked_skill_slots);
    payload.extend_from_slice(&player.primary_cooldown_remaining_ms.to_le_bytes());
    payload.extend_from_slice(&player.primary_cooldown_total_ms.to_le_bytes());
    for remaining in player.slot_cooldown_remaining_ms {
        payload.extend_from_slice(&remaining.to_le_bytes());
    }
    for total in player.slot_cooldown_total_ms {
        payload.extend_from_slice(&total.to_le_bytes());
    }
    for tree in &player.equipped_skill_trees {
        match tree {
            Some(tree) => {
                payload.push(1);
                push_len_prefixed_string(
                    payload,
                    "skill_tree",
                    tree.as_str(),
                    MAX_SKILL_TREE_NAME_BYTES,
                )?;
            }
            None => payload.push(0),
        }
    }
    encode_optional_u8(payload, player.current_cast_slot);
    payload.extend_from_slice(&player.current_cast_remaining_ms.to_le_bytes());
    payload.extend_from_slice(&player.current_cast_total_ms.to_le_bytes());
    let status_count =
        u8::try_from(player.active_statuses.len()).map_err(|_| PacketError::PayloadTooLarge {
            actual: player.active_statuses.len(),
            maximum: usize::from(u8::MAX),
        })?;
    payload.push(status_count);
    for status in &player.active_statuses {
        payload.extend_from_slice(&status.source.get().to_le_bytes());
        payload.push(status.slot);
        payload.push(encode_arena_status_kind(status.kind));
        payload.push(status.stacks);
        payload.extend_from_slice(&status.remaining_ms.to_le_bytes());
    }
    Ok(())
}

pub(super) fn encode_arena_projectiles(
    payload: &mut Vec<u8>,
    projectiles: &[ArenaProjectileSnapshot],
) -> Result<(), PacketError> {
    let projectile_count =
        u16::try_from(projectiles.len()).map_err(|_| PacketError::PayloadTooLarge {
            actual: projectiles.len(),
            maximum: usize::from(u16::MAX),
        })?;
    payload.extend_from_slice(&projectile_count.to_le_bytes());
    for projectile in projectiles {
        payload.extend_from_slice(&projectile.owner.get().to_le_bytes());
        payload.push(projectile.slot);
        payload.push(encode_arena_effect_kind(projectile.kind));
        payload.extend_from_slice(&projectile.x.to_le_bytes());
        payload.extend_from_slice(&projectile.y.to_le_bytes());
        payload.extend_from_slice(&projectile.radius.to_le_bytes());
    }
    Ok(())
}

pub(super) fn encode_arena_effect_batch(
    payload: &mut Vec<u8>,
    effects: &[ArenaEffectSnapshot],
) -> Result<(), PacketError> {
    let effect_count = u16::try_from(effects.len()).map_err(|_| PacketError::PayloadTooLarge {
        actual: effects.len(),
        maximum: usize::from(u16::MAX),
    })?;
    payload.extend_from_slice(&effect_count.to_le_bytes());
    for effect in effects {
        payload.push(encode_arena_effect_kind(effect.kind));
        payload.extend_from_slice(&effect.owner.get().to_le_bytes());
        payload.push(effect.slot);
        payload.extend_from_slice(&effect.x.to_le_bytes());
        payload.extend_from_slice(&effect.y.to_le_bytes());
        payload.extend_from_slice(&effect.target_x.to_le_bytes());
        payload.extend_from_slice(&effect.target_y.to_le_bytes());
        payload.extend_from_slice(&effect.radius.to_le_bytes());
    }
    Ok(())
}

pub(super) fn encode_arena_combat_text_batch(
    payload: &mut Vec<u8>,
    entries: &[ArenaCombatTextEntry],
) -> Result<(), PacketError> {
    let entry_count = u16::try_from(entries.len()).map_err(|_| PacketError::PayloadTooLarge {
        actual: entries.len(),
        maximum: usize::from(u16::MAX),
    })?;
    payload.extend_from_slice(&entry_count.to_le_bytes());
    for entry in entries {
        payload.extend_from_slice(&entry.x.to_le_bytes());
        payload.extend_from_slice(&entry.y.to_le_bytes());
        payload.push(encode_arena_combat_text_style(entry.style));
        push_len_prefixed_string(payload, "text", &entry.text, MAX_MESSAGE_BYTES)?;
    }
    Ok(())
}
