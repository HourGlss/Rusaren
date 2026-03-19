use crate::PacketError;

use super::codec::{
    decode_bytes, read_arena_effect_kind, read_arena_match_phase, read_arena_obstacle_kind,
    read_arena_status_kind, read_bool, read_i16, read_lobby_id, read_lobby_snapshot_phase,
    read_optional_team, read_optional_u8, read_player_id, read_player_name, read_player_record,
    read_ready_state, read_team, read_u16, read_u8,
};
use super::server_types::{
    ArenaDeltaSnapshot, ArenaEffectSnapshot, ArenaObstacleSnapshot, ArenaPlayerSnapshot,
    ArenaProjectileSnapshot, ArenaStateSnapshot, ArenaStatusSnapshot, LobbyDirectoryEntry,
    LobbySnapshotPlayer, ServerControlEvent,
};

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
