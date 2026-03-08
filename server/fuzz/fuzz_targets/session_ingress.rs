#![no_main]

use game_net::NetworkSessionGuard;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut guard = NetworkSessionGuard::new();
    let mut index = 0_usize;
    let mut packets_seen = 0_u8;

    while index < data.len() && packets_seen < 32 {
        let declared_len = usize::from(data[index]);
        index += 1;

        let remaining = data.len().saturating_sub(index);
        let packet_len = declared_len.min(remaining);
        let packet = &data[index..index + packet_len];
        let _ = guard.accept_packet(packet);

        index += packet_len;
        packets_seen = packets_seen.saturating_add(1);
    }
});
