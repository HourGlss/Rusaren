use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use game_api::{classify_http_path, ServerObservability};

fn corpus_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("fuzz")
        .join("corpus")
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

fn corpus_files(target: &str) -> Vec<Vec<u8>> {
    let root = corpus_root().join(target);
    if !root.exists() {
        return Vec::new();
    }

    let mut entries = match fs::read_dir(&root) {
        Ok(entries) => entries.collect::<Result<Vec<_>, _>>(),
        Err(error) => panic!("corpus directory should be readable: {error}"),
    }
    .unwrap_or_else(|error| panic!("corpus entry should be readable: {error}"));
    entries.sort_by_key(std::fs::DirEntry::file_name);

    entries
        .into_iter()
        .map(|entry| {
            fs::read(entry.path())
                .unwrap_or_else(|error| panic!("corpus file should be readable: {error}"))
        })
        .collect()
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
        match opcode % 13 {
            0 => observability.record_http_request(classify_http_path("/")),
            1 => observability.record_http_request(classify_http_path("/healthz")),
            2 => observability.record_http_request(classify_http_path("/metrics")),
            3 => observability.record_http_request(classify_http_path("/ws")),
            4 => observability.record_http_request(classify_http_path("/assets/client/game.wasm")),
            5 => observability.record_websocket_upgrade_attempt(),
            6 => observability.record_websocket_session_bound(),
            7 => observability.record_websocket_disconnect(),
            8 => observability.record_websocket_rejection(),
            9 => observability.record_ingress_packet(true),
            10 => observability.record_ingress_packet(false),
            11 => observability.record_tick(Duration::from_micros(u64::from(*opcode))),
            12 => observability.record_tick(Duration::from_millis(u64::from(*opcode) + 1)),
            _ => unreachable!(),
        }
    }

    let rendered = observability.render_prometheus();
    let _ = rendered.len();
}
