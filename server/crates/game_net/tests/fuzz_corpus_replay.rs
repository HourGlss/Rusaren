use std::{fs, path::PathBuf};

use game_net::{ClientControlCommand, PacketHeader, ServerControlEvent, ValidatedInputFrame};

#[path = "../../../fuzz/support/game_net/mod.rs"]
mod game_net_support;

fn server_roots() -> Vec<PathBuf> {
    let mut roots = vec![PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")];
    if let Ok(path) = std::env::var("RARENA_SERVER_ROOT") {
        if !path.trim().is_empty() {
            roots.push(PathBuf::from(path));
        }
    }

    let mut unique = Vec::new();
    for root in roots {
        let canonical = fs::canonicalize(&root).unwrap_or(root);
        if !unique
            .iter()
            .any(|existing: &PathBuf| existing == &canonical)
        {
            unique.push(canonical);
        }
    }

    unique
}

fn corpus_dirs(target: &str) -> Vec<PathBuf> {
    server_roots()
        .into_iter()
        .flat_map(|root| {
            [
                root.join("fuzz").join("corpus").join(target),
                root.join("target")
                    .join("fuzz-generated-corpus")
                    .join(target),
            ]
        })
        .filter(|dir| dir.exists())
        .map(|dir| fs::canonicalize(&dir).unwrap_or(dir))
        .collect()
}

fn corpus_files(target: &str) -> Vec<Vec<u8>> {
    let mut entries = Vec::new();
    let dirs = corpus_dirs(target);
    for dir in &dirs {
        let mut dir_entries = fs::read_dir(dir)
            .unwrap_or_else(|error| {
                panic!("failed to read corpus directory {}: {error}", dir.display())
            })
            .map(|entry| {
                entry.unwrap_or_else(|error| {
                    panic!("failed to read corpus entry in {}: {error}", dir.display())
                })
            })
            .collect::<Vec<_>>();
        entries.append(&mut dir_entries);
    }
    entries.sort_by_key(std::fs::DirEntry::file_name);
    assert!(
        !entries.is_empty(),
        "at least one checked-in or generated corpus file should exist for target {target}"
    );

    entries
        .into_iter()
        .map(|entry| {
            fs::read(entry.path()).unwrap_or_else(|error| {
                panic!(
                    "failed to read corpus file {}: {error}",
                    entry.path().display()
                )
            })
        })
        .collect()
}

#[test]
fn replay_packet_header_corpus() {
    for bytes in corpus_files("packet_header_decode") {
        consume_result(PacketHeader::decode(&bytes));
    }
}

#[test]
fn replay_control_command_corpus() {
    for bytes in corpus_files("control_command_decode") {
        consume_result(ClientControlCommand::decode_packet(&bytes));
    }
}

#[test]
fn replay_input_frame_corpus() {
    for bytes in corpus_files("input_frame_decode") {
        consume_result(ValidatedInputFrame::decode_packet(&bytes));
    }
}

#[test]
fn replay_session_ingress_corpus() {
    for bytes in corpus_files("session_ingress") {
        game_net_support::ingress::run_prefixed_session_ingress_stream(&bytes);
    }
}

#[test]
fn replay_session_ingress_sequence_corpus() {
    for bytes in corpus_files("session_ingress_sequence") {
        game_net_support::ingress::run_session_ingress_sequence(&bytes);
    }
}

#[test]
fn replay_server_control_event_corpus() {
    for bytes in corpus_files("server_control_event_decode") {
        consume_result(ServerControlEvent::decode_packet(&bytes));
    }
}

#[test]
fn replay_server_control_event_roundtrip_corpus() {
    for bytes in corpus_files("server_control_event_roundtrip") {
        game_net_support::events::run_server_control_event_roundtrip(&bytes);
    }
}

#[test]
fn replay_arena_full_snapshot_corpus() {
    for bytes in corpus_files("arena_full_snapshot_decode") {
        consume_result(ServerControlEvent::decode_packet(&bytes));
    }
}

#[test]
fn replay_arena_full_snapshot_roundtrip_corpus() {
    for bytes in corpus_files("arena_full_snapshot_roundtrip") {
        game_net_support::events::run_arena_full_snapshot_roundtrip(&bytes);
    }
}

#[test]
fn replay_arena_delta_snapshot_corpus() {
    for bytes in corpus_files("arena_delta_snapshot_decode") {
        consume_result(ServerControlEvent::decode_packet(&bytes));
    }
}

#[test]
fn replay_arena_delta_snapshot_roundtrip_corpus() {
    for bytes in corpus_files("arena_delta_snapshot_roundtrip") {
        game_net_support::events::run_arena_delta_snapshot_roundtrip(&bytes);
    }
}

fn consume_result<T, E: std::fmt::Display>(result: Result<T, E>) {
    if let Err(error) = result {
        let _ = error.to_string();
    }
}
