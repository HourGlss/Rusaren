use std::fs;
use std::path::PathBuf;

use game_api::canonicalize_record_store_contents;

fn corpus_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn corpus_roots() -> Vec<PathBuf> {
    let repo_root = corpus_root();
    [
        repo_root.join("fuzz").join("corpus"),
        repo_root.join("target").join("fuzz-generated-corpus"),
    ]
    .into_iter()
    .filter(|root| root.exists())
    .collect()
}

#[test]
fn replay_player_record_store_parse_corpus() {
    for bytes in corpus_files("player_record_store_parse") {
        let input = String::from_utf8_lossy(&bytes);
        let _ = canonicalize_record_store_contents(&input);
    }
}

fn corpus_files(target: &str) -> Vec<Vec<u8>> {
    let mut bytes = Vec::new();
    for root in corpus_roots() {
        let target_root = root.join(target);
        if !target_root.exists() {
            continue;
        }

        let mut entries = match fs::read_dir(&target_root) {
            Ok(entries) => entries.collect::<Result<Vec<_>, _>>(),
            Err(error) => panic!("corpus directory should be readable: {error}"),
        }
        .unwrap_or_else(|error| panic!("corpus entry should be readable: {error}"));
        entries.sort_by_key(std::fs::DirEntry::file_name);

        bytes.extend(entries.into_iter().map(|entry| {
            fs::read(entry.path())
                .unwrap_or_else(|error| panic!("corpus file should be readable: {error}"))
        }));
    }

    bytes
}
