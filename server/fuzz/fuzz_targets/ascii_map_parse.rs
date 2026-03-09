#![no_main]

use game_content::parse_ascii_map;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let input = String::from_utf8_lossy(data);
    let _ = parse_ascii_map("fuzz/maps/generated.txt", &input);
});
