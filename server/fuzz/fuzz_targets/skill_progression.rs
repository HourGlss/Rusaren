#![no_main]

use game_domain::{LoadoutProgress, SkillChoice, SkillTree};
use libfuzzer_sys::fuzz_target;

fn tree_from_byte(raw: u8) -> SkillTree {
    match raw % 4 {
        0 => SkillTree::Warrior,
        1 => SkillTree::Rogue,
        2 => SkillTree::Mage,
        _ => SkillTree::Cleric,
    }
}

fuzz_target!(|data: &[u8]| {
    let mut progress = LoadoutProgress::new();

    for chunk in data.chunks_exact(2).take(64) {
        let tree = tree_from_byte(chunk[0]);
        let tier = chunk[1];

        if let Ok(choice) = SkillChoice::new(tree, tier) {
            let _ = progress.apply(&choice);
        }
    }
});
