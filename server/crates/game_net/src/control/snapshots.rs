use game_domain::{LobbyId, PlayerRecord};

use crate::PacketError;

use super::codec::*;
use super::{
    ArenaDeltaSnapshot, ArenaEffectSnapshot, ArenaObstacleSnapshot, ArenaPlayerSnapshot,
    ArenaProjectileSnapshot, ArenaStateSnapshot, ArenaStatusSnapshot, LobbyDirectoryEntry,
    LobbySnapshotPhase, LobbySnapshotPlayer, ServerControlEvent, SkillCatalogEntry,
    MAX_SKILL_ID_BYTES, MAX_SKILL_NAME_BYTES, MAX_SKILL_TREE_NAME_BYTES,
};
pub(super) fn encode_player_record(payload: &mut Vec<u8>, record: PlayerRecord) {
    payload.extend_from_slice(&record.wins.to_le_bytes());
    payload.extend_from_slice(&record.losses.to_le_bytes());
    payload.extend_from_slice(&record.no_contests.to_le_bytes());
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

pub(super) fn decode_lobby_directory_snapshot(
    payload: &[u8],
    index: &mut usize,
) -> Result<ServerControlEvent, PacketError> {
    let lobby_count = usize::from(read_u16(payload, index, "LobbyDirectorySnapshot")?);
    let mut lobbies = Vec::with_capacity(lobby_count);
    for _ in 0..lobby_count {
        lobbies.push(LobbyDirectoryEntry {
            lobby_id: read_lobby_id(payload, index, "LobbyDirectorySnapshot")?,
            player_count: read_u16(payload, index, "LobbyDirectorySnapshot")?,
            team_a_count: read_u16(payload, index, "LobbyDirectorySnapshot")?,
            team_b_count: read_u16(payload, index, "LobbyDirectorySnapshot")?,
            ready_count: read_u16(payload, index, "LobbyDirectorySnapshot")?,
            phase: read_lobby_snapshot_phase(payload, index, "LobbyDirectorySnapshot")?,
        });
    }

    Ok(ServerControlEvent::LobbyDirectorySnapshot { lobbies })
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
        encode_player_record(payload, player.record);
        payload.push(encode_optional_team(player.team));
        payload.push(encode_ready_state(player.ready));
    }

    Ok(())
}

pub(super) fn decode_game_lobby_snapshot(
    payload: &[u8],
    index: &mut usize,
) -> Result<ServerControlEvent, PacketError> {
    let lobby_id = read_lobby_id(payload, index, "GameLobbySnapshot")?;
    let phase = read_lobby_snapshot_phase(payload, index, "GameLobbySnapshot")?;
    let player_count = usize::from(read_u16(payload, index, "GameLobbySnapshot")?);
    let mut players = Vec::with_capacity(player_count);
    for _ in 0..player_count {
        players.push(LobbySnapshotPlayer {
            player_id: read_player_id(payload, index, "GameLobbySnapshot")?,
            player_name: read_player_name(payload, index, "GameLobbySnapshot")?,
            record: read_player_record(payload, index, "GameLobbySnapshot")?,
            team: read_optional_team(payload, index, "GameLobbySnapshot")?,
            ready: read_ready_state(payload, index, "GameLobbySnapshot")?,
        });
    }

    Ok(ServerControlEvent::GameLobbySnapshot {
        lobby_id,
        phase,
        players,
    })
}

pub(super) fn encode_arena_state_snapshot(
    payload: &mut Vec<u8>,
    snapshot: &ArenaStateSnapshot,
) -> Result<(), PacketError> {
    payload.push(encode_arena_match_phase(snapshot.phase));
    encode_optional_u8(payload, snapshot.phase_seconds_remaining);
    payload.extend_from_slice(&snapshot.width.to_le_bytes());
    payload.extend_from_slice(&snapshot.height.to_le_bytes());
    payload.extend_from_slice(&snapshot.tile_units.to_le_bytes());
    encode_bytes(payload, "visible_tiles", &snapshot.visible_tiles)?;
    encode_bytes(payload, "explored_tiles", &snapshot.explored_tiles)?;

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

    encode_arena_players(payload, &snapshot.players)?;
    encode_arena_projectiles(payload, &snapshot.projectiles)?;

    Ok(())
}

