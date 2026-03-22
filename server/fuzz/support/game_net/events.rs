use arbitrary::{Arbitrary, Unstructured};
use game_net::{
    ArenaDeltaSnapshot, ArenaEffectSnapshot, ArenaObstacleSnapshot, ArenaPlayerSnapshot,
    ArenaProjectileSnapshot, ArenaStateSnapshot, ArenaStatusSnapshot, LobbyDirectoryEntry,
    LobbySnapshotPlayer, ServerControlEvent, SkillCatalogEntry,
};

use super::common::{
    normalize_cooldown_totals, normalize_lobby_id, normalize_match_id, normalize_player_id,
    normalize_round, normalize_skill_tier, sanitize_ascii_label, sanitize_display_text,
    sanitize_player_name, take_vec, truncate_bytes, FuzzArenaEffectKind, FuzzArenaMatchPhase,
    FuzzArenaObstacleKind, FuzzArenaStatusKind, FuzzLobbySnapshotPhase, FuzzMatchOutcome,
    FuzzPlayerRecord, FuzzReadyState, FuzzSkillTree, FuzzTeamSide,
};

const MAX_CATALOG_ENTRIES: usize = 6;
const MAX_DIRECTORY_ENTRIES: usize = 6;
const MAX_LOBBY_PLAYERS: usize = 6;
const MAX_OBSTACLES: usize = 8;
const MAX_ARENA_PLAYERS: usize = 6;
const MAX_PROJECTILES: usize = 8;
const MAX_EFFECTS: usize = 8;
const MAX_STATUSES: usize = 4;

#[derive(Arbitrary, Clone, Debug)]
struct FuzzSkillCatalogEntry {
    tree: FuzzSkillTree,
    tier: u8,
    skill_id: Vec<u8>,
    skill_name: Vec<u8>,
}

impl FuzzSkillCatalogEntry {
    fn into_real(self) -> SkillCatalogEntry {
        SkillCatalogEntry {
            tree: self.tree.into_real(),
            tier: normalize_skill_tier(self.tier),
            skill_id: sanitize_ascii_label(&self.skill_id, 24, "skill"),
            skill_name: sanitize_display_text(&self.skill_name, 32, "Skill"),
        }
    }
}

#[derive(Arbitrary, Clone, Debug)]
struct FuzzLobbyDirectoryEntry {
    lobby_id: u32,
    player_count: u16,
    team_a_count: u16,
    team_b_count: u16,
    ready_count: u16,
    phase: FuzzLobbySnapshotPhase,
}

impl FuzzLobbyDirectoryEntry {
    fn into_real(self) -> LobbyDirectoryEntry {
        LobbyDirectoryEntry {
            lobby_id: normalize_lobby_id(self.lobby_id),
            player_count: self.player_count,
            team_a_count: self.team_a_count,
            team_b_count: self.team_b_count,
            ready_count: self.ready_count,
            phase: self.phase.into_real(),
        }
    }
}

#[derive(Arbitrary, Clone, Debug)]
struct FuzzLobbySnapshotPlayer {
    player_id: u32,
    player_name: Vec<u8>,
    record: FuzzPlayerRecord,
    team: Option<FuzzTeamSide>,
    ready: FuzzReadyState,
}

impl FuzzLobbySnapshotPlayer {
    fn into_real(self) -> LobbySnapshotPlayer {
        LobbySnapshotPlayer {
            player_id: normalize_player_id(self.player_id),
            player_name: sanitize_player_name(&self.player_name),
            record: self.record.into_real(),
            team: self.team.map(FuzzTeamSide::into_real),
            ready: self.ready.into_real(),
        }
    }
}

#[derive(Arbitrary, Clone, Debug)]
struct FuzzArenaStatusSnapshot {
    source: u32,
    slot: u8,
    kind: FuzzArenaStatusKind,
    stacks: u8,
    remaining_ms: u16,
}

