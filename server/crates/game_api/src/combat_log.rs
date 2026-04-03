#![allow(missing_docs)]

use std::fmt;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use game_domain::MatchId;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::diagnostics::{RollingTimingStats, TimingStatsSnapshot};

const COMBAT_LOG_SCHEMA_VERSION: u16 = 1;

/// Stable combat-log phase labels for persisted match events.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatLogPhase {
    SkillPick,
    PreCombat,
    Combat,
    MatchEnd,
}

/// Stable team labels for persisted combat events.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatLogTeam {
    TeamA,
    TeamB,
}

/// Stable match-outcome labels for persisted combat events.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatLogOutcome {
    TeamAWin,
    TeamBWin,
    NoContest,
}

/// Stable cast-mode labels for persisted combat events.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatLogCastMode {
    Windup,
    Channel,
}

/// Stable cast-cancel reasons for persisted combat events.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatLogCastCancelReason {
    Manual,
    Movement,
    ControlLoss,
    Defeat,
    Interrupt,
}

/// Stable impact-miss reasons for persisted combat events.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatLogMissReason {
    NoTarget,
    Blocked,
    Expired,
}

/// Stable target-kind labels for persisted combat events.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatLogTargetKind {
    Player,
    Deployable,
}

/// Stable status-removal reasons for persisted combat events.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatLogStatusRemovedReason {
    Expired,
    Dispelled,
    DamageBroken,
    Defeat,
    ShieldConsumed,
}

/// Stable trigger reasons for persisted combat events.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatLogTriggerReason {
    Expire,
    Dispel,
}

/// One removed status summary inside a dispel result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombatLogRemovedStatus {
    pub source_player_id: u32,
    pub slot: u8,
    pub status_kind: String,
    pub stacks: u8,
    pub remaining_ms: u16,
}

/// One stable persisted combat event payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CombatLogEvent {
    MatchStarted {
        participant_player_ids: Vec<u32>,
    },
    SkillPicked {
        player_id: u32,
        tree: String,
        tier: u8,
    },
    PreCombatStarted {
        seconds_remaining: u8,
    },
    CombatStarted,
    CastStarted {
        player_id: u32,
        slot: u8,
        behavior: String,
        mode: CombatLogCastMode,
        total_ms: u16,
    },
    CastCompleted {
        player_id: u32,
        slot: u8,
        behavior: String,
    },
    CastCanceled {
        player_id: u32,
        slot: u8,
        reason: CombatLogCastCancelReason,
    },
    ChannelTick {
        player_id: u32,
        slot: u8,
        tick_index: u16,
        behavior: String,
    },
    ImpactHit {
        source_player_id: u32,
        slot: u8,
        target_kind: CombatLogTargetKind,
        target_id: u32,
    },
    ImpactMiss {
        source_player_id: u32,
        slot: u8,
        reason: CombatLogMissReason,
    },
    DamageApplied {
        source_player_id: u32,
        target_kind: CombatLogTargetKind,
        target_id: u32,
        slot: u8,
        amount: u16,
        remaining_hit_points: u16,
        defeated: bool,
        status_kind: Option<String>,
        trigger: Option<CombatLogTriggerReason>,
    },
    HealingApplied {
        source_player_id: u32,
        target_player_id: u32,
        slot: u8,
        amount: u16,
        resulting_hit_points: u16,
        status_kind: Option<String>,
        trigger: Option<CombatLogTriggerReason>,
    },
    StatusApplied {
        source_player_id: u32,
        target_player_id: u32,
        slot: u8,
        status_kind: String,
        stacks: u8,
        stack_delta: u8,
        remaining_ms: u16,
    },
    StatusRemoved {
        source_player_id: u32,
        target_player_id: u32,
        slot: u8,
        status_kind: String,
        stacks: u8,
        remaining_ms: u16,
        reason: CombatLogStatusRemovedReason,
    },
    DispelCast {
        source_player_id: u32,
        slot: u8,
        scope: String,
        max_statuses: u8,
    },
    DispelResult {
        source_player_id: u32,
        slot: u8,
        target_player_id: u32,
        removed_statuses: Vec<CombatLogRemovedStatus>,
        triggered_payload_count: u8,
    },
    TriggerResolved {
        source_player_id: u32,
        target_kind: CombatLogTargetKind,
        target_id: u32,
        slot: u8,
        status_kind: String,
        trigger: CombatLogTriggerReason,
        payload_kind: String,
        amount: u16,
    },
    Defeat {
        source_player_id: Option<u32>,
        target_player_id: u32,
    },
    RoundWon {
        round: u8,
        winning_team: CombatLogTeam,
        score_a: u8,
        score_b: u8,
    },
    MatchEnded {
        outcome: CombatLogOutcome,
        score_a: u8,
        score_b: u8,
        message: String,
    },
}

