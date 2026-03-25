use game_domain::{
    LobbyId, MatchId, MatchOutcome, PlayerId, PlayerName, PlayerRecord, ReadyState, RoundNumber,
    SkillTree, TeamSide,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerControlEvent {
    Connected {
        player_id: PlayerId,
        player_name: PlayerName,
        record: PlayerRecord,
        skill_catalog: Vec<SkillCatalogEntry>,
    },
    GameLobbyCreated {
        lobby_id: LobbyId,
    },
    GameLobbyJoined {
        lobby_id: LobbyId,
        player_id: PlayerId,
    },
    GameLobbyLeft {
        lobby_id: LobbyId,
        player_id: PlayerId,
    },
    TeamSelected {
        player_id: PlayerId,
        team: TeamSide,
        ready_reset: bool,
    },
    ReadyChanged {
        player_id: PlayerId,
        ready: ReadyState,
    },
    LaunchCountdownStarted {
        lobby_id: LobbyId,
        seconds_remaining: u8,
        roster_size: u16,
    },
    LaunchCountdownTick {
        lobby_id: LobbyId,
        seconds_remaining: u8,
    },
    MatchStarted {
        match_id: MatchId,
        round: RoundNumber,
        skill_pick_seconds: u8,
    },
    SkillChosen {
        player_id: PlayerId,
        tree: SkillTree,
        tier: u8,
    },
    PreCombatStarted {
        seconds_remaining: u8,
    },
    CombatStarted,
    RoundWon {
        round: RoundNumber,
        winning_team: TeamSide,
        score_a: u8,
        score_b: u8,
    },
    MatchEnded {
        outcome: MatchOutcome,
        score_a: u8,
        score_b: u8,
        message: String,
    },
    ReturnedToCentralLobby {
        record: PlayerRecord,
    },
    LobbyDirectorySnapshot {
        lobbies: Vec<LobbyDirectoryEntry>,
    },
    GameLobbySnapshot {
        lobby_id: LobbyId,
        phase: LobbySnapshotPhase,
        players: Vec<LobbySnapshotPlayer>,
    },
    ArenaStateSnapshot {
        snapshot: ArenaStateSnapshot,
    },
    ArenaDeltaSnapshot {
        snapshot: ArenaDeltaSnapshot,
    },
    ArenaEffectBatch {
        effects: Vec<ArenaEffectSnapshot>,
    },
    Error {
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillCatalogEntry {
    pub tree: SkillTree,
    pub tier: u8,
    pub skill_id: String,
    pub skill_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LobbyDirectoryEntry {
    pub lobby_id: LobbyId,
    pub player_count: u16,
    pub team_a_count: u16,
    pub team_b_count: u16,
    pub ready_count: u16,
    pub phase: LobbySnapshotPhase,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LobbySnapshotPlayer {
    pub player_id: PlayerId,
    pub player_name: PlayerName,
    pub record: PlayerRecord,
    pub team: Option<TeamSide>,
    pub ready: ReadyState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LobbySnapshotPhase {
    Open,
    LaunchCountdown { seconds_remaining: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArenaObstacleKind {
    Pillar,
    Shrub,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArenaMatchPhase {
    SkillPick,
    PreCombat,
    Combat,
    MatchEnd,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArenaStatusKind {
    Poison,
    Hot,
    Chill,
    Root,
    Haste,
    Silence,
    Stun,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArenaStatusSnapshot {
    pub source: PlayerId,
    pub slot: u8,
    pub kind: ArenaStatusKind,
    pub stacks: u8,
    pub remaining_ms: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArenaObstacleSnapshot {
    pub kind: ArenaObstacleKind,
    pub center_x: i16,
    pub center_y: i16,
    pub half_width: u16,
    pub half_height: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArenaPlayerSnapshot {
    pub player_id: PlayerId,
    pub player_name: PlayerName,
    pub team: TeamSide,
    pub x: i16,
    pub y: i16,
    pub aim_x: i16,
    pub aim_y: i16,
    pub hit_points: u16,
    pub max_hit_points: u16,
    pub mana: u16,
    pub max_mana: u16,
    pub alive: bool,
    pub unlocked_skill_slots: u8,
    pub primary_cooldown_remaining_ms: u16,
    pub primary_cooldown_total_ms: u16,
    pub slot_cooldown_remaining_ms: [u16; 5],
    pub slot_cooldown_total_ms: [u16; 5],
    pub current_cast_slot: Option<u8>,
    pub current_cast_remaining_ms: u16,
    pub current_cast_total_ms: u16,
    pub active_statuses: Vec<ArenaStatusSnapshot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArenaProjectileSnapshot {
    pub owner: PlayerId,
    pub slot: u8,
    pub kind: ArenaEffectKind,
    pub x: i16,
    pub y: i16,
    pub radius: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArenaEffectKind {
    MeleeSwing,
    SkillShot,
    DashTrail,
    Burst,
    Nova,
    Beam,
    HitSpark,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArenaEffectSnapshot {
    pub kind: ArenaEffectKind,
    pub owner: PlayerId,
    pub slot: u8,
    pub x: i16,
    pub y: i16,
    pub target_x: i16,
    pub target_y: i16,
    pub radius: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArenaStateSnapshot {
    pub phase: ArenaMatchPhase,
    pub phase_seconds_remaining: Option<u8>,
    pub width: u16,
    pub height: u16,
    pub tile_units: u16,
    pub visible_tiles: Vec<u8>,
    pub explored_tiles: Vec<u8>,
    pub obstacles: Vec<ArenaObstacleSnapshot>,
    pub players: Vec<ArenaPlayerSnapshot>,
    pub projectiles: Vec<ArenaProjectileSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArenaDeltaSnapshot {
    pub phase: ArenaMatchPhase,
    pub phase_seconds_remaining: Option<u8>,
    pub tile_units: u16,
    pub visible_tiles: Vec<u8>,
    pub explored_tiles: Vec<u8>,
    pub obstacles: Vec<ArenaObstacleSnapshot>,
    pub players: Vec<ArenaPlayerSnapshot>,
    pub projectiles: Vec<ArenaProjectileSnapshot>,
}