pub(super) fn decode_arena_state_snapshot(
    payload: &[u8],
    index: &mut usize,
) -> Result<ServerControlEvent, PacketError> {
    let phase = read_arena_match_phase(payload, index, "ArenaStateSnapshot")?;
    let phase_seconds_remaining = read_optional_u8(payload, index, "ArenaStateSnapshot")?;
    let width = read_u16(payload, index, "ArenaStateSnapshot")?;
    let height = read_u16(payload, index, "ArenaStateSnapshot")?;
    let tile_units = read_u16(payload, index, "ArenaStateSnapshot")?;
    let visible_tiles = decode_bytes(payload, index, "ArenaStateSnapshot", "visible_tiles")?;
    let explored_tiles = decode_bytes(payload, index, "ArenaStateSnapshot", "explored_tiles")?;
    let obstacles = decode_arena_obstacles(payload, index, "ArenaStateSnapshot")?;

    let players = decode_arena_players(payload, index, "ArenaStateSnapshot")?;
    let projectiles = decode_arena_projectiles(payload, index, "ArenaStateSnapshot")?;

    Ok(ServerControlEvent::ArenaStateSnapshot {
        snapshot: ArenaStateSnapshot {
            phase,
            phase_seconds_remaining,
            width,
            height,
            tile_units,
            visible_tiles,
            explored_tiles,
            obstacles,
            players,
            projectiles,
        },
    })
}

pub(super) fn encode_arena_delta_snapshot(
    payload: &mut Vec<u8>,
    snapshot: &ArenaDeltaSnapshot,
) -> Result<(), PacketError> {
    payload.push(encode_arena_match_phase(snapshot.phase));
    encode_optional_u8(payload, snapshot.phase_seconds_remaining);
    payload.extend_from_slice(&snapshot.tile_units.to_le_bytes());
    encode_bytes(payload, "visible_tiles", &snapshot.visible_tiles)?;
    encode_bytes(payload, "explored_tiles", &snapshot.explored_tiles)?;
    encode_arena_obstacles(payload, &snapshot.obstacles)?;
    encode_arena_players(payload, &snapshot.players)?;
    encode_arena_projectiles(payload, &snapshot.projectiles)?;
    Ok(())
}

