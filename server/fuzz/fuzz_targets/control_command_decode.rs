#![no_main]

use game_net::ClientControlCommand;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = ClientControlCommand::decode_packet(data);
});
