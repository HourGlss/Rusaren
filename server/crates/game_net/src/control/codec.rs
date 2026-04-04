use game_domain::{
    LobbyId, MatchId, MatchOutcome, PlayerId, PlayerName, PlayerRecord, ReadyState, RoundNumber,
    SkillTree, TeamSide,
};
use std::collections::BTreeMap;

use crate::PacketError;

use super::{
    ArenaCombatTextStyle, ArenaDeployableKind, ArenaEffectKind, ArenaMatchPhase, ArenaObstacleKind,
    ArenaSessionMode, ArenaStatusKind, LobbySnapshotPhase, SkillCatalogEntry,
    MAX_SKILL_AUDIO_CUE_BYTES, MAX_SKILL_DESCRIPTION_BYTES, MAX_SKILL_ID_BYTES,
    MAX_SKILL_NAME_BYTES, MAX_SKILL_SUMMARY_BYTES, MAX_SKILL_TREE_NAME_BYTES,
    MAX_SKILL_UI_CATEGORY_BYTES,
};

pub(super) fn encode_bytes(
    payload: &mut Vec<u8>,
    field: &'static str,
    bytes: &[u8],
) -> Result<(), PacketError> {
    let byte_len = u16::try_from(bytes.len()).map_err(|_| PacketError::PayloadTooLarge {
        actual: bytes.len(),
        maximum: usize::from(u16::MAX),
    })?;
    let _ = field;
    payload.extend_from_slice(&byte_len.to_le_bytes());
    payload.extend_from_slice(bytes);
    Ok(())
}

pub(super) fn decode_bytes(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
    field: &'static str,
) -> Result<Vec<u8>, PacketError> {
    let byte_len = usize::from(read_u16(payload, index, kind)?);
    ensure_available(payload, *index, byte_len, kind)?;
    let bytes = payload[*index..*index + byte_len].to_vec();
    *index += byte_len;
    let _ = field;
    Ok(bytes)
}