pub(super) fn decode_arena_delta_snapshot(
    payload: &[u8],
    index: &mut usize,
) -> Result<ServerControlEvent, PacketError> {
    let phase = read_arena_match_phase(payload, index, "ArenaDeltaSnapshot")?;
    let phase_seconds_remaining = read_optional_u8(payload, index, "ArenaDeltaSnapshot")?;
    let tile_units = read_u16(payload, index, "ArenaDeltaSnapshot")?;
    let visible_tiles = decode_bytes(payload, index, "ArenaDeltaSnapshot", "visible_tiles")?;
    let explored_tiles = decode_bytes(payload, index, "ArenaDeltaSnapshot", "explored_tiles")?;
    let obstacles = decode_arena_obstacles(payload, index, "ArenaDeltaSnapshot")?;
    let players = decode_arena_players(payload, index, "ArenaDeltaSnapshot")?;
    let projectiles = decode_arena_projectiles(payload, index, "ArenaDeltaSnapshot")?;
    Ok(ServerControlEvent::ArenaDeltaSnapshot {
        snapshot: ArenaDeltaSnapshot {
            phase,
            phase_seconds_remaining,
            tile_units,
            visible_tiles,
            explored_tiles,
            obstacles,
            players,
            projectiles,
        },
    })
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

pub(super) fn decode_arena_obstacles(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<Vec<ArenaObstacleSnapshot>, PacketError> {
    let obstacle_count = usize::from(read_u16(payload, index, kind)?);
    let mut obstacles = Vec::with_capacity(obstacle_count);
    for _ in 0..obstacle_count {
        obstacles.push(ArenaObstacleSnapshot {
            kind: read_arena_obstacle_kind(payload, index, kind)?,
            center_x: read_i16(payload, index, kind)?,
            center_y: read_i16(payload, index, kind)?,
            half_width: read_u16(payload, index, kind)?,
            half_height: read_u16(payload, index, kind)?,
        });
    }
    Ok(obstacles)
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

pub(super) fn decode_arena_players(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<Vec<ArenaPlayerSnapshot>, PacketError> {
    let player_count = usize::from(read_u16(payload, index, kind)?);
    let mut players = Vec::with_capacity(player_count);
    for _ in 0..player_count {
        let player_id = read_player_id(payload, index, kind)?;
        let player_name = read_player_name(payload, index, kind)?;
        let team = read_team(payload, index, kind)?;
        let x = read_i16(payload, index, kind)?;
        let y = read_i16(payload, index, kind)?;
        let aim_x = read_i16(payload, index, kind)?;
        let aim_y = read_i16(payload, index, kind)?;
        let hit_points = read_u16(payload, index, kind)?;
        let max_hit_points = read_u16(payload, index, kind)?;
        let mana = read_u16(payload, index, kind)?;
        let max_mana = read_u16(payload, index, kind)?;
        let alive = read_bool(payload, index, kind)?;
        let unlocked_skill_slots = read_u8(payload, index, kind)?;
        let primary_cooldown_remaining_ms = read_u16(payload, index, kind)?;
        let primary_cooldown_total_ms = read_u16(payload, index, kind)?;
        let mut slot_cooldown_remaining_ms = [0_u16; 5];
        for remaining in &mut slot_cooldown_remaining_ms {
            *remaining = read_u16(payload, index, kind)?;
        }
        let mut slot_cooldown_total_ms = [0_u16; 5];
        for total in &mut slot_cooldown_total_ms {
            *total = read_u16(payload, index, kind)?;
        }
        let status_count = usize::from(read_u8(payload, index, kind)?);
        let mut active_statuses = Vec::with_capacity(status_count);
        for _ in 0..status_count {
            active_statuses.push(ArenaStatusSnapshot {
                source: read_player_id(payload, index, kind)?,
                slot: read_u8(payload, index, kind)?,
                kind: read_arena_status_kind(payload, index, kind)?,
                stacks: read_u8(payload, index, kind)?,
                remaining_ms: read_u16(payload, index, kind)?,
            });
        }
        players.push(ArenaPlayerSnapshot {
            player_id,
            player_name,
            team,
            x,
            y,
            aim_x,
            aim_y,
            hit_points,
            max_hit_points,
            mana,
            max_mana,
            alive,
            unlocked_skill_slots,
            primary_cooldown_remaining_ms,
            primary_cooldown_total_ms,
            slot_cooldown_remaining_ms,
            slot_cooldown_total_ms,
            active_statuses,
        });
    }
    Ok(players)
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

pub(super) fn decode_arena_projectiles(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<Vec<ArenaProjectileSnapshot>, PacketError> {
    let projectile_count = usize::from(read_u16(payload, index, kind)?);
    let mut projectiles = Vec::with_capacity(projectile_count);
    for _ in 0..projectile_count {
        projectiles.push(ArenaProjectileSnapshot {
            owner: read_player_id(payload, index, kind)?,
            slot: read_u8(payload, index, kind)?,
            kind: read_arena_effect_kind(payload, index, kind)?,
            x: read_i16(payload, index, kind)?,
            y: read_i16(payload, index, kind)?,
            radius: read_u16(payload, index, kind)?,
        });
    }
    Ok(projectiles)
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

pub(super) fn decode_arena_effect_batch(
    payload: &[u8],
    index: &mut usize,
) -> Result<ServerControlEvent, PacketError> {
    let effect_count = usize::from(read_u16(payload, index, "ArenaEffectBatch")?);
    let mut effects = Vec::with_capacity(effect_count);
    for _ in 0..effect_count {
        effects.push(ArenaEffectSnapshot {
            kind: read_arena_effect_kind(payload, index, "ArenaEffectBatch")?,
            owner: read_player_id(payload, index, "ArenaEffectBatch")?,
            slot: read_u8(payload, index, "ArenaEffectBatch")?,
            x: read_i16(payload, index, "ArenaEffectBatch")?,
            y: read_i16(payload, index, "ArenaEffectBatch")?,
            target_x: read_i16(payload, index, "ArenaEffectBatch")?,
            target_y: read_i16(payload, index, "ArenaEffectBatch")?,
            radius: read_u16(payload, index, "ArenaEffectBatch")?,
        });
    }

    Ok(ServerControlEvent::ArenaEffectBatch { effects })
}
