#![no_main]

use game_net::ServerControlEvent;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = ServerControlEvent::decode_packet(data);
});