impl FuzzArenaStatusSnapshot {
    fn into_real(self) -> ArenaStatusSnapshot {
        ArenaStatusSnapshot {
            source: normalize_player_id(self.source),
            slot: self.slot % 5,
            kind: self.kind.into_real(),
            stacks: self.stacks.max(1),
            remaining_ms: self.remaining_ms,
        }
    }
}

#[derive(Arbitrary, Clone, Debug)]
struct FuzzArenaObstacleSnapshot {
    kind: FuzzArenaObstacleKind,
    center_x: i16,
    center_y: i16,
    half_width: u16,
    half_height: u16,
}

impl FuzzArenaObstacleSnapshot {
    fn into_real(self) -> ArenaObstacleSnapshot {
        ArenaObstacleSnapshot {
            kind: self.kind.into_real(),
            center_x: self.center_x,
            center_y: self.center_y,
            half_width: self.half_width.max(1),
            half_height: self.half_height.max(1),
        }
    }
}

#[derive(Arbitrary, Clone, Debug)]
struct FuzzArenaPlayerSnapshot {
    player_id: u32,
    player_name: Vec<u8>,
    team: FuzzTeamSide,
    x: i16,
    y: i16,
    aim_x: i16,
    aim_y: i16,
    hit_points: u16,
    max_hit_points: u16,
    mana: u16,
    max_mana: u16,
    alive: bool,
    unlocked_skill_slots: u8,
    primary_cooldown_remaining_ms: u16,
    primary_cooldown_total_ms: u16,
    slot_cooldown_remaining_ms: [u16; 5],
    slot_cooldown_total_ms: [u16; 5],
    active_statuses: Vec<FuzzArenaStatusSnapshot>,
}

impl FuzzArenaPlayerSnapshot {
    fn into_real(self) -> ArenaPlayerSnapshot {
        ArenaPlayerSnapshot {
            player_id: normalize_player_id(self.player_id),
            player_name: sanitize_player_name(&self.player_name),
            team: self.team.into_real(),
            x: self.x,
            y: self.y,
            aim_x: self.aim_x,
            aim_y: self.aim_y,
            hit_points: self.hit_points,
            max_hit_points: self.max_hit_points.max(self.hit_points.max(1)),
            mana: self.mana,
            max_mana: self.max_mana.max(self.mana.max(1)),
            alive: self.alive,
            unlocked_skill_slots: self.unlocked_skill_slots.min(5),
            primary_cooldown_remaining_ms: self.primary_cooldown_remaining_ms,
            primary_cooldown_total_ms: self
                .primary_cooldown_total_ms
                .max(self.primary_cooldown_remaining_ms),
            slot_cooldown_remaining_ms: self.slot_cooldown_remaining_ms,
            slot_cooldown_total_ms: normalize_cooldown_totals(
                self.slot_cooldown_remaining_ms,
                self.slot_cooldown_total_ms,
            ),
            active_statuses: take_vec(self.active_statuses, MAX_STATUSES)
                .into_iter()
                .map(FuzzArenaStatusSnapshot::into_real)
                .collect(),
        }
    }
}

#[derive(Arbitrary, Clone, Debug)]
struct FuzzArenaProjectileSnapshot {
    owner: u32,
    slot: u8,
    kind: FuzzArenaEffectKind,
    x: i16,
    y: i16,
    radius: u16,
}

impl FuzzArenaProjectileSnapshot {
    fn into_real(self) -> ArenaProjectileSnapshot {
        ArenaProjectileSnapshot {
            owner: normalize_player_id(self.owner),
            slot: self.slot % 5,
            kind: self.kind.into_real(),
            x: self.x,
            y: self.y,
            radius: self.radius.max(1),
        }
    }
}

#[derive(Arbitrary, Clone, Debug)]
struct FuzzArenaEffectSnapshot {
    kind: FuzzArenaEffectKind,
    owner: u32,
    slot: u8,
    x: i16,
    y: i16,
    target_x: i16,
    target_y: i16,
    radius: u16,
}

