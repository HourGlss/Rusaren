use std::{fs, path::PathBuf};

use game_net::{
    ClientControlCommand, NetworkSessionGuard, PacketHeader, ServerControlEvent,
    ValidatedInputFrame,
};

fn corpus_dir(target: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("fuzz")
        .join("corpus")
        .join(target);

    fs::canonicalize(&dir).unwrap_or(dir)
}

fn corpus_files(target: &str) -> Vec<Vec<u8>> {
    let dir = corpus_dir(target);
    let mut entries = fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("failed to read corpus directory {}: {error}", dir.display()))
        .map(|entry| {
            entry.unwrap_or_else(|error| {
                panic!("failed to read corpus entry in {}: {error}", dir.display())
            })
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    assert!(
        !entries.is_empty(),
        "corpus directory {} should contain at least one seed",
        dir.display()
    );

    entries
        .into_iter()
        .map(|entry| {
            fs::read(entry.path())
                .unwrap_or_else(|error| panic!("failed to read corpus file {}: {error}", entry.path().display()))
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
        let mut guard = NetworkSessionGuard::new();
        let mut index = 0_usize;
        let mut packets_seen = 0_u8;

        while index < bytes.len() && packets_seen < 32 {
            let declared_len = usize::from(bytes[index]);
            index += 1;

            let remaining = bytes.len().saturating_sub(index);
            let packet_len = declared_len.min(remaining);
            let packet = &bytes[index..index + packet_len];
            consume_result(guard.accept_packet(packet));

            index += packet_len;
            packets_seen = packets_seen.saturating_add(1);
        }
    }
}

#[test]
fn replay_server_control_event_corpus() {
    for bytes in corpus_files("server_control_event_decode") {
        consume_result(ServerControlEvent::decode_packet(&bytes));
    }
}

fn consume_result<T, E: std::fmt::Display>(result: Result<T, E>) {
    if let Err(error) = result {
        let _ = error.to_string();
    }
}