impl CombatLogEvent {
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::MatchStarted { .. } => "match_started",
            Self::SkillPicked { .. } => "skill_picked",
            Self::PreCombatStarted { .. } => "pre_combat_started",
            Self::CombatStarted => "combat_started",
            Self::CastStarted { .. } => "cast_started",
            Self::CastCompleted { .. } => "cast_completed",
            Self::CastCanceled { .. } => "cast_canceled",
            Self::ChannelTick { .. } => "channel_tick",
            Self::ImpactHit { .. } => "impact_hit",
            Self::ImpactMiss { .. } => "impact_miss",
            Self::DamageApplied { .. } => "damage_applied",
            Self::HealingApplied { .. } => "healing_applied",
            Self::StatusApplied { .. } => "status_applied",
            Self::StatusRemoved { .. } => "status_removed",
            Self::DispelCast { .. } => "dispel_cast",
            Self::DispelResult { .. } => "dispel_result",
            Self::TriggerResolved { .. } => "trigger_resolved",
            Self::Defeat { .. } => "defeat",
            Self::RoundWon { .. } => "round_won",
            Self::MatchEnded { .. } => "match_ended",
        }
    }
}

/// One persisted combat-log row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombatLogEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence: Option<i64>,
    pub schema_version: u16,
    pub match_id: u32,
    pub round: u8,
    pub phase: CombatLogPhase,
    pub frame_index: u32,
    pub event: CombatLogEvent,
}

impl CombatLogEntry {
    #[must_use]
    pub fn new(
        match_id: MatchId,
        round: u8,
        phase: CombatLogPhase,
        frame_index: u32,
        event: CombatLogEvent,
    ) -> Self {
        Self {
            sequence: None,
            schema_version: COMBAT_LOG_SCHEMA_VERSION,
            match_id: match_id.get(),
            round,
            phase,
            frame_index,
            event,
        }
    }
}

/// One compact recent-match summary derived from the durable combat log.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombatLogMatchSummary {
    pub match_id: u32,
    pub event_count: u64,
    pub first_sequence: i64,
    pub last_sequence: i64,
    pub last_round: u8,
    pub last_phase: CombatLogPhase,
    pub last_frame_index: u32,
    pub last_event_kind: String,
}

/// Errors returned while opening, encoding, querying, or decoding the combat-log store.
#[derive(Debug)]
pub enum CombatLogStoreError {
    CreateParentDir {
        path: PathBuf,
        source: io::Error,
    },
    Open {
        path: Option<PathBuf>,
        source: rusqlite::Error,
    },
    Query {
        context: &'static str,
        source: rusqlite::Error,
    },
    Encode {
        source: serde_json::Error,
    },
    Decode {
        sequence: i64,
        source: serde_json::Error,
    },
}

impl fmt::Display for CombatLogStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CreateParentDir { path, source } => {
                write!(
                    f,
                    "failed to create combat log directory {}: {source}",
                    path.display()
                )
            }
            Self::Open { path, source } => match path {
                Some(path) => write!(f, "failed to open combat log {}: {source}", path.display()),
                None => write!(f, "failed to open in-memory combat log: {source}"),
            },
            Self::Query { context, source } => {
                write!(f, "combat log query failed while {context}: {source}")
            }
            Self::Encode { source } => {
                write!(f, "failed to encode a combat log event as json: {source}")
            }
            Self::Decode { sequence, source } => {
                write!(
                    f,
                    "failed to decode combat log row #{sequence} from json: {source}"
                )
            }
        }
    }
}

impl std::error::Error for CombatLogStoreError {}

/// Append-only SQLite-backed storage for server-authored combat events.
#[derive(Debug)]
pub struct CombatLogStore {
    connection: Connection,
    path: Option<PathBuf>,
    append_timings: Mutex<RollingTimingStats>,
    query_timings: Mutex<RollingTimingStats>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct CombatLogStoreDiagnosticsSnapshot {
    pub path: Option<String>,
    pub file_bytes: Option<u64>,
    pub event_count: u64,
    pub append: TimingStatsSnapshot,
    pub query: TimingStatsSnapshot,
}

impl CombatLogStore {
    /// Creates an in-memory combat log store for tests.
    pub fn new_ephemeral() -> Result<Self, CombatLogStoreError> {
        let connection = Connection::open_in_memory()
            .map_err(|source| CombatLogStoreError::Open { path: None, source })?;
        Self::initialize(connection, None)
    }

