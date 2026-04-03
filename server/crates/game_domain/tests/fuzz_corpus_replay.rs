use std::{fs, path::PathBuf};

use game_domain::{LoadoutProgress, SkillChoice, SkillTree};

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

fn corpus_dirs(target: &str) -> Vec<PathBuf> {
    server_roots()
        .into_iter()
        .flat_map(|root| {
            [
                root.join("target").join("fuzz-seed-corpus").join(target),
                root.join("target").join("fuzz-generated-corpus").join(target),
                root.join("fuzz").join("corpus").join(target),
            ]
        })
        .filter(|dir| dir.exists())
        .map(|dir| fs::canonicalize(&dir).unwrap_or(dir))
        .collect()
}

fn corpus_files(target: &str) -> Vec<Vec<u8>> {
    let dirs = corpus_dirs(target);
    assert!(
        !dirs.is_empty(),
        "at least one seed or generated corpus directory should exist for target {target}"
    );

    let mut entries = dirs
        .iter()
        .flat_map(|dir| {
            fs::read_dir(dir)
                .unwrap_or_else(|error| {
                    panic!("failed to read corpus directory {}: {error}", dir.display())
                })
                .map(|entry| {
                    entry.unwrap_or_else(|error| {
                        panic!("failed to read corpus entry in {}: {error}", dir.display())
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    assert!(
        !entries.is_empty(),
        "at least one corpus seed should exist for target {target}"
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
    match raw % 6 {
        0 => SkillTree::Warrior,
        1 => SkillTree::Rogue,
        2 => SkillTree::Mage,
        3 => SkillTree::Cleric,
        4 => custom_tree("Druid"),
        _ => custom_tree("Engineer"),
    }
}

fn custom_tree(name: &str) -> SkillTree {
    match SkillTree::new(name) {
        Ok(tree) => tree,
        Err(error) => panic!("custom skill tree {name} should parse: {error}"),
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
                let _ = progress.apply(&choice);
            }
        }
    }
}
