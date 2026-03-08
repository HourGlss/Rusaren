#![no_main]

use game_api::canonicalize_record_store_contents;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let input = String::from_utf8_lossy(data);
    let _ = canonicalize_record_store_contents(&input);
});
