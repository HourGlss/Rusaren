use std::fs;
use std::path::PathBuf;

use game_content::{parse_ascii_map, parse_skill_yaml};

fn corpus_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("fuzz")
        .join("corpus")
}

#[test]
fn replay_ascii_map_parse_corpus() {
    for bytes in corpus_files("ascii_map_parse") {
        let input = String::from_utf8_lossy(&bytes);
        let _ = parse_ascii_map("fuzz/maps/replay.txt", &input);
    }
}

#[test]
fn replay_skill_yaml_parse_corpus() {
    for bytes in corpus_files("skill_yaml_parse") {
        let input = String::from_utf8_lossy(&bytes);
        let _ = parse_skill_yaml("fuzz/skills/replay.yaml", &input);
    }
}

fn corpus_files(target: &str) -> Vec<Vec<u8>> {
    let root = corpus_root().join(target);
    if !root.exists() {
        return Vec::new();
    }

    let mut entries = match fs::read_dir(&root) {
        Ok(entries) => entries.collect::<Result<Vec<_>, _>>(),
        Err(error) => panic!("corpus directory should be readable: {error}"),
    }
    .unwrap_or_else(|error| panic!("corpus entry should be readable: {error}"));
    entries.sort_by_key(std::fs::DirEntry::file_name);

    entries
        .into_iter()
        .map(|entry| {
            fs::read(entry.path())
                .unwrap_or_else(|error| panic!("corpus file should be readable: {error}"))
        })
        .collect()
}
