use std::fs;
use std::path::PathBuf;

use game_content::{parse_ascii_map, parse_skill_yaml};

fn server_roots() -> Vec<PathBuf> {
    let mut roots = vec![PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")];
    if let Ok(path) = std::env::var("RARENA_SERVER_ROOT") {
        if !path.trim().is_empty() {
            roots.push(PathBuf::from(path));
        }
    }

    let mut unique = Vec::new();
    for root in roots {
        let canonical = fs::canonicalize(&root).unwrap_or(root);
        if !unique
            .iter()
            .any(|existing: &PathBuf| existing == &canonical)
        {
            unique.push(canonical);
        }
    }

    unique
}

fn corpus_roots() -> Vec<PathBuf> {
    server_roots()
        .into_iter()
        .flat_map(|root| {
            [
                root.join("target").join("fuzz-seed-corpus"),
                root.join("target").join("fuzz-generated-corpus"),
                root.join("fuzz").join("corpus"),
            ]
        })
        .filter(|root| root.exists())
        .collect()
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
