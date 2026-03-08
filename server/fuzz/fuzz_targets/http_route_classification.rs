#![no_main]

use game_api::classify_http_path;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let path = String::from_utf8_lossy(data);
    let _ = classify_http_path(&path).as_str();
});