    /// Opens or creates a persistent combat log store on disk.
    pub fn new_persistent(path: impl Into<PathBuf>) -> Result<Self, CombatLogStoreError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| CombatLogStoreError::CreateParentDir {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let connection = Connection::open(&path).map_err(|source| CombatLogStoreError::Open {
            path: Some(path.clone()),
            source,
        })?;
        Self::initialize(connection, Some(path))
    }

    fn initialize(
        connection: Connection,
        path: Option<PathBuf>,
    ) -> Result<Self, CombatLogStoreError> {
        connection
            .execute_batch(
                "
                PRAGMA journal_mode = WAL;
                PRAGMA foreign_keys = ON;
                CREATE TABLE IF NOT EXISTS combat_events (
                    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                    match_id INTEGER NOT NULL,
                    round INTEGER NOT NULL,
                    phase TEXT NOT NULL,
                    frame_index INTEGER NOT NULL,
                    event_kind TEXT NOT NULL,
                    event_json TEXT NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_combat_events_match_sequence
                    ON combat_events (match_id, sequence);
                ",
            )
            .map_err(|source| CombatLogStoreError::Query {
                context: "initializing schema",
                source,
            })?;
        Ok(Self {
            connection,
            path,
            append_timings: Mutex::new(RollingTimingStats::default()),
            query_timings: Mutex::new(RollingTimingStats::default()),
        })
    }

    /// Appends one durable combat event to the backing store.
    pub fn append(&mut self, entry: &CombatLogEntry) -> Result<(), CombatLogStoreError> {
        let started_at = Instant::now();
        let json = serde_json::to_string(entry)
            .map_err(|source| CombatLogStoreError::Encode { source })?;
        self.connection
            .execute(
                "
                INSERT INTO combat_events (
                    match_id,
                    round,
                    phase,
                    frame_index,
                    event_kind,
                    event_json
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ",
                params![
                    i64::from(entry.match_id),
                    i64::from(entry.round),
                    phase_label(entry.phase),
                    i64::from(entry.frame_index),
                    entry.event.kind(),
                    json,
                ],
            )
            .map_err(|source| CombatLogStoreError::Query {
                context: "appending combat event",
                source,
            })?;
        if let Ok(mut timings) = self.append_timings.lock() {
            timings.record_duration(started_at.elapsed());
        }
        Ok(())
    }

    /// Returns every logged combat event for one match, ordered by append sequence.
    pub fn events_for_match(
        &self,
        match_id: MatchId,
    ) -> Result<Vec<CombatLogEntry>, CombatLogStoreError> {
        self.events_for_match_limit(match_id, usize::MAX)
    }

    /// Returns the most recent durable combat events for one match in append order.
    pub fn events_for_match_limit(
        &self,
        match_id: MatchId,
        limit: usize,
    ) -> Result<Vec<CombatLogEntry>, CombatLogStoreError> {
        let started_at = Instant::now();
        let mut statement = self
            .connection
            .prepare(
                "
                SELECT sequence, event_json
                FROM combat_events
                WHERE match_id = ?1
                ORDER BY sequence DESC
                LIMIT ?2
                ",
            )
            .map_err(|source| CombatLogStoreError::Query {
                context: "preparing match query",
                source,
            })?;
        let limit = i64::try_from(limit.min(usize::try_from(i64::MAX).unwrap_or(usize::MAX)))
            .unwrap_or(i64::MAX);
        let rows = statement
            .query_map(params![i64::from(match_id.get()), limit], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|source| CombatLogStoreError::Query {
                context: "executing match query",
                source,
            })?;

        let mut events = Vec::new();
        for row in rows {
            let (sequence, json) = row.map_err(|source| CombatLogStoreError::Query {
                context: "reading match query row",
                source,
            })?;
            let mut entry: CombatLogEntry = serde_json::from_str(&json)
                .map_err(|source| CombatLogStoreError::Decode { sequence, source })?;
            entry.sequence = Some(sequence);
            events.push(entry);
        }
        events.reverse();
        if let Ok(mut timings) = self.query_timings.lock() {
            timings.record_duration(started_at.elapsed());
        }
        Ok(events)
    }

    /// Returns the most recent matches present in the durable combat log.
    pub fn recent_matches(
        &self,
        limit: usize,
    ) -> Result<Vec<CombatLogMatchSummary>, CombatLogStoreError> {
        let started_at = Instant::now();
        let limit = i64::try_from(limit.min(usize::try_from(i64::MAX).unwrap_or(usize::MAX)))
            .unwrap_or(i64::MAX);
        let mut statement = self
            .connection
            .prepare(
                "
                WITH recent_matches AS (
                    SELECT
                        match_id,
                        COUNT(*) AS event_count,
                        MIN(sequence) AS first_sequence,
                        MAX(sequence) AS last_sequence
                    FROM combat_events
                    GROUP BY match_id
                    ORDER BY last_sequence DESC
                    LIMIT ?1
                )
                SELECT
                    recent_matches.match_id,
                    recent_matches.event_count,
                    recent_matches.first_sequence,
                    recent_matches.last_sequence,
                    combat_events.round,
                    combat_events.phase,
                    combat_events.frame_index,
                    combat_events.event_kind
                FROM recent_matches
                JOIN combat_events
                    ON combat_events.sequence = recent_matches.last_sequence
                ORDER BY recent_matches.last_sequence DESC
                ",
            )
            .map_err(|source| CombatLogStoreError::Query {
                context: "preparing recent-match query",
                source,
            })?;
        let rows = statement
            .query_map([limit], |row| {
                let phase_label = row.get::<_, String>(5)?;
                Ok(CombatLogMatchSummary {
                    match_id: row.get::<_, u32>(0)?,
                    event_count: row
                        .get::<_, i64>(1)
                        .map(|count| u64::try_from(count).unwrap_or(0))?,
                    first_sequence: row.get(2)?,
                    last_sequence: row.get(3)?,
                    last_round: row.get::<_, u8>(4)?,
                    last_phase: parse_phase_label(&phase_label).ok_or_else(|| {
                        rusqlite::Error::FromSqlConversionFailure(
                            5,
                            rusqlite::types::Type::Text,
                            format!("invalid combat log phase label '{phase_label}'").into(),
                        )
                    })?,
                    last_frame_index: row.get::<_, u32>(6)?,
                    last_event_kind: row.get(7)?,
                })
            })
            .map_err(|source| CombatLogStoreError::Query {
                context: "executing recent-match query",
                source,
            })?;

        let mut matches = Vec::new();
        for row in rows {
            matches.push(row.map_err(|source| CombatLogStoreError::Query {
                context: "reading recent-match query row",
                source,
            })?);
        }
        if let Ok(mut timings) = self.query_timings.lock() {
            timings.record_duration(started_at.elapsed());
        }
        Ok(matches)
    }

    /// Returns the current operational diagnostics for the combat-log store.
    #[must_use]
    pub(crate) fn diagnostics_snapshot(&self) -> CombatLogStoreDiagnosticsSnapshot {
        CombatLogStoreDiagnosticsSnapshot {
            path: self.path.as_ref().map(|path| path.display().to_string()),
            file_bytes: self
                .path
                .as_ref()
                .and_then(|path| fs::metadata(path).ok())
                .map(|metadata| metadata.len()),
            event_count: self.event_count_untracked().unwrap_or(0),
            append: self.append_timings.lock().map_or_else(
                |_| RollingTimingStats::default().snapshot(),
                |timings| timings.snapshot(),
            ),
            query: self.query_timings.lock().map_or_else(
                |_| RollingTimingStats::default().snapshot(),
                |timings| timings.snapshot(),
            ),
        }
    }

    fn event_count_untracked(&self) -> Result<u64, CombatLogStoreError> {
        self.connection
            .query_row("SELECT COUNT(*) FROM combat_events", [], |row| {
                row.get::<_, i64>(0)
            })
            .map(|count| u64::try_from(count).unwrap_or(0))
            .map_err(|source| CombatLogStoreError::Query {
                context: "counting combat events",
                source,
            })
    }
}

fn phase_label(phase: CombatLogPhase) -> &'static str {
    match phase {
        CombatLogPhase::SkillPick => "skill_pick",
        CombatLogPhase::PreCombat => "pre_combat",
        CombatLogPhase::Combat => "combat",
        CombatLogPhase::MatchEnd => "match_end",
    }
}

fn parse_phase_label(label: &str) -> Option<CombatLogPhase> {
    match label {
        "skill_pick" => Some(CombatLogPhase::SkillPick),
        "pre_combat" => Some(CombatLogPhase::PreCombat),
        "combat" => Some(CombatLogPhase::Combat),
        "match_end" => Some(CombatLogPhase::MatchEnd),
        _ => None,
    }
}
