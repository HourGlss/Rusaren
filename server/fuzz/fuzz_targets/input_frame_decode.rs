#![no_main]

use game_net::ValidatedInputFrame;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = ValidatedInputFrame::decode_packet(data);
});