impl FuzzArenaEffectSnapshot {
    fn into_real(self) -> ArenaEffectSnapshot {
        ArenaEffectSnapshot {
            kind: self.kind.into_real(),
            owner: normalize_player_id(self.owner),
            slot: self.slot % 5,
            x: self.x,
            y: self.y,
            target_x: self.target_x,
            target_y: self.target_y,
            radius: self.radius.max(1),
        }
    }
}

#[derive(Arbitrary, Clone, Debug)]
struct FuzzArenaStateSnapshot {
    phase: FuzzArenaMatchPhase,
    phase_seconds_remaining: Option<u8>,
    width: u16,
    height: u16,
    tile_units: u16,
    visible_tiles: Vec<u8>,
    explored_tiles: Vec<u8>,
    obstacles: Vec<FuzzArenaObstacleSnapshot>,
    players: Vec<FuzzArenaPlayerSnapshot>,
    projectiles: Vec<FuzzArenaProjectileSnapshot>,
}

impl FuzzArenaStateSnapshot {
    fn into_real(self) -> ArenaStateSnapshot {
        ArenaStateSnapshot {
            phase: self.phase.into_real(),
            phase_seconds_remaining: self.phase_seconds_remaining,
            width: self.width.max(1),
            height: self.height.max(1),
            tile_units: self.tile_units.max(1),
            visible_tiles: truncate_bytes(self.visible_tiles, 16),
            explored_tiles: truncate_bytes(self.explored_tiles, 16),
            obstacles: take_vec(self.obstacles, MAX_OBSTACLES)
                .into_iter()
                .map(FuzzArenaObstacleSnapshot::into_real)
                .collect(),
            players: take_vec(self.players, MAX_ARENA_PLAYERS)
                .into_iter()
                .map(FuzzArenaPlayerSnapshot::into_real)
                .collect(),
            projectiles: take_vec(self.projectiles, MAX_PROJECTILES)
                .into_iter()
                .map(FuzzArenaProjectileSnapshot::into_real)
                .collect(),
        }
    }
}

#[derive(Arbitrary, Clone, Debug)]
struct FuzzArenaDeltaSnapshot {
    phase: FuzzArenaMatchPhase,
    phase_seconds_remaining: Option<u8>,
    tile_units: u16,
    visible_tiles: Vec<u8>,
    explored_tiles: Vec<u8>,
    obstacles: Vec<FuzzArenaObstacleSnapshot>,
    players: Vec<FuzzArenaPlayerSnapshot>,
    projectiles: Vec<FuzzArenaProjectileSnapshot>,
}

impl FuzzArenaDeltaSnapshot {
    fn into_real(self) -> ArenaDeltaSnapshot {
        ArenaDeltaSnapshot {
            phase: self.phase.into_real(),
            phase_seconds_remaining: self.phase_seconds_remaining,
            tile_units: self.tile_units.max(1),
            visible_tiles: truncate_bytes(self.visible_tiles, 16),
            explored_tiles: truncate_bytes(self.explored_tiles, 16),
            obstacles: take_vec(self.obstacles, MAX_OBSTACLES)
                .into_iter()
                .map(FuzzArenaObstacleSnapshot::into_real)
                .collect(),
            players: take_vec(self.players, MAX_ARENA_PLAYERS)
                .into_iter()
                .map(FuzzArenaPlayerSnapshot::into_real)
                .collect(),
            projectiles: take_vec(self.projectiles, MAX_PROJECTILES)
                .into_iter()
                .map(FuzzArenaProjectileSnapshot::into_real)
                .collect(),
        }
    }
}

