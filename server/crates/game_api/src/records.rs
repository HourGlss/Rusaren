use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::PathBuf;

use game_domain::{PlayerId, PlayerName, PlayerRecord};

const LEGACY_MINIMAL_FIELD_COUNT: usize = 5;
const LEGACY_EXTENDED_FIELD_COUNT: usize = 13;
const MINIMAL_FIELD_COUNT: usize = 4;
const EXTENDED_FIELD_COUNT: usize = 12;
/// Maximum accepted size, in bytes, for the persisted player-record store.
pub const MAX_RECORD_STORE_BYTES: u64 = 1_048_576;

/// Persistent or in-memory storage for player win/loss/no-contest records.
#[derive(Debug)]
pub struct PlayerRecordStore {
    path: Option<PathBuf>,
    records: BTreeMap<PlayerName, PlayerRecord>,
}

impl PlayerRecordStore {
    /// Creates an in-memory store that performs no filesystem I/O.
    #[must_use]
    pub fn new_ephemeral() -> Self {
        Self {
            path: None,
            records: BTreeMap::new(),
        }
    }

    /// Opens a persistent store from disk, creating an empty one in memory if the file is absent.
    pub fn new_persistent(path: impl Into<PathBuf>) -> Result<Self, RecordStoreError> {
        let path = path.into();
        let records = if path.exists() {
            let metadata = fs::metadata(&path).map_err(|source| RecordStoreError::Read {
                path: path.clone(),
                source,
            })?;
            if metadata.len() > MAX_RECORD_STORE_BYTES {
                return Err(RecordStoreError::FileTooLarge {
                    path: path.clone(),
                    actual: metadata.len(),
                    maximum: MAX_RECORD_STORE_BYTES,
                });
            }

            parse_records(
                &fs::read_to_string(&path).map_err(|source| RecordStoreError::Read {
                    path: path.clone(),
                    source,
                })?,
            )?
        } else {
            BTreeMap::new()
        };

        Ok(Self {
            path: Some(path),
            records,
        })
    }

    /// Loads an existing record for a player or creates a new zeroed record.
    pub fn load_or_create(
        &mut self,
        player_name: &PlayerName,
    ) -> Result<PlayerRecord, RecordStoreError> {
        if let Some(record) = self.records.get(player_name) {
            return Ok(record.clone());
        }

        self.records
            .insert(player_name.clone(), PlayerRecord::new());
        self.flush()?;
        Ok(PlayerRecord::new())
    }

    /// Writes an updated player record back into the store.
    pub fn save(
        &mut self,
        player_name: &PlayerName,
        record: &PlayerRecord,
    ) -> Result<(), RecordStoreError> {
        self.records.insert(player_name.clone(), record.clone());
        self.flush()
    }

    fn flush(&self) -> Result<(), RecordStoreError> {
        let Some(path) = &self.path else {
            return Ok(());
        };

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| RecordStoreError::CreateParentDir {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        fs::write(path, serialize_records(&self.records)).map_err(|source| {
            RecordStoreError::Write {
                path: path.clone(),
                source,
            }
        })
    }
}

/// Parses and reserializes record-store contents into the canonical sorted format.
///
/// VERIFIED MODEL: `server/verus/player_record_store_model.rs` mirrors the size-bounded,
/// line-oriented record-store invariants enforced by the runtime parser below. The proof
/// model is intentionally small and does not replace the parser tests over production data.
pub fn canonicalize_record_store_contents(input: &str) -> Result<String, RecordStoreError> {
    let records = parse_records(input)?;
    Ok(serialize_records(&records))
}

/// Errors returned while reading, validating, or writing the player record store.
#[derive(Debug)]
pub enum RecordStoreError {
    /// Reading the backing store failed.
    Read {
        /// The path that failed to read.
        path: PathBuf,
        /// The underlying I/O error.
        source: io::Error,
    },
    /// Writing the backing store failed.
    Write {
        /// The path that failed to write.
        path: PathBuf,
        /// The underlying I/O error.
        source: io::Error,
    },
    /// Creating the parent directory for the backing store failed.
    CreateParentDir {
        /// The parent directory path that could not be created.
        path: PathBuf,
        /// The underlying I/O error.
        source: io::Error,
    },
    /// The backing file exceeded the configured safety limit.
    FileTooLarge {
        /// The path of the oversized file.
        path: PathBuf,
        /// The observed byte count.
        actual: u64,
        /// The maximum accepted byte count.
        maximum: u64,
    },
    /// One row in the backing file was malformed.
    MalformedLine {
        /// The 1-based line number that failed validation.
        line_number: usize,
        /// The specific reason the row was rejected.
        reason: String,
    },
    /// A legacy row repeated a player id that had already appeared.
    DuplicatePlayerId {
        /// The 1-based line number that failed validation.
        line_number: usize,
        /// The repeated legacy player id.
        player_id: u32,
    },
    /// A non-legacy row repeated a player name that had already appeared.
    DuplicatePlayerName {
        /// The 1-based line number that failed validation.
        line_number: usize,
        /// The repeated player name.
        player_name: String,
    },
}

impl fmt::Display for RecordStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(
                    f,
                    "failed to read player record store {}: {source}",
                    path.display()
                )
            }
            Self::Write { path, source } => {
                write!(
                    f,
                    "failed to write player record store {}: {source}",
                    path.display()
                )
            }
            Self::CreateParentDir { path, source } => {
                write!(
                    f,
                    "failed to create player record store directory {}: {source}",
                    path.display()
                )
            }
            Self::FileTooLarge {
                path,
                actual,
                maximum,
            } => write!(
                f,
                "player record store {} exceeds the maximum allowed size: {actual} bytes > {maximum} bytes",
                path.display()
            ),
            Self::MalformedLine {
                line_number,
                reason,
            } => write!(
                f,
                "player record store line {line_number} is malformed: {reason}"
            ),
            Self::DuplicatePlayerId {
                line_number,
                player_id,
            } => write!(
                f,
                "player record store line {line_number} repeats player id {player_id}"
            ),
            Self::DuplicatePlayerName {
                line_number,
                player_name,
            } => write!(
                f,
                "player record store line {line_number} repeats player name {player_name}"
            ),
        }
    }
}

