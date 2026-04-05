use arbitrary::Arbitrary;
use game_domain::{
    LobbyId, MatchId, MatchOutcome, PlayerId, PlayerName, PlayerRecord, ReadyState, RoundNumber,
    SkillTree, TeamSide, MAX_PLAYER_NAME_LEN, MAX_ROUNDS, MAX_SKILL_TIER,
};
use game_net::{
    ArenaEffectKind, ArenaMatchPhase, ArenaObstacleKind, ArenaStatusKind, LobbySnapshotPhase,
};

#[derive(Arbitrary, Clone, Copy, Debug)]
pub enum FuzzTeamSide {
    TeamA,
    TeamB,
}

impl FuzzTeamSide {
    pub fn into_real(self) -> TeamSide {
        match self {
            Self::TeamA => TeamSide::TeamA,
            Self::TeamB => TeamSide::TeamB,
        }
    }
}

#[derive(Arbitrary, Clone, Copy, Debug)]
pub enum FuzzSkillTree {
    Warrior,
    Rogue,
    Mage,
    Cleric,
}

impl FuzzSkillTree {
    pub fn into_real(self) -> SkillTree {
        match self {
            Self::Warrior => SkillTree::Warrior,
            Self::Rogue => SkillTree::Rogue,
            Self::Mage => SkillTree::Mage,
            Self::Cleric => SkillTree::Cleric,
        }
    }
}

#[derive(Arbitrary, Clone, Copy, Debug)]
pub enum FuzzReadyState {
    Ready,
    NotReady,
}

impl FuzzReadyState {
    pub fn into_real(self) -> ReadyState {
        match self {
            Self::Ready => ReadyState::Ready,
            Self::NotReady => ReadyState::NotReady,
        }
    }
}

#[derive(Arbitrary, Clone, Copy, Debug)]
pub enum FuzzMatchOutcome {
    TeamAWin,
    TeamBWin,
    NoContest,
}

impl FuzzMatchOutcome {
    pub fn into_real(self) -> MatchOutcome {
        match self {
            Self::TeamAWin => MatchOutcome::TeamAWin,
            Self::TeamBWin => MatchOutcome::TeamBWin,
            Self::NoContest => MatchOutcome::NoContest,
        }
    }
}

#[derive(Arbitrary, Clone, Copy, Debug)]
pub enum FuzzLobbySnapshotPhase {
    Open,
    LaunchCountdown { seconds_remaining: u8 },
}

impl FuzzLobbySnapshotPhase {
    pub fn into_real(self) -> LobbySnapshotPhase {
        match self {
            Self::Open => LobbySnapshotPhase::Open,
            Self::LaunchCountdown { seconds_remaining } => {
                LobbySnapshotPhase::LaunchCountdown { seconds_remaining }
            }
        }
    }
}

#[derive(Arbitrary, Clone, Copy, Debug)]
pub enum FuzzArenaMatchPhase {
    SkillPick,
    PreCombat,
    Combat,
    MatchEnd,
}

impl FuzzArenaMatchPhase {
    pub fn into_real(self) -> ArenaMatchPhase {
        match self {
            Self::SkillPick => ArenaMatchPhase::SkillPick,
            Self::PreCombat => ArenaMatchPhase::PreCombat,
            Self::Combat => ArenaMatchPhase::Combat,
            Self::MatchEnd => ArenaMatchPhase::MatchEnd,
        }
    }
}

#[derive(Arbitrary, Clone, Copy, Debug)]
pub enum FuzzArenaObstacleKind {
    Pillar,
    Shrub,
}

impl FuzzArenaObstacleKind {
    pub fn into_real(self) -> ArenaObstacleKind {
        match self {
            Self::Pillar => ArenaObstacleKind::Pillar,
            Self::Shrub => ArenaObstacleKind::Shrub,
        }
    }
}

#[derive(Arbitrary, Clone, Copy, Debug)]
pub enum FuzzArenaStatusKind {
    Poison,
    Hot,
    Chill,
    Root,
    Haste,
    Silence,
    Stun,
}

impl FuzzArenaStatusKind {
    pub fn into_real(self) -> ArenaStatusKind {
        match self {
            Self::Poison => ArenaStatusKind::Poison,
            Self::Hot => ArenaStatusKind::Hot,
            Self::Chill => ArenaStatusKind::Chill,
            Self::Root => ArenaStatusKind::Root,
            Self::Haste => ArenaStatusKind::Haste,
            Self::Silence => ArenaStatusKind::Silence,
            Self::Stun => ArenaStatusKind::Stun,
        }
    }
}

#[derive(Arbitrary, Clone, Copy, Debug)]
pub enum FuzzArenaEffectKind {
    MeleeSwing,
    SkillShot,
    DashTrail,
    Burst,
    Nova,
    Beam,
    HitSpark,
    Footstep,
    BrushRustle,
    StealthFootstep,
}