pub(super) fn read_u8(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<u8, PacketError> {
    ensure_available(payload, *index, 1, kind)?;
    let value = payload[*index];
    *index += 1;
    Ok(value)
}

pub(super) fn read_i16(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<i16, PacketError> {
    ensure_available(payload, *index, 2, kind)?;
    let value = i16::from_le_bytes([payload[*index], payload[*index + 1]]);
    *index += 2;
    Ok(value)
}

pub(super) fn read_u16(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<u16, PacketError> {
    ensure_available(payload, *index, 2, kind)?;
    let value = u16::from_le_bytes([payload[*index], payload[*index + 1]]);
    *index += 2;
    Ok(value)
}

pub(super) fn read_u32(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<u32, PacketError> {
    ensure_available(payload, *index, 4, kind)?;
    let value = u32::from_le_bytes([
        payload[*index],
        payload[*index + 1],
        payload[*index + 2],
        payload[*index + 3],
    ]);
    *index += 4;
    Ok(value)
}

pub(super) fn read_bool(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<bool, PacketError> {
    match read_u8(payload, index, kind)? {
        0 => Ok(false),
        1 => Ok(true),
        other => Err(PacketError::InvalidEncodedBoolean(other)),
    }
}

pub(super) fn read_optional_u8(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<Option<u8>, PacketError> {
    if read_bool(payload, index, kind)? {
        Ok(Some(read_u8(payload, index, kind)?))
    } else {
        Ok(None)
    }
}

pub(super) fn read_player_id(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<PlayerId, PacketError> {
    let raw = read_u32(payload, index, kind)?;
    PlayerId::new(raw).map_err(|_| PacketError::InvalidEncodedPlayerId(raw))
}

pub(super) fn read_lobby_id(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<LobbyId, PacketError> {
    let raw = read_u32(payload, index, kind)?;
    LobbyId::new(raw).map_err(|_| PacketError::InvalidEncodedLobbyId(raw))
}

pub(super) fn read_match_id(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<MatchId, PacketError> {
    let raw = read_u32(payload, index, kind)?;
    MatchId::new(raw).map_err(|_| PacketError::InvalidEncodedMatchId(raw))
}

pub(super) fn read_round(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<RoundNumber, PacketError> {
    let raw = read_u8(payload, index, kind)?;
    RoundNumber::new(raw).map_err(|_| PacketError::InvalidEncodedRound(raw))
}

pub(super) fn read_player_name(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<PlayerName, PacketError> {
    let name = read_string(
        payload,
        index,
        kind,
        "player_name",
        game_domain::MAX_PLAYER_NAME_LEN,
    )?;
    PlayerName::new(name).map_err(PacketError::InvalidEncodedPlayerName)
}

pub(super) fn read_string(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
    field: &'static str,
    max_len: usize,
) -> Result<String, PacketError> {
    let len = usize::from(read_u8(payload, index, kind)?);
    if len > max_len {
        return Err(PacketError::StringLengthOutOfBounds {
            field,
            len,
            max: max_len,
        });
    }

    ensure_available(payload, *index, len, kind)?;
    let bytes = &payload[*index..*index + len];
    *index += len;

    String::from_utf8(bytes.to_vec()).map_err(|_| PacketError::InvalidUtf8String { field })
}

pub(super) fn read_player_record(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<PlayerRecord, PacketError> {
    let wins = read_u16(payload, index, kind)?;
    let losses = read_u16(payload, index, kind)?;
    let no_contests = read_u16(payload, index, kind)?;
    let round_wins = read_u16(payload, index, kind)?;
    let round_losses = read_u16(payload, index, kind)?;
    let total_damage_done = read_u32(payload, index, kind)?;
    let total_healing_done = read_u32(payload, index, kind)?;
    let total_combat_ms = read_u32(payload, index, kind)?;
    let cc_used = read_u16(payload, index, kind)?;
    let cc_hits = read_u16(payload, index, kind)?;
    let skill_pick_count = usize::from(read_u16(payload, index, kind)?);
    let mut skill_pick_counts = BTreeMap::new();
    for _ in 0..skill_pick_count {
        let skill_id = read_string(payload, index, kind, "skill_id", super::MAX_SKILL_ID_BYTES)?;
        let count = read_u16(payload, index, kind)?;
        skill_pick_counts.insert(skill_id, count);
    }
    Ok(PlayerRecord {
        wins,
        losses,
        no_contests,
        round_wins,
        round_losses,
        total_damage_done,
        total_healing_done,
        total_combat_ms,
        cc_used,
        cc_hits,
        skill_pick_counts,
    })
}

pub(super) fn decode_skill_catalog(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<Vec<SkillCatalogEntry>, PacketError> {
    let entry_count = usize::from(read_u16(payload, index, kind)?);
    let mut entries = Vec::with_capacity(entry_count);
    for _ in 0..entry_count {
        entries.push(SkillCatalogEntry {
            tree: read_skill_tree(payload, index, kind)?,
            tier: read_u8(payload, index, kind)?,
            skill_id: read_string(payload, index, kind, "skill_id", MAX_SKILL_ID_BYTES)?,
            skill_name: read_string(payload, index, kind, "skill_name", MAX_SKILL_NAME_BYTES)?,
            skill_description: read_string(
                payload,
                index,
                kind,
                "skill_description",
                MAX_SKILL_DESCRIPTION_BYTES,
            )?,
            skill_summary: read_string(
                payload,
                index,
                kind,
                "skill_summary",
                MAX_SKILL_SUMMARY_BYTES,
            )?,
            ui_category: read_string(
                payload,
                index,
                kind,
                "ui_category",
                MAX_SKILL_UI_CATEGORY_BYTES,
            )?,
            audio_cue_id: read_string(
                payload,
                index,
                kind,
                "audio_cue_id",
                MAX_SKILL_AUDIO_CUE_BYTES,
            )?,
        });
    }
    Ok(entries)
}

pub(super) fn read_team(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<TeamSide, PacketError> {
    match read_u8(payload, index, kind)? {
        1 => Ok(TeamSide::TeamA),
        2 => Ok(TeamSide::TeamB),
        other => Err(PacketError::InvalidEncodedTeam(other)),
    }
}

pub(super) fn read_optional_team(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<Option<TeamSide>, PacketError> {
    match read_u8(payload, index, kind)? {
        0 => Ok(None),
        1 => Ok(Some(TeamSide::TeamA)),
        2 => Ok(Some(TeamSide::TeamB)),
        other => Err(PacketError::InvalidEncodedOptionalTeam(other)),
    }
}

pub(super) fn read_ready_state(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<ReadyState, PacketError> {
    match read_u8(payload, index, kind)? {
        0 => Ok(ReadyState::NotReady),
        1 => Ok(ReadyState::Ready),
        other => Err(PacketError::InvalidEncodedReadyState(other)),
    }
}

pub(super) fn read_skill_tree(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<SkillTree, PacketError> {
    let raw = read_string(
        payload,
        index,
        kind,
        "skill_tree",
        MAX_SKILL_TREE_NAME_BYTES,
    )?;
    SkillTree::new(raw).map_err(PacketError::InvalidEncodedSkillTree)
}

pub(super) fn read_match_outcome(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<MatchOutcome, PacketError> {
    match read_u8(payload, index, kind)? {
        1 => Ok(MatchOutcome::TeamAWin),
        2 => Ok(MatchOutcome::TeamBWin),
        3 => Ok(MatchOutcome::NoContest),
        other => Err(PacketError::InvalidEncodedMatchOutcome(other)),
    }
}

pub(super) fn read_lobby_snapshot_phase(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<LobbySnapshotPhase, PacketError> {
    match read_u8(payload, index, kind)? {
        0 => Ok(LobbySnapshotPhase::Open),
        1 => Ok(LobbySnapshotPhase::LaunchCountdown {
            seconds_remaining: read_u8(payload, index, kind)?,
        }),
        other => Err(PacketError::InvalidEncodedLobbyPhase(other)),
    }
}

pub(super) fn read_arena_match_phase(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<ArenaMatchPhase, PacketError> {
    match read_u8(payload, index, kind)? {
        1 => Ok(ArenaMatchPhase::SkillPick),
        2 => Ok(ArenaMatchPhase::PreCombat),
        3 => Ok(ArenaMatchPhase::Combat),
        4 => Ok(ArenaMatchPhase::MatchEnd),
        other => Err(PacketError::InvalidEncodedArenaMatchPhase(other)),
    }
}

pub(super) fn read_arena_session_mode(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<ArenaSessionMode, PacketError> {
    match read_u8(payload, index, kind)? {
        1 => Ok(ArenaSessionMode::Match),
        2 => Ok(ArenaSessionMode::Training),
        other => Err(PacketError::InvalidEncodedArenaMatchPhase(other)),
    }
}

pub(super) fn read_arena_status_kind(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<ArenaStatusKind, PacketError> {
    match read_u8(payload, index, kind)? {
        1 => Ok(ArenaStatusKind::Poison),
        2 => Ok(ArenaStatusKind::Hot),
        3 => Ok(ArenaStatusKind::Chill),
        4 => Ok(ArenaStatusKind::Root),
        5 => Ok(ArenaStatusKind::Haste),
        6 => Ok(ArenaStatusKind::Silence),
        7 => Ok(ArenaStatusKind::Stun),
        8 => Ok(ArenaStatusKind::Sleep),
        9 => Ok(ArenaStatusKind::Shield),
        10 => Ok(ArenaStatusKind::Stealth),
        11 => Ok(ArenaStatusKind::Reveal),
        12 => Ok(ArenaStatusKind::Fear),
        other => Err(PacketError::InvalidEncodedArenaStatusKind(other)),
    }
}

pub(super) fn encode_team(team: TeamSide) -> u8 {
    match team {
        TeamSide::TeamA => 1,
        TeamSide::TeamB => 2,
    }
}

pub(super) fn encode_optional_team(team: Option<TeamSide>) -> u8 {
    match team {
        None => 0,
        Some(TeamSide::TeamA) => 1,
        Some(TeamSide::TeamB) => 2,
    }
}

pub(super) fn encode_ready_state(ready: ReadyState) -> u8 {
    match ready {
        ReadyState::NotReady => 0,
        ReadyState::Ready => 1,
    }
}

pub(super) fn encode_match_outcome(outcome: MatchOutcome) -> u8 {
    match outcome {
        MatchOutcome::TeamAWin => 1,
        MatchOutcome::TeamBWin => 2,
        MatchOutcome::NoContest => 3,
    }
}

pub(super) fn encode_arena_obstacle_kind(kind: ArenaObstacleKind) -> u8 {
    match kind {
        ArenaObstacleKind::Pillar => 1,
        ArenaObstacleKind::Shrub => 2,
        ArenaObstacleKind::Barrier => 3,
    }
}

pub(super) fn encode_arena_match_phase(phase: ArenaMatchPhase) -> u8 {
    match phase {
        ArenaMatchPhase::SkillPick => 1,
        ArenaMatchPhase::PreCombat => 2,
        ArenaMatchPhase::Combat => 3,
        ArenaMatchPhase::MatchEnd => 4,
    }
}

pub(super) fn encode_arena_session_mode(mode: ArenaSessionMode) -> u8 {
    match mode {
        ArenaSessionMode::Match => 1,
        ArenaSessionMode::Training => 2,
    }
}

pub(super) fn encode_arena_status_kind(kind: ArenaStatusKind) -> u8 {
    match kind {
        ArenaStatusKind::Poison => 1,
        ArenaStatusKind::Hot => 2,
        ArenaStatusKind::Chill => 3,
        ArenaStatusKind::Root => 4,
        ArenaStatusKind::Haste => 5,
        ArenaStatusKind::Silence => 6,
        ArenaStatusKind::Stun => 7,
        ArenaStatusKind::Sleep => 8,
        ArenaStatusKind::Shield => 9,
        ArenaStatusKind::Stealth => 10,
        ArenaStatusKind::Reveal => 11,
        ArenaStatusKind::Fear => 12,
    }
}

pub(super) fn encode_arena_deployable_kind(kind: ArenaDeployableKind) -> u8 {
    match kind {
        ArenaDeployableKind::Summon => 1,
        ArenaDeployableKind::Ward => 2,
        ArenaDeployableKind::Trap => 3,
        ArenaDeployableKind::Barrier => 4,
        ArenaDeployableKind::Aura => 5,
        ArenaDeployableKind::TrainingDummyResetFull => 6,
        ArenaDeployableKind::TrainingDummyExecute => 7,
    }
}

pub(super) fn encode_arena_effect_kind(kind: ArenaEffectKind) -> u8 {
    match kind {
        ArenaEffectKind::MeleeSwing => 1,
        ArenaEffectKind::SkillShot => 2,
        ArenaEffectKind::DashTrail => 3,
        ArenaEffectKind::Burst => 4,
        ArenaEffectKind::Nova => 5,
        ArenaEffectKind::Beam => 6,
        ArenaEffectKind::HitSpark => 7,
    }
}

pub(super) fn encode_arena_combat_text_style(style: ArenaCombatTextStyle) -> u8 {
    match style {
        ArenaCombatTextStyle::DamageOutgoing => 1,
        ArenaCombatTextStyle::DamageIncoming => 2,
        ArenaCombatTextStyle::HealOutgoing => 3,
        ArenaCombatTextStyle::HealIncoming => 4,
        ArenaCombatTextStyle::PositiveStatus => 5,
        ArenaCombatTextStyle::NegativeStatus => 6,
        ArenaCombatTextStyle::Utility => 7,
    }
}

pub(super) fn read_arena_obstacle_kind(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<ArenaObstacleKind, PacketError> {
    match read_u8(payload, index, kind)? {
        1 => Ok(ArenaObstacleKind::Pillar),
        2 => Ok(ArenaObstacleKind::Shrub),
        3 => Ok(ArenaObstacleKind::Barrier),
        other => Err(PacketError::InvalidEncodedArenaObstacleKind(other)),
    }
}

pub(super) fn read_arena_deployable_kind(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<ArenaDeployableKind, PacketError> {
    match read_u8(payload, index, kind)? {
        1 => Ok(ArenaDeployableKind::Summon),
        2 => Ok(ArenaDeployableKind::Ward),
        3 => Ok(ArenaDeployableKind::Trap),
        4 => Ok(ArenaDeployableKind::Barrier),
        5 => Ok(ArenaDeployableKind::Aura),
        6 => Ok(ArenaDeployableKind::TrainingDummyResetFull),
        7 => Ok(ArenaDeployableKind::TrainingDummyExecute),
        other => Err(PacketError::InvalidEncodedArenaEffectKind(other)),
    }
}

pub(super) fn read_arena_effect_kind(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<ArenaEffectKind, PacketError> {
    match read_u8(payload, index, kind)? {
        1 => Ok(ArenaEffectKind::MeleeSwing),
        2 => Ok(ArenaEffectKind::SkillShot),
        3 => Ok(ArenaEffectKind::DashTrail),
        4 => Ok(ArenaEffectKind::Burst),
        5 => Ok(ArenaEffectKind::Nova),
        6 => Ok(ArenaEffectKind::Beam),
        7 => Ok(ArenaEffectKind::HitSpark),
        other => Err(PacketError::InvalidEncodedArenaEffectKind(other)),
    }
}

pub(super) fn read_arena_combat_text_style(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<ArenaCombatTextStyle, PacketError> {
    match read_u8(payload, index, kind)? {
        1 => Ok(ArenaCombatTextStyle::DamageOutgoing),
        2 => Ok(ArenaCombatTextStyle::DamageIncoming),
        3 => Ok(ArenaCombatTextStyle::HealOutgoing),
        4 => Ok(ArenaCombatTextStyle::HealIncoming),
        5 => Ok(ArenaCombatTextStyle::PositiveStatus),
        6 => Ok(ArenaCombatTextStyle::NegativeStatus),
        7 => Ok(ArenaCombatTextStyle::Utility),
        other => Err(PacketError::InvalidEncodedArenaCombatTextStyle(other)),
    }
}

pub(super) fn encode_lobby_snapshot_phase(payload: &mut Vec<u8>, phase: LobbySnapshotPhase) {
    match phase {
        LobbySnapshotPhase::Open => payload.push(0),
        LobbySnapshotPhase::LaunchCountdown { seconds_remaining } => {
            payload.push(1);
            payload.push(seconds_remaining);
        }
    }
}

pub(super) fn encode_optional_u8(payload: &mut Vec<u8>, value: Option<u8>) {
    match value {
        Some(value) => {
            payload.push(1);
            payload.push(value);
        }
        None => payload.push(0),
    }
}

pub(super) fn push_len_prefixed_string(
    payload: &mut Vec<u8>,
    field: &'static str,
    value: &str,
    max_len: usize,
) -> Result<(), PacketError> {
    let bytes = value.as_bytes();
    if bytes.len() > max_len {
        return Err(PacketError::StringLengthOutOfBounds {
            field,
            len: bytes.len(),
            max: max_len,
        });
    }

    let Ok(len) = u8::try_from(bytes.len()) else {
        return Err(PacketError::StringLengthOutOfBounds {
            field,
            len: bytes.len(),
            max: usize::from(u8::MAX),
        });
    };

    payload.push(len);
    payload.extend_from_slice(bytes);
    Ok(())
}

pub(super) fn ensure_available(
    payload: &[u8],
    index: usize,
    needed: usize,
    kind: &'static str,
) -> Result<(), PacketError> {
    let expected = index.saturating_add(needed);
    if payload.len() < expected {
        return Err(PacketError::ControlPayloadTooShort {
            kind,
            expected,
            actual: payload.len(),
        });
    }

    Ok(())
}

pub(super) fn ensure_consumed(
    payload: &[u8],
    index: usize,
    kind: &'static str,
) -> Result<(), PacketError> {
    if payload.len() != index {
        return Err(PacketError::UnexpectedTrailingBytes {
            kind,
            actual: payload.len() - index,
        });
    }

    Ok(())
}
