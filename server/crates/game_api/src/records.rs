use std::collections::BTreeMap;
use std::fmt;
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::PathBuf;

use game_domain::{PlayerId, PlayerName, PlayerRecord};

const FIELD_COUNT: usize = 5;

#[derive(Clone, Debug, PartialEq, Eq)]
struct StoredPlayerRecord {
    player_name: PlayerName,
    record: PlayerRecord,
}

#[derive(Debug)]
pub struct PlayerRecordStore {
    path: Option<PathBuf>,
    records: BTreeMap<PlayerId, StoredPlayerRecord>,
}

impl PlayerRecordStore {
    #[must_use]
    pub fn new_ephemeral() -> Self {
        Self {
            path: None,
            records: BTreeMap::new(),
        }
    }

    pub fn new_persistent(path: impl Into<PathBuf>) -> Result<Self, RecordStoreError> {
        let path = path.into();
        let records = if path.exists() {
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

    pub fn load_or_create(
        &mut self,
        player_id: PlayerId,
        player_name: &PlayerName,
    ) -> Result<PlayerRecord, RecordStoreError> {
        let mut changed = false;
        let record = if let Some(stored) = self.records.get_mut(&player_id) {
            if stored.player_name != *player_name {
                stored.player_name = player_name.clone();
                changed = true;
            }
            stored.record
        } else {
            self.records.insert(
                player_id,
                StoredPlayerRecord {
                    player_name: player_name.clone(),
                    record: PlayerRecord::new(),
                },
            );
            changed = true;
            PlayerRecord::new()
        };

        if changed {
            self.flush()?;
        }

        Ok(record)
    }

    pub fn save(
        &mut self,
        player_id: PlayerId,
        player_name: &PlayerName,
        record: PlayerRecord,
    ) -> Result<(), RecordStoreError> {
        self.records.insert(
            player_id,
            StoredPlayerRecord {
                player_name: player_name.clone(),
                record,
            },
        );
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

#[derive(Debug)]
pub enum RecordStoreError {
    Read { path: PathBuf, source: io::Error },
    Write { path: PathBuf, source: io::Error },
    CreateParentDir { path: PathBuf, source: io::Error },
    MalformedLine { line_number: usize, reason: String },
    DuplicatePlayerId { line_number: usize, player_id: u32 },
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
        }
    }
}

impl std::error::Error for RecordStoreError {}

fn parse_records(input: &str) -> Result<BTreeMap<PlayerId, StoredPlayerRecord>, RecordStoreError> {
    let mut records = BTreeMap::new();

    for (line_index, raw_line) in input.lines().enumerate() {
        let line_number = line_index + 1;
        if raw_line.trim().is_empty() {
            continue;
        }

        let fields = raw_line.split('\t').collect::<Vec<_>>();
        if fields.len() != FIELD_COUNT {
            return Err(RecordStoreError::MalformedLine {
                line_number,
                reason: format!(
                    "expected {FIELD_COUNT} tab-separated fields, found {}",
                    fields.len()
                ),
            });
        }

        let player_id = parse_player_id(fields[0], line_number)?;
        let player_name =
            PlayerName::new(fields[1]).map_err(|error| RecordStoreError::MalformedLine {
                line_number,
                reason: error.to_string(),
            })?;
        let wins = parse_counter(fields[2], line_number, "wins")?;
        let losses = parse_counter(fields[3], line_number, "losses")?;
        let no_contests = parse_counter(fields[4], line_number, "no_contests")?;

        if records.contains_key(&player_id) {
            return Err(RecordStoreError::DuplicatePlayerId {
                line_number,
                player_id: player_id.get(),
            });
        }

        records.insert(
            player_id,
            StoredPlayerRecord {
                player_name,
                record: PlayerRecord {
                    wins,
                    losses,
                    no_contests,
                },
            },
        );
    }

    Ok(records)
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

fn serialize_records(records: &BTreeMap<PlayerId, StoredPlayerRecord>) -> String {
    let mut output = String::new();
    for (player_id, stored) in records {
        let _ = writeln!(
            output,
            "{}\t{}\t{}\t{}\t{}",
            player_id.get(),
            stored.player_name,
            stored.record.wins,
            stored.record.losses,
            stored.record.no_contests
        );
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn player_id(raw: u32) -> PlayerId {
        PlayerId::new(raw).expect("valid player id")
    }

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
                .load_or_create(player_id(1), &player_name("Alice"))
                .expect("record should load"),
            PlayerRecord::new()
        );

        let updated = PlayerRecord {
            wins: 2,
            losses: 1,
            no_contests: 3,
        };
        store
            .save(player_id(1), &player_name("Alice"), updated)
            .expect("record should save");

        assert_eq!(
            store
                .load_or_create(player_id(1), &player_name("Alice"))
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
            .load_or_create(player_id(9), &player_name("Mallory"))
            .expect("record should load");
        assert_eq!(record, PlayerRecord::new());

        let updated = PlayerRecord {
            wins: 4,
            losses: 2,
            no_contests: 1,
        };
        store
            .save(player_id(9), &player_name("Mallory"), updated)
            .expect("record should persist");
        drop(store);

        let mut reloaded =
            PlayerRecordStore::new_persistent(&path).expect("store should reload from disk");
        assert_eq!(
            reloaded
                .load_or_create(player_id(9), &player_name("Mallory"))
                .expect("record should exist"),
            updated
        );

        remove_if_exists(&path);
    }

    #[test]
    fn persistent_store_rejects_malformed_rows_and_duplicate_ids() {
        let bad_line = parse_records("1\tAlice\t1\t2\n");
        assert!(matches!(
            bad_line,
            Err(RecordStoreError::MalformedLine { line_number: 1, .. })
        ));

        let duplicate = parse_records("1\tAlice\t0\t0\t0\n1\tBob\t1\t1\t1\n");
        assert_eq!(
            duplicate
                .expect_err("duplicate rows should fail")
                .to_string(),
            "player record store line 2 repeats player id 1"
        );
    }
}
