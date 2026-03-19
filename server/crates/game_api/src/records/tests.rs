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
    let canonical = canonicalize_record_store_contents("1\tAlice\t1\t2\t3\n2\tAlice\t4\t5\t6\n")
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
    let canonical = canonicalize_record_store_contents("9\tMallory\t4\t2\t1\n1\tAlice\t1\t0\t0\n")
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
