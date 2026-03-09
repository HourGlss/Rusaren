#![no_main]

use game_content::parse_skill_yaml;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let input = String::from_utf8_lossy(data);
    let _ = parse_skill_yaml("fuzz/skills/generated.yaml", &input);
});
