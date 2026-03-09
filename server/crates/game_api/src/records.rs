use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::PathBuf;

use game_domain::{PlayerId, PlayerName, PlayerRecord};

const CURRENT_FIELD_COUNT: usize = 4;
const LEGACY_FIELD_COUNT: usize = 5;
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
            return Ok(*record);
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
        record: PlayerRecord,
    ) -> Result<(), RecordStoreError> {
        self.records.insert(player_name.clone(), record);
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
        CURRENT_FIELD_COUNT => {
            let player_name = parse_player_name(fields[0], line_number)?;
            let record = parse_record_counters(fields[1], fields[2], fields[3], line_number)?;
            Ok((None, player_name, record))
        }
        LEGACY_FIELD_COUNT => {
            let player_id = parse_player_id(fields[0], line_number)?;
            let player_name = parse_player_name(fields[1], line_number)?;
            let record = parse_record_counters(fields[2], fields[3], fields[4], line_number)?;
            Ok((Some(player_id), player_name, record))
        }
        other => Err(RecordStoreError::MalformedLine {
            line_number,
            reason: format!(
                "expected {CURRENT_FIELD_COUNT} or {LEGACY_FIELD_COUNT} tab-separated fields, found {other}"
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

fn serialize_records(records: &BTreeMap<PlayerName, PlayerRecord>) -> String {
    let mut output = String::new();
    for (player_name, record) in records {
        let _ = writeln!(
            output,
            "{}\t{}\t{}\t{}",
            player_name, record.wins, record.losses, record.no_contests
        );
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn player_name(raw: &str) -> PlayerName {
        PlayerName::new(raw).expect("valid player name")
    }

    fn temp_path(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should move forward")
            .as_nanos();
        std::env::temp_dir()
            .join("rusaren-tests")
            .join(format!("{label}-{}-{unique}", std::process::id()))
            .join("player-records.tsv")
    }

    fn remove_if_exists(path: &Path) {
        if let Some(parent) = path.parent() {
            if parent.exists() {
                let _ = fs::remove_dir_all(parent);
            }
        }
    }

    #[test]
    fn ephemeral_store_creates_and_updates_records_without_io() {
        let mut store = PlayerRecordStore::new_ephemeral();
        assert_eq!(
            store
                .load_or_create(&player_name("Alice"))
                .expect("record should load"),
            PlayerRecord::new()
        );

        let updated = PlayerRecord {
            wins: 2,
            losses: 1,
            no_contests: 3,
        };
        store
            .save(&player_name("Alice"), updated)
            .expect("record should save");

        assert_eq!(
            store
                .load_or_create(&player_name("Alice"))
                .expect("record should reload"),
            updated
        );
    }

    #[test]
    fn persistent_store_round_trips_records_through_disk() {
        let path = temp_path("player-records");
        remove_if_exists(&path);

        let mut store =
            PlayerRecordStore::new_persistent(&path).expect("store should create on demand");
        let record = store
            .load_or_create(&player_name("Mallory"))
            .expect("record should load");
        assert_eq!(record, PlayerRecord::new());

        let updated = PlayerRecord {
            wins: 4,
            losses: 2,
            no_contests: 1,
        };
        store
            .save(&player_name("Mallory"), updated)
            .expect("record should persist");
        drop(store);

        let mut reloaded =
            PlayerRecordStore::new_persistent(&path).expect("store should reload from disk");
        assert_eq!(
            reloaded
                .load_or_create(&player_name("Mallory"))
                .expect("record should exist"),
            updated
        );

        remove_if_exists(&path);
    }

    #[test]
    fn persistent_store_rejects_malformed_rows_and_duplicate_keys() {
        let bad_line = parse_records("Alice\t1\t2\n");
        assert!(matches!(
            bad_line,
            Err(RecordStoreError::MalformedLine { line_number: 1, .. })
        ));

        let duplicate_names = parse_records("Alice\t0\t0\t0\nAlice\t1\t1\t1\n");
        assert_eq!(
            duplicate_names
                .expect_err("duplicate rows should fail")
                .to_string(),
            "player record store line 2 repeats player name Alice"
        );

        let duplicate_legacy_ids = parse_records("1\tAlice\t0\t0\t0\n1\tBob\t1\t1\t1\n");
        assert_eq!(
            duplicate_legacy_ids
                .expect_err("duplicate legacy ids should fail")
                .to_string(),
            "player record store line 2 repeats player id 1"
        );
    }

    #[test]
    fn legacy_rows_merge_duplicate_player_names_during_migration() {
        let canonical =
            canonicalize_record_store_contents("1\tAlice\t1\t2\t3\n2\tAlice\t4\t5\t6\n")
                .expect("legacy duplicate names should merge during migration");
        assert_eq!(canonical, "Alice\t5\t7\t9\n");
    }

    #[test]
    fn canonicalize_record_store_rewrites_rows_in_sorted_order() {
        let canonical = canonicalize_record_store_contents("Mallory\t4\t2\t1\nAlice\t1\t0\t0\n")
            .expect("canonicalization should succeed");
        assert_eq!(canonical, "Alice\t1\t0\t0\nMallory\t4\t2\t1\n");
    }

    #[test]
    fn canonicalize_record_store_preserves_empty_input() {
        let canonical = canonicalize_record_store_contents("").expect("empty input should parse");
        assert_eq!(canonical, "");
    }

    #[test]
    fn canonicalize_record_store_reads_legacy_rows_and_rewrites_them() {
        let canonical =
            canonicalize_record_store_contents("9\tMallory\t4\t2\t1\n1\tAlice\t1\t0\t0\n")
                .expect("legacy rows should parse");
        assert_eq!(canonical, "Alice\t1\t0\t0\nMallory\t4\t2\t1\n");
    }

    #[test]
    fn persistent_store_rejects_oversized_files() {
        let path = temp_path("oversized-record-store");
        remove_if_exists(&path);

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("test directory should exist");
        }
        let oversized = "A".repeat(usize::try_from(MAX_RECORD_STORE_BYTES).unwrap_or(0) + 1);
        fs::write(&path, oversized).expect("oversized store should be written");

        let error = PlayerRecordStore::new_persistent(&path)
            .expect_err("oversized record store should be rejected");
        assert_eq!(
            error.to_string(),
            format!(
                "player record store {} exceeds the maximum allowed size: {} bytes > {} bytes",
                path.display(),
                MAX_RECORD_STORE_BYTES + 1,
                MAX_RECORD_STORE_BYTES
            )
        );

        remove_if_exists(&path);
    }
}