impl std::error::Error for RecordStoreError {}

fn parse_records(input: &str) -> Result<BTreeMap<PlayerName, PlayerRecord>, RecordStoreError> {
    let mut records: BTreeMap<PlayerName, PlayerRecord> = BTreeMap::new();
    let mut legacy_ids = BTreeSet::new();

    for (line_index, raw_line) in input.lines().enumerate() {
        let line_number = line_index + 1;
        if raw_line.trim().is_empty() {
            continue;
        }

        let fields = raw_line.split('\t').collect::<Vec<_>>();
        let (legacy_player_id, player_name, record) = parse_record_fields(&fields, line_number)?;

        if let Some(player_id) = legacy_player_id {
            if !legacy_ids.insert(player_id) {
                return Err(RecordStoreError::DuplicatePlayerId {
                    line_number,
                    player_id: player_id.get(),
                });
            }
        }

        if let Some(existing) = records.get_mut(&player_name) {
            if legacy_player_id.is_some() {
                existing.wins = existing.wins.saturating_add(record.wins);
                existing.losses = existing.losses.saturating_add(record.losses);
                existing.no_contests = existing.no_contests.saturating_add(record.no_contests);
                existing.round_wins = existing.round_wins.saturating_add(record.round_wins);
                existing.round_losses = existing.round_losses.saturating_add(record.round_losses);
                existing.total_damage_done = existing
                    .total_damage_done
                    .saturating_add(record.total_damage_done);
                existing.total_healing_done = existing
                    .total_healing_done
                    .saturating_add(record.total_healing_done);
                existing.total_combat_ms = existing
                    .total_combat_ms
                    .saturating_add(record.total_combat_ms);
                existing.cc_used = existing.cc_used.saturating_add(record.cc_used);
                existing.cc_hits = existing.cc_hits.saturating_add(record.cc_hits);
                for (skill_id, count) in &record.skill_pick_counts {
                    let existing_count = existing
                        .skill_pick_counts
                        .entry(skill_id.clone())
                        .or_insert(0);
                    *existing_count = existing_count.saturating_add(*count);
                }
            } else {
                return Err(RecordStoreError::DuplicatePlayerName {
                    line_number,
                    player_name: player_name.to_string(),
                });
            }
        } else {
            records.insert(player_name, record);
        }
    }

    Ok(records)
}

fn parse_record_fields(
    fields: &[&str],
    line_number: usize,
) -> Result<(Option<PlayerId>, PlayerName, PlayerRecord), RecordStoreError> {
    match fields.len() {
        MINIMAL_FIELD_COUNT => {
            let player_name = parse_player_name(fields[0], line_number)?;
            let record = parse_record_counters(fields[1], fields[2], fields[3], line_number)?;
            Ok((None, player_name, record))
        }
        LEGACY_MINIMAL_FIELD_COUNT => {
            let player_id = parse_player_id(fields[0], line_number)?;
            let player_name = parse_player_name(fields[1], line_number)?;
            let record = parse_record_counters(fields[2], fields[3], fields[4], line_number)?;
            Ok((Some(player_id), player_name, record))
        }
        EXTENDED_FIELD_COUNT => {
            let player_name = parse_player_name(fields[0], line_number)?;
            let record = parse_extended_record_fields(&fields[1..], line_number)?;
            Ok((None, player_name, record))
        }
        LEGACY_EXTENDED_FIELD_COUNT => {
            let player_id = parse_player_id(fields[0], line_number)?;
            let player_name = parse_player_name(fields[1], line_number)?;
            let record = parse_extended_record_fields(&fields[2..], line_number)?;
            Ok((Some(player_id), player_name, record))
        }
        other => Err(RecordStoreError::MalformedLine {
            line_number,
            reason: format!(
                "expected {MINIMAL_FIELD_COUNT}, {LEGACY_MINIMAL_FIELD_COUNT}, {EXTENDED_FIELD_COUNT}, or {LEGACY_EXTENDED_FIELD_COUNT} tab-separated fields, found {other}"
            ),
        }),
    }
}