impl FuzzArenaEffectKind {
    pub fn into_real(self) -> ArenaEffectKind {
        match self {
            Self::MeleeSwing => ArenaEffectKind::MeleeSwing,
            Self::SkillShot => ArenaEffectKind::SkillShot,
            Self::DashTrail => ArenaEffectKind::DashTrail,
            Self::Burst => ArenaEffectKind::Burst,
            Self::Nova => ArenaEffectKind::Nova,
            Self::Beam => ArenaEffectKind::Beam,
            Self::HitSpark => ArenaEffectKind::HitSpark,
            Self::Footstep => ArenaEffectKind::Footstep,
            Self::BrushRustle => ArenaEffectKind::BrushRustle,
            Self::StealthFootstep => ArenaEffectKind::StealthFootstep,
        }
    }
}

#[derive(Arbitrary, Clone, Debug)]
pub struct FuzzPlayerRecord {
    pub wins: u16,
    pub losses: u16,
    pub no_contests: u16,
}

impl FuzzPlayerRecord {
    pub fn into_real(self) -> PlayerRecord {
        PlayerRecord {
            wins: self.wins,
            losses: self.losses,
            no_contests: self.no_contests,
            ..PlayerRecord::new()
        }
    }
}

pub fn sanitize_player_name(raw: &[u8]) -> PlayerName {
    let bytes = if raw.is_empty() {
        b"Player".as_slice()
    } else {
        raw
    };
    let mut normalized = String::with_capacity(bytes.len().min(MAX_PLAYER_NAME_LEN));
    for byte in bytes.iter().copied().take(MAX_PLAYER_NAME_LEN) {
        let mapped = match byte % 64 {
            0..=9 => char::from(b'0' + (byte % 10)),
            10..=35 => char::from(b'A' + ((byte - 10) % 26)),
            36..=61 => char::from(b'a' + ((byte - 36) % 26)),
            62 => '_',
            _ => '-',
        };
        normalized.push(mapped);
    }

    if normalized.is_empty() {
        normalized.push_str("Player");
    }

    match PlayerName::new(normalized) {
        Ok(value) => value,
        Err(_) => unreachable!("sanitized player names should always be valid"),
    }
}

pub fn sanitize_ascii_label(raw: &[u8], max_len: usize, fallback: &str) -> String {
    sanitize_string(raw, max_len, fallback, |byte| match byte % 64 {
        0..=9 => char::from(b'0' + (byte % 10)),
        10..=35 => char::from(b'A' + ((byte - 10) % 26)),
        36..=61 => char::from(b'a' + ((byte - 36) % 26)),
        62 => '_',
        _ => '-',
    })
}

pub fn sanitize_display_text(raw: &[u8], max_len: usize, fallback: &str) -> String {
    sanitize_string(raw, max_len, fallback, |byte| match byte % 67 {
        0..=9 => char::from(b'0' + (byte % 10)),
        10..=35 => char::from(b'A' + ((byte - 10) % 26)),
        36..=61 => char::from(b'a' + ((byte - 36) % 26)),
        62 => ' ',
        63 => '.',
        64 => '_',
        65 => '-',
        _ => '/',
    })
}

fn sanitize_string<F>(raw: &[u8], max_len: usize, fallback: &str, map: F) -> String
where
    F: Fn(u8) -> char,
{
    let bytes = if raw.is_empty() {
        fallback.as_bytes()
    } else {
        raw
    };
    let mut normalized = String::with_capacity(bytes.len().min(max_len));
    for byte in bytes.iter().copied().take(max_len) {
        normalized.push(map(byte));
    }
    if normalized.trim().is_empty() {
        return fallback.to_owned();
    }
    normalized
}

pub fn take_vec<T>(values: Vec<T>, max_len: usize) -> Vec<T> {
    values.into_iter().take(max_len).collect()
}

pub fn truncate_bytes(bytes: Vec<u8>, max_len: usize) -> Vec<u8> {
    take_vec(bytes, max_len)
}

pub fn normalize_player_id(raw: u32) -> PlayerId {
    match PlayerId::new(raw.max(1)) {
        Ok(value) => value,
        Err(_) => unreachable!("normalized player ids should always be valid"),
    }
}

pub fn normalize_lobby_id(raw: u32) -> LobbyId {
    match LobbyId::new(raw.max(1)) {
        Ok(value) => value,
        Err(_) => unreachable!("normalized lobby ids should always be valid"),
    }
}

pub fn normalize_match_id(raw: u32) -> MatchId {
    match MatchId::new(raw.max(1)) {
        Ok(value) => value,
        Err(_) => unreachable!("normalized match ids should always be valid"),
    }
}

pub fn normalize_round(raw: u8) -> RoundNumber {
    let round = (raw % MAX_ROUNDS).saturating_add(1);
    match RoundNumber::new(round) {
        Ok(value) => value,
        Err(_) => unreachable!("normalized rounds should always be valid"),
    }
}

pub fn normalize_skill_tier(raw: u8) -> u8 {
    (raw % MAX_SKILL_TIER).saturating_add(1)
}

pub fn normalize_cooldown_totals(remaining: [u16; 5], totals: [u16; 5]) -> [u16; 5] {
    let mut normalized = [0_u16; 5];
    for index in 0..normalized.len() {
        normalized[index] = totals[index].max(remaining[index]);
    }
    normalized
}
