#![no_main]

#[path = "../support/game_net/mod.rs"]
mod game_net_support;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    game_net_support::ingress::run_prefixed_session_ingress_stream(data);
});
