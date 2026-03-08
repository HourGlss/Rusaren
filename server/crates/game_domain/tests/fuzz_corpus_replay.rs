use std::{fs, path::PathBuf};

use game_domain::{LoadoutProgress, SkillChoice, SkillTree};

fn corpus_dir(target: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("fuzz")
        .join("corpus")
        .join(target);

    fs::canonicalize(&dir).unwrap_or(dir)
}

fn corpus_files(target: &str) -> Vec<Vec<u8>> {
    let dir = corpus_dir(target);
    let mut entries = fs::read_dir(&dir)
        .unwrap_or_else(|error| {
            panic!("failed to read corpus directory {}: {error}", dir.display())
        })
        .map(|entry| {
            entry.unwrap_or_else(|error| {
                panic!("failed to read corpus entry in {}: {error}", dir.display())
            })
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    assert!(
        !entries.is_empty(),
        "corpus directory {} should contain at least one seed",
        dir.display()
    );

    entries
        .into_iter()
        .map(|entry| {
            fs::read(entry.path()).unwrap_or_else(|error| {
                panic!(
                    "failed to read corpus file {}: {error}",
                    entry.path().display()
                )
            })
        })
        .collect()
}

fn tree_from_byte(raw: u8) -> SkillTree {
    match raw % 4 {
        0 => SkillTree::Warrior,
        1 => SkillTree::Rogue,
        2 => SkillTree::Mage,
        _ => SkillTree::Cleric,
    }
}

#[test]
fn replay_skill_progression_corpus() {
    for bytes in corpus_files("skill_progression") {
        let mut progress = LoadoutProgress::new();
        for chunk in bytes.chunks_exact(2).take(64) {
            let tree = tree_from_byte(chunk[0]);
            let tier = chunk[1];
            if let Ok(choice) = SkillChoice::new(tree, tier) {
                let _ = progress.apply(choice);
            }
        }
    }
}
