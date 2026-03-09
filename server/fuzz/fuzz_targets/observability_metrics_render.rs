#![no_main]

use std::time::Duration;

use game_api::{classify_http_path, ServerObservability};
use libfuzzer_sys::fuzz_target;

fn exercise_observability_metrics(data: &[u8]) {
    let version_len = data.first().map_or(0, |byte| {
        usize::from(*byte).min(32).min(data.len().saturating_sub(1))
    });
    let version_end = 1 + version_len;
    let version_bytes = if version_len == 0 {
        &[][..]
    } else {
        &data[1..version_end]
    };
    let operations = if data.len() > version_end {
        &data[version_end..]
    } else {
        &[][..]
    };

    let observability = ServerObservability::new(String::from_utf8_lossy(version_bytes));

    for opcode in operations {
        match opcode % 14 {
            0 => observability.record_http_request(classify_http_path("/")),
            1 => observability.record_http_request(classify_http_path("/healthz")),
            2 => observability.record_http_request(classify_http_path("/metrics")),
            3 => observability.record_http_request(classify_http_path("/session/bootstrap")),
            4 => observability.record_http_request(classify_http_path("/ws")),
            5 => observability.record_http_request(classify_http_path("/assets/client/game.wasm")),
            6 => observability.record_websocket_upgrade_attempt(),
            7 => observability.record_websocket_session_bound(),
            8 => observability.record_websocket_disconnect(),
            9 => observability.record_websocket_rejection(),
            10 => observability.record_ingress_packet(true),
            11 => observability.record_ingress_packet(false),
            12 => observability.record_tick(Duration::from_micros(u64::from(*opcode))),
            13 => observability.record_tick(Duration::from_millis(u64::from(*opcode) + 1)),
            _ => unreachable!(),
        }
    }

    let rendered = observability.render_prometheus();
    let _ = rendered.len();
}

fuzz_target!(|data: &[u8]| {
    exercise_observability_metrics(data);
});