#[derive(Arbitrary, Clone, Debug)]
enum FuzzServerControlEvent {
    Connected {
        player_id: u32,
        player_name: Vec<u8>,
        record: FuzzPlayerRecord,
        skill_catalog: Vec<FuzzSkillCatalogEntry>,
    },
    GameLobbyCreated {
        lobby_id: u32,
    },
    GameLobbyJoined {
        lobby_id: u32,
        player_id: u32,
    },
    GameLobbyLeft {
        lobby_id: u32,
        player_id: u32,
    },
    TeamSelected {
        player_id: u32,
        team: FuzzTeamSide,
        ready_reset: bool,
    },
    ReadyChanged {
        player_id: u32,
        ready: FuzzReadyState,
    },
    LaunchCountdownStarted {
        lobby_id: u32,
        seconds_remaining: u8,
        roster_size: u16,
    },
    LaunchCountdownTick {
        lobby_id: u32,
        seconds_remaining: u8,
    },
    MatchStarted {
        match_id: u32,
        round: u8,
        skill_pick_seconds: u8,
    },
    SkillChosen {
        player_id: u32,
        tree: FuzzSkillTree,
        tier: u8,
    },
    PreCombatStarted {
        seconds_remaining: u8,
    },
    CombatStarted,
    RoundWon {
        round: u8,
        winning_team: FuzzTeamSide,
        score_a: u8,
        score_b: u8,
    },
    MatchEnded {
        outcome: FuzzMatchOutcome,
        score_a: u8,
        score_b: u8,
        message: Vec<u8>,
    },
    ReturnedToCentralLobby {
        record: FuzzPlayerRecord,
    },
    LobbyDirectorySnapshot {
        lobbies: Vec<FuzzLobbyDirectoryEntry>,
    },
    GameLobbySnapshot {
        lobby_id: u32,
        phase: FuzzLobbySnapshotPhase,
        players: Vec<FuzzLobbySnapshotPlayer>,
    },
    ArenaStateSnapshot {
        snapshot: FuzzArenaStateSnapshot,
    },
    ArenaDeltaSnapshot {
        snapshot: FuzzArenaDeltaSnapshot,
    },
    ArenaEffectBatch {
        effects: Vec<FuzzArenaEffectSnapshot>,
    },
    Error {
        message: Vec<u8>,
    },
}

impl FuzzServerControlEvent {
    fn into_real(self) -> ServerControlEvent {
        match self {
            Self::Connected {
                player_id,
                player_name,
                record,
                skill_catalog,
            } => build_connected_event(player_id, &player_name, record, skill_catalog),
            Self::GameLobbyCreated { lobby_id } => ServerControlEvent::GameLobbyCreated {
                lobby_id: normalize_lobby_id(lobby_id),
            },
            Self::GameLobbyJoined {
                lobby_id,
                player_id,
            } => ServerControlEvent::GameLobbyJoined {
                lobby_id: normalize_lobby_id(lobby_id),
                player_id: normalize_player_id(player_id),
            },
            Self::GameLobbyLeft {
                lobby_id,
                player_id,
            } => ServerControlEvent::GameLobbyLeft {
                lobby_id: normalize_lobby_id(lobby_id),
                player_id: normalize_player_id(player_id),
            },
            Self::TeamSelected {
                player_id,
                team,
                ready_reset,
            } => build_team_selected_event(player_id, team, ready_reset),
            Self::ReadyChanged { player_id, ready } => ServerControlEvent::ReadyChanged {
                player_id: normalize_player_id(player_id),
                ready: ready.into_real(),
            },
            Self::LaunchCountdownStarted {
                lobby_id,
                seconds_remaining,
                roster_size,
            } => build_launch_countdown_started_event(lobby_id, seconds_remaining, roster_size),
            Self::LaunchCountdownTick {
                lobby_id,
                seconds_remaining,
            } => ServerControlEvent::LaunchCountdownTick {
                lobby_id: normalize_lobby_id(lobby_id),
                seconds_remaining,
            },
            Self::MatchStarted {
                match_id,
                round,
                skill_pick_seconds,
            } => build_match_started_event(match_id, round, skill_pick_seconds),
            Self::SkillChosen {
                player_id,
                tree,
                tier,
            } => ServerControlEvent::SkillChosen {
                player_id: normalize_player_id(player_id),
                tree: tree.into_real(),
                tier: normalize_skill_tier(tier),
            },
            Self::PreCombatStarted { seconds_remaining } => {
                ServerControlEvent::PreCombatStarted { seconds_remaining }
            }
            Self::CombatStarted => ServerControlEvent::CombatStarted,
            Self::RoundWon {
                round,
                winning_team,
                score_a,
                score_b,
            } => build_round_won_event(round, winning_team, score_a, score_b),
            Self::MatchEnded {
                outcome,
                score_a,
                score_b,
                message,
            } => build_match_ended_event(outcome, score_a, score_b, &message),
            Self::ReturnedToCentralLobby { record } => ServerControlEvent::ReturnedToCentralLobby {
                record: record.into_real(),
            },
            Self::LobbyDirectorySnapshot { lobbies } => build_lobby_directory_snapshot(lobbies),
            Self::GameLobbySnapshot {
                lobby_id,
                phase,
                players,
            } => build_game_lobby_snapshot(lobby_id, phase, players),
            Self::ArenaStateSnapshot { snapshot } => ServerControlEvent::ArenaStateSnapshot {
                snapshot: snapshot.into_real(),
            },
            Self::ArenaDeltaSnapshot { snapshot } => ServerControlEvent::ArenaDeltaSnapshot {
                snapshot: snapshot.into_real(),
            },
            Self::ArenaEffectBatch { effects } => build_arena_effect_batch(effects),
            Self::Error { message } => ServerControlEvent::Error {
                message: sanitize_display_text(&message, 48, "error"),
            },
        }
    }
}

