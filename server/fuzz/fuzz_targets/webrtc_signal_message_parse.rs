#![no_main]

use game_api::decode_client_signal_message;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = decode_client_signal_message(text);
    }
});
