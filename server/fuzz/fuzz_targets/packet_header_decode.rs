#![no_main]

use game_net::PacketHeader;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = PacketHeader::decode(data);
});