fn build_connected_event(
    player_id: u32,
    player_name: &[u8],
    record: FuzzPlayerRecord,
    skill_catalog: Vec<FuzzSkillCatalogEntry>,
) -> ServerControlEvent {
    ServerControlEvent::Connected {
        player_id: normalize_player_id(player_id),
        player_name: sanitize_player_name(player_name),
        record: record.into_real(),
        skill_catalog: take_vec(skill_catalog, MAX_CATALOG_ENTRIES)
            .into_iter()
            .map(FuzzSkillCatalogEntry::into_real)
            .collect(),
    }
}

fn build_team_selected_event(
    player_id: u32,
    team: FuzzTeamSide,
    ready_reset: bool,
) -> ServerControlEvent {
    ServerControlEvent::TeamSelected {
        player_id: normalize_player_id(player_id),
        team: team.into_real(),
        ready_reset,
    }
}

fn build_launch_countdown_started_event(
    lobby_id: u32,
    seconds_remaining: u8,
    roster_size: u16,
) -> ServerControlEvent {
    ServerControlEvent::LaunchCountdownStarted {
        lobby_id: normalize_lobby_id(lobby_id),
        seconds_remaining,
        roster_size,
    }
}

fn build_match_started_event(
    match_id: u32,
    round: u8,
    skill_pick_seconds: u8,
) -> ServerControlEvent {
    ServerControlEvent::MatchStarted {
        match_id: normalize_match_id(match_id),
        round: normalize_round(round),
        skill_pick_seconds,
    }
}

fn build_round_won_event(
    round: u8,
    winning_team: FuzzTeamSide,
    score_a: u8,
    score_b: u8,
) -> ServerControlEvent {
    ServerControlEvent::RoundWon {
        round: normalize_round(round),
        winning_team: winning_team.into_real(),
        score_a,
        score_b,
    }
}

fn build_match_ended_event(
    outcome: FuzzMatchOutcome,
    score_a: u8,
    score_b: u8,
    message: &[u8],
) -> ServerControlEvent {
    ServerControlEvent::MatchEnded {
        outcome: outcome.into_real(),
        score_a,
        score_b,
        message: sanitize_display_text(message, 48, "match ended"),
    }
}

