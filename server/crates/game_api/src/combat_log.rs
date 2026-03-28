#![allow(missing_docs)]

use std::fmt;
use std::fs;
use std::io;
use std::path::PathBuf;

use game_domain::MatchId;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

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
}

impl CombatLogStore {
    /// Creates an in-memory combat log store for tests.
    pub fn new_ephemeral() -> Result<Self, CombatLogStoreError> {
        let connection = Connection::open_in_memory()
            .map_err(|source| CombatLogStoreError::Open { path: None, source })?;
        Self::initialize(connection)
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
            path: Some(path),
            source,
        })?;
        Self::initialize(connection)
    }

    fn initialize(connection: Connection) -> Result<Self, CombatLogStoreError> {
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
        Ok(Self { connection })
    }

    /// Appends one durable combat event to the backing store.
    pub fn append(&mut self, entry: &CombatLogEntry) -> Result<(), CombatLogStoreError> {
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
        Ok(())
    }

    /// Returns every logged combat event for one match, ordered by append sequence.
    pub fn events_for_match(
        &self,
        match_id: MatchId,
    ) -> Result<Vec<CombatLogEntry>, CombatLogStoreError> {
        let mut statement = self
            .connection
            .prepare(
                "
                SELECT sequence, event_json
                FROM combat_events
                WHERE match_id = ?1
                ORDER BY sequence ASC
                ",
            )
            .map_err(|source| CombatLogStoreError::Query {
                context: "preparing match query",
                source,
            })?;
        let rows = statement
            .query_map([i64::from(match_id.get())], |row| {
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
        Ok(events)
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