fn parse_player_name(raw: &str, line_number: usize) -> Result<PlayerName, RecordStoreError> {
    PlayerName::new(raw).map_err(|error| RecordStoreError::MalformedLine {
        line_number,
        reason: error.to_string(),
    })
}

fn parse_record_counters(
    wins: &str,
    losses: &str,
    no_contests: &str,
    line_number: usize,
) -> Result<PlayerRecord, RecordStoreError> {
    Ok(PlayerRecord {
        wins: parse_counter(wins, line_number, "wins")?,
        losses: parse_counter(losses, line_number, "losses")?,
        no_contests: parse_counter(no_contests, line_number, "no_contests")?,
        ..PlayerRecord::new()
    })
}

fn parse_extended_record_fields(
    fields: &[&str],
    line_number: usize,
) -> Result<PlayerRecord, RecordStoreError> {
    if fields.len() != EXTENDED_FIELD_COUNT - 1 {
        return Err(RecordStoreError::MalformedLine {
            line_number,
            reason: format!(
                "extended record rows require {} fields after the player name, found {}",
                EXTENDED_FIELD_COUNT - 1,
                fields.len()
            ),
        });
    }
    Ok(PlayerRecord {
        wins: parse_counter(fields[0], line_number, "wins")?,
        losses: parse_counter(fields[1], line_number, "losses")?,
        no_contests: parse_counter(fields[2], line_number, "no_contests")?,
        round_wins: parse_counter(fields[3], line_number, "round_wins")?,
        round_losses: parse_counter(fields[4], line_number, "round_losses")?,
        total_damage_done: parse_counter_u32(fields[5], line_number, "total_damage_done")?,
        total_healing_done: parse_counter_u32(fields[6], line_number, "total_healing_done")?,
        total_combat_ms: parse_counter_u32(fields[7], line_number, "total_combat_ms")?,
        cc_used: parse_counter(fields[8], line_number, "cc_used")?,
        cc_hits: parse_counter(fields[9], line_number, "cc_hits")?,
        skill_pick_counts: parse_skill_pick_counts(fields[10], line_number)?,
    })
}

fn parse_player_id(raw: &str, line_number: usize) -> Result<PlayerId, RecordStoreError> {
    let parsed = raw
        .parse::<u32>()
        .map_err(|error| RecordStoreError::MalformedLine {
            line_number,
            reason: format!("player_id '{raw}' is not a valid u32: {error}"),
        })?;

    PlayerId::new(parsed).map_err(|error| RecordStoreError::MalformedLine {
        line_number,
        reason: error.to_string(),
    })
}

fn parse_counter(
    raw: &str,
    line_number: usize,
    field: &'static str,
) -> Result<u16, RecordStoreError> {
    raw.parse::<u16>()
        .map_err(|error| RecordStoreError::MalformedLine {
            line_number,
            reason: format!("{field} '{raw}' is not a valid u16: {error}"),
        })
}

fn parse_counter_u32(
    raw: &str,
    line_number: usize,
    field: &'static str,
) -> Result<u32, RecordStoreError> {
    raw.parse::<u32>()
        .map_err(|error| RecordStoreError::MalformedLine {
            line_number,
            reason: format!("{field} '{raw}' is not a valid u32: {error}"),
        })
}

fn parse_skill_pick_counts(
    raw: &str,
    line_number: usize,
) -> Result<BTreeMap<String, u16>, RecordStoreError> {
    let mut counts = BTreeMap::new();
    if raw.is_empty() {
        return Ok(counts);
    }
    for entry in raw.split(',') {
        let Some((skill_id, count_raw)) = entry.split_once('=') else {
            return Err(RecordStoreError::MalformedLine {
                line_number,
                reason: format!("skill pick entry '{entry}' must be encoded as skill_id=count"),
            });
        };
        if skill_id.is_empty() {
            return Err(RecordStoreError::MalformedLine {
                line_number,
                reason: String::from("skill pick counts cannot contain an empty skill id"),
            });
        }
        let count = parse_counter(count_raw, line_number, "skill_pick_count")?;
        counts.insert(skill_id.to_string(), count);
    }
    Ok(counts)
}

fn serialize_records(records: &BTreeMap<PlayerName, PlayerRecord>) -> String {
    let mut output = String::new();
    for (player_name, record) in records {
        let skill_counts = record
            .skill_pick_counts
            .iter()
            .map(|(skill_id, count)| format!("{skill_id}={count}"))
            .collect::<Vec<_>>()
            .join(",");
        let _ = writeln!(
            output,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            player_name,
            record.wins,
            record.losses,
            record.no_contests,
            record.round_wins,
            record.round_losses,
            record.total_damage_done,
            record.total_healing_done,
            record.total_combat_ms,
            record.cc_used,
            record.cc_hits,
            skill_counts
        );
    }
    output
}

#[cfg(test)]
mod tests;
