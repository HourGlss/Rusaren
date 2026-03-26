use game_domain::{PlayerId, PlayerName, TeamSide};

use crate::PacketError;

use super::codec::{
    decode_bytes, read_arena_deployable_kind, read_arena_effect_kind, read_arena_match_phase,
    read_arena_obstacle_kind, read_arena_status_kind, read_bool, read_i16, read_lobby_id,
    read_lobby_snapshot_phase, read_optional_team, read_optional_u8, read_player_id,
    read_player_name, read_player_record, read_ready_state, read_team, read_u16, read_u32,
    read_u8,
};
use super::server_types::{
    ArenaDeltaSnapshot, ArenaDeployableSnapshot, ArenaEffectSnapshot, ArenaObstacleSnapshot,
    ArenaPlayerSnapshot, ArenaProjectileSnapshot, ArenaStateSnapshot, ArenaStatusSnapshot,
    LobbyDirectoryEntry, LobbySnapshotPlayer, ServerControlEvent,
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
    let deployables = decode_arena_deployables(payload, index, "ArenaStateSnapshot")?;
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
            deployables,
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
    let deployables = decode_arena_deployables(payload, index, "ArenaDeltaSnapshot")?;
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
            deployables,
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
        players.push(decode_arena_player(payload, index, kind)?);
    }
    Ok(players)
}

pub(super) fn decode_arena_deployables(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<Vec<ArenaDeployableSnapshot>, PacketError> {
    let deployable_count = usize::from(read_u16(payload, index, kind)?);
    let mut deployables = Vec::with_capacity(deployable_count);
    for _ in 0..deployable_count {
        deployables.push(ArenaDeployableSnapshot {
            id: read_u32(payload, index, kind)?,
            owner: read_player_id(payload, index, kind)?,
            team: read_team(payload, index, kind)?,
            kind: read_arena_deployable_kind(payload, index, kind)?,
            x: read_i16(payload, index, kind)?,
            y: read_i16(payload, index, kind)?,
            radius: read_u16(payload, index, kind)?,
            hit_points: read_u16(payload, index, kind)?,
            max_hit_points: read_u16(payload, index, kind)?,
            remaining_ms: read_u16(payload, index, kind)?,
        });
    }
    Ok(deployables)
}

fn decode_arena_player(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<ArenaPlayerSnapshot, PacketError> {
    let (player_id, player_name, team) = decode_arena_player_identity(payload, index, kind)?;
    let (x, y, aim_x, aim_y) = decode_arena_player_position(payload, index, kind)?;
    let resources = decode_arena_player_resources(payload, index, kind)?;

    Ok(ArenaPlayerSnapshot {
        player_id,
        player_name,
        team,
        x,
        y,
        aim_x,
        aim_y,
        hit_points: resources.hit_points,
        max_hit_points: resources.max_hit_points,
        mana: resources.mana,
        max_mana: resources.max_mana,
        alive: resources.alive,
        unlocked_skill_slots: resources.unlocked_skill_slots,
        primary_cooldown_remaining_ms: resources.primary_cooldown_remaining_ms,
        primary_cooldown_total_ms: resources.primary_cooldown_total_ms,
        slot_cooldown_remaining_ms: decode_cooldown_array(payload, index, kind)?,
        slot_cooldown_total_ms: decode_cooldown_array(payload, index, kind)?,
        current_cast_slot: read_optional_u8(payload, index, kind)?,
        current_cast_remaining_ms: read_u16(payload, index, kind)?,
        current_cast_total_ms: read_u16(payload, index, kind)?,
        active_statuses: decode_active_statuses(payload, index, kind)?,
    })
}

fn decode_arena_player_identity(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<(PlayerId, PlayerName, TeamSide), PacketError> {
    Ok((
        read_player_id(payload, index, kind)?,
        read_player_name(payload, index, kind)?,
        read_team(payload, index, kind)?,
    ))
}

fn decode_arena_player_position(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<(i16, i16, i16, i16), PacketError> {
    Ok((
        read_i16(payload, index, kind)?,
        read_i16(payload, index, kind)?,
        read_i16(payload, index, kind)?,
        read_i16(payload, index, kind)?,
    ))
}

struct ArenaPlayerResources {
    hit_points: u16,
    max_hit_points: u16,
    mana: u16,
    max_mana: u16,
    alive: bool,
    unlocked_skill_slots: u8,
    primary_cooldown_remaining_ms: u16,
    primary_cooldown_total_ms: u16,
}

fn decode_arena_player_resources(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<ArenaPlayerResources, PacketError> {
    Ok(ArenaPlayerResources {
        hit_points: read_u16(payload, index, kind)?,
        max_hit_points: read_u16(payload, index, kind)?,
        mana: read_u16(payload, index, kind)?,
        max_mana: read_u16(payload, index, kind)?,
        alive: read_bool(payload, index, kind)?,
        unlocked_skill_slots: read_u8(payload, index, kind)?,
        primary_cooldown_remaining_ms: read_u16(payload, index, kind)?,
        primary_cooldown_total_ms: read_u16(payload, index, kind)?,
    })
}

fn decode_cooldown_array(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<[u16; 5], PacketError> {
    let mut values = [0_u16; 5];
    for value in &mut values {
        *value = read_u16(payload, index, kind)?;
    }
    Ok(values)
}

fn decode_active_statuses(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<Vec<ArenaStatusSnapshot>, PacketError> {
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
    Ok(active_statuses)
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
