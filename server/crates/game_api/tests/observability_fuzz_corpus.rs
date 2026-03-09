use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use game_api::{classify_http_path, decode_client_signal_message, ServerObservability};

fn corpus_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn corpus_roots() -> Vec<PathBuf> {
    let repo_root = corpus_root();
    [
        repo_root.join("fuzz").join("corpus"),
        repo_root.join("target").join("fuzz-generated-corpus"),
    ]
    .into_iter()
    .filter(|root| root.exists())
    .collect()
}

#[test]
fn replay_http_route_classification_corpus() {
    for bytes in corpus_files("http_route_classification") {
        let path = String::from_utf8_lossy(&bytes);
        let _ = classify_http_path(&path).as_str();
    }
}

#[test]
fn replay_observability_metrics_render_corpus() {
    for bytes in corpus_files("observability_metrics_render") {
        exercise_observability_metrics(&bytes);
    }
}

#[test]
fn replay_webrtc_signal_message_corpus() {
    for bytes in corpus_files("webrtc_signal_message_parse") {
        if let Ok(text) = std::str::from_utf8(&bytes) {
            let _ = decode_client_signal_message(text);
        }
    }
}

fn corpus_files(target: &str) -> Vec<Vec<u8>> {
    let mut bytes = Vec::new();
    for root in corpus_roots() {
        let target_root = root.join(target);
        if !target_root.exists() {
            continue;
        }

        let mut entries = match fs::read_dir(&target_root) {
            Ok(entries) => entries.collect::<Result<Vec<_>, _>>(),
            Err(error) => panic!("corpus directory should be readable: {error}"),
        }
        .unwrap_or_else(|error| panic!("corpus entry should be readable: {error}"));
        entries.sort_by_key(std::fs::DirEntry::file_name);

        bytes.extend(entries.into_iter().map(|entry| {
            fs::read(entry.path())
                .unwrap_or_else(|error| panic!("corpus file should be readable: {error}"))
        }));
    }

    bytes
}

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