fn build_lobby_directory_snapshot(lobbies: Vec<FuzzLobbyDirectoryEntry>) -> ServerControlEvent {
    ServerControlEvent::LobbyDirectorySnapshot {
        lobbies: take_vec(lobbies, MAX_DIRECTORY_ENTRIES)
            .into_iter()
            .map(FuzzLobbyDirectoryEntry::into_real)
            .collect(),
    }
}

fn build_game_lobby_snapshot(
    lobby_id: u32,
    phase: FuzzLobbySnapshotPhase,
    players: Vec<FuzzLobbySnapshotPlayer>,
) -> ServerControlEvent {
    ServerControlEvent::GameLobbySnapshot {
        lobby_id: normalize_lobby_id(lobby_id),
        phase: phase.into_real(),
        players: take_vec(players, MAX_LOBBY_PLAYERS)
            .into_iter()
            .map(FuzzLobbySnapshotPlayer::into_real)
            .collect(),
    }
}

fn build_arena_effect_batch(effects: Vec<FuzzArenaEffectSnapshot>) -> ServerControlEvent {
    ServerControlEvent::ArenaEffectBatch {
        effects: take_vec(effects, MAX_EFFECTS)
            .into_iter()
            .map(FuzzArenaEffectSnapshot::into_real)
            .collect(),
    }
}

#[derive(Arbitrary, Clone, Debug)]
struct FuzzServerEventEnvelope {
    event: FuzzServerControlEvent,
    seq: u32,
    sim_tick: u32,
}

#[derive(Arbitrary, Clone, Debug)]
struct FuzzArenaStateEnvelope {
    snapshot: FuzzArenaStateSnapshot,
    seq: u32,
    sim_tick: u32,
}

#[derive(Arbitrary, Clone, Debug)]
struct FuzzArenaDeltaEnvelope {
    snapshot: FuzzArenaDeltaSnapshot,
    seq: u32,
    sim_tick: u32,
}

pub fn run_server_control_event_roundtrip(bytes: &[u8]) {
    let Some(envelope) = parse_input::<FuzzServerEventEnvelope>(bytes) else {
        return;
    };
    let event = envelope.event.into_real();
    run_server_event_roundtrip(&event, envelope.seq, envelope.sim_tick);
}

pub fn run_arena_full_snapshot_roundtrip(bytes: &[u8]) {
    let Some(envelope) = parse_input::<FuzzArenaStateEnvelope>(bytes) else {
        return;
    };
    let event = ServerControlEvent::ArenaStateSnapshot {
        snapshot: envelope.snapshot.into_real(),
    };
    run_server_event_roundtrip(&event, envelope.seq, envelope.sim_tick);
}

pub fn run_arena_delta_snapshot_roundtrip(bytes: &[u8]) {
    let Some(envelope) = parse_input::<FuzzArenaDeltaEnvelope>(bytes) else {
        return;
    };
    let event = ServerControlEvent::ArenaDeltaSnapshot {
        snapshot: envelope.snapshot.into_real(),
    };
    run_server_event_roundtrip(&event, envelope.seq, envelope.sim_tick);
}

fn run_server_event_roundtrip(event: &ServerControlEvent, seq: u32, sim_tick: u32) {
    let Ok(packet) = event.clone().encode_packet(seq, sim_tick) else {
        return;
    };
    let Ok((header, decoded)) = ServerControlEvent::decode_packet(&packet) else {
        panic!("encoded server event should decode");
    };

    assert_eq!(decoded, *event);
    assert_eq!(header.seq, seq);
    assert_eq!(header.sim_tick, sim_tick);
}

fn parse_input<T>(bytes: &[u8]) -> Option<T>
where
    T: for<'a> Arbitrary<'a>,
{
    let mut unstructured = Unstructured::new(bytes);
    T::arbitrary(&mut unstructured).ok()
}
