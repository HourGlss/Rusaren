#![forbid(unsafe_code)]

use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};

use game_domain::{LobbyId, PlayerId, PlayerName, ReadyState, SkillTree, TeamSide};
use game_net::{
    ChannelId, ClientControlCommand, LobbyDirectoryEntry, LobbySnapshotPhase,
    LobbySnapshotPlayer, PacketHeader, PacketKind, ServerControlEvent, ValidatedInputFrame,
    BUTTON_CAST, BUTTON_PRIMARY,
};

fn main() -> Result<(), Box<dyn Error>> {
    let corpus_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("fuzz")
        .join("corpus");

    write_packet_header_corpus(&corpus_root.join("packet_header_decode"))?;
    write_control_command_corpus(&corpus_root.join("control_command_decode"))?;
    write_input_frame_corpus(&corpus_root.join("input_frame_decode"))?;
    write_session_ingress_corpus(&corpus_root.join("session_ingress"))?;
    write_server_control_event_corpus(&corpus_root.join("server_control_event_decode"))?;

    println!("Seed corpora written under {}", corpus_root.display());
    Ok(())
}

fn write_packet_header_corpus(dir: &Path) -> Result<(), Box<dyn Error>> {
    recreate_dir(dir)?;

    let valid = PacketHeader::new(ChannelId::Control, PacketKind::ControlCommand, 0, 0, 1, 0)?
        .encode(&[]);
    let mut bad_magic = valid.clone();
    bad_magic[0] = 0;
    let mut bad_version = valid.clone();
    bad_version[2] = 99;

    write_seed(dir, "empty.bin", &[])?;
    write_seed(dir, "valid_control_header.bin", &valid)?;
    write_seed(dir, "bad_magic.bin", &bad_magic)?;
    write_seed(dir, "bad_version.bin", &bad_version)?;
    Ok(())
}

fn write_control_command_corpus(dir: &Path) -> Result<(), Box<dyn Error>> {
    recreate_dir(dir)?;

    let connect = ClientControlCommand::Connect {
        player_id: player_id(7)?,
        player_name: player_name("Alice")?,
    }
    .encode_packet(1, 0)?;
    let create = ClientControlCommand::CreateGameLobby.encode_packet(2, 0)?;
    let join = ClientControlCommand::JoinGameLobby {
        lobby_id: lobby_id(3)?,
    }
    .encode_packet(3, 0)?;
    let invalid_kind =
        PacketHeader::new(ChannelId::Control, PacketKind::ControlCommand, 0, 1, 4, 0)?
            .encode(&[255]);
    let truncated = connect[..connect.len() - 1].to_vec();

    write_seed(dir, "connect_valid.bin", &connect)?;
    write_seed(dir, "create_valid.bin", &create)?;
    write_seed(dir, "join_valid.bin", &join)?;
    write_seed(dir, "invalid_kind.bin", &invalid_kind)?;
    write_seed(dir, "truncated_connect.bin", &truncated)?;
    Ok(())
}

fn write_input_frame_corpus(dir: &Path) -> Result<(), Box<dyn Error>> {
    recreate_dir(dir)?;

    let cast = ValidatedInputFrame::new(3, 1, -1, 50, -50, BUTTON_CAST, 9)?.encode_packet(17, 99)?;
    let movement =
        ValidatedInputFrame::new(4, 25, -25, 0, 0, BUTTON_PRIMARY, 0)?.encode_packet(18, 100)?;
    let truncated = cast[..cast.len() - 1].to_vec();

    let mut bad_buttons_payload = [0_u8; 16];
    bad_buttons_payload[12..14].copy_from_slice(&0x8000_u16.to_le_bytes());
    let bad_buttons =
        PacketHeader::new(ChannelId::Input, PacketKind::InputFrame, 0, 16, 19, 101)?
            .encode(&bad_buttons_payload);

    write_seed(dir, "cast_valid.bin", &cast)?;
    write_seed(dir, "movement_valid.bin", &movement)?;
    write_seed(dir, "truncated_cast.bin", &truncated)?;
    write_seed(dir, "invalid_buttons.bin", &bad_buttons)?;
    Ok(())
}

fn write_session_ingress_corpus(dir: &Path) -> Result<(), Box<dyn Error>> {
    recreate_dir(dir)?;

    let connect = ClientControlCommand::Connect {
        player_id: player_id(11)?,
        player_name: player_name("Mallory")?,
    }
    .encode_packet(1, 0)?;
    let select_team = ClientControlCommand::SelectTeam {
        team: TeamSide::TeamA,
    }
    .encode_packet(2, 0)?;
    let set_ready = ClientControlCommand::SetReady {
        ready: ReadyState::Ready,
    }
    .encode_packet(3, 0)?;
    let reconnect = ClientControlCommand::Connect {
        player_id: player_id(12)?,
        player_name: player_name("Eve")?,
    }
    .encode_packet(4, 0)?;
    let choose_skill = ClientControlCommand::ChooseSkill {
        tree: SkillTree::Mage,
        tier: 1,
    }
    .encode_packet(5, 0)?;

    let valid_stream = prefix_packets(&[connect.clone(), select_team, set_ready]);
    let invalid_first = prefix_packets(&[choose_skill]);
    let rebinding = prefix_packets(&[connect, reconnect]);

    write_seed(dir, "valid_bind_then_ready.bin", &valid_stream)?;
    write_seed(dir, "invalid_first_packet.bin", &invalid_first)?;
    write_seed(dir, "rebinding_attempt.bin", &rebinding)?;
    Ok(())
}

fn write_server_control_event_corpus(dir: &Path) -> Result<(), Box<dyn Error>> {
    recreate_dir(dir)?;

    let connected = ServerControlEvent::Connected {
        player_id: player_id(7)?,
        player_name: player_name("Alice")?,
        record: game_domain::PlayerRecord {
            wins: 1,
            losses: 2,
            no_contests: 3,
        },
    }
    .encode_packet(1, 0)?;
    let directory = ServerControlEvent::LobbyDirectorySnapshot {
        lobbies: vec![LobbyDirectoryEntry {
            lobby_id: lobby_id(3)?,
            player_count: 2,
            team_a_count: 1,
            team_b_count: 1,
            ready_count: 2,
            phase: LobbySnapshotPhase::LaunchCountdown {
                seconds_remaining: 5,
            },
        }],
    }
    .encode_packet(2, 10)?;
    let snapshot = ServerControlEvent::GameLobbySnapshot {
        lobby_id: lobby_id(3)?,
        phase: LobbySnapshotPhase::Open,
        players: vec![
            LobbySnapshotPlayer {
                player_id: player_id(7)?,
                player_name: player_name("Alice")?,
                record: game_domain::PlayerRecord::new(),
                team: Some(TeamSide::TeamA),
                ready: ReadyState::Ready,
            },
            LobbySnapshotPlayer {
                player_id: player_id(8)?,
                player_name: player_name("Bob")?,
                record: game_domain::PlayerRecord {
                    wins: 4,
                    losses: 1,
                    no_contests: 0,
                },
                team: Some(TeamSide::TeamB),
                ready: ReadyState::NotReady,
            },
        ],
    }
    .encode_packet(3, 11)?;
    let truncated = snapshot[..snapshot.len() - 1].to_vec();

    let mut invalid_optional_team_payload = vec![18];
    invalid_optional_team_payload.extend_from_slice(&3_u32.to_le_bytes());
    invalid_optional_team_payload.push(0);
    invalid_optional_team_payload.extend_from_slice(&1_u16.to_le_bytes());
    invalid_optional_team_payload.extend_from_slice(&7_u32.to_le_bytes());
    invalid_optional_team_payload.push(5);
    invalid_optional_team_payload.extend_from_slice(b"Alice");
    invalid_optional_team_payload.extend_from_slice(&0_u16.to_le_bytes());
    invalid_optional_team_payload.extend_from_slice(&0_u16.to_le_bytes());
    invalid_optional_team_payload.extend_from_slice(&0_u16.to_le_bytes());
    invalid_optional_team_payload.push(9);
    invalid_optional_team_payload.push(0);
    let invalid_optional_team_payload_len = u16::try_from(invalid_optional_team_payload.len())?;
    let invalid_optional_team = PacketHeader::new(
        ChannelId::Control,
        PacketKind::ControlEvent,
        0,
        invalid_optional_team_payload_len,
        4,
        12,
    )?
    .encode(&invalid_optional_team_payload);

    write_seed(dir, "connected_valid.bin", &connected)?;
    write_seed(dir, "directory_valid.bin", &directory)?;
    write_seed(dir, "snapshot_valid.bin", &snapshot)?;
    write_seed(dir, "truncated_snapshot.bin", &truncated)?;
    write_seed(dir, "invalid_optional_team.bin", &invalid_optional_team)?;
    Ok(())
}

fn recreate_dir(path: &Path) -> Result<(), Box<dyn Error>> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }

    fs::create_dir_all(path)?;
    Ok(())
}

fn write_seed(dir: &Path, name: &str, bytes: &[u8]) -> Result<(), Box<dyn Error>> {
    fs::write(dir.join(name), bytes)?;
    Ok(())
}

fn prefix_packets(packets: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for packet in packets {
        let Ok(packet_len) = u8::try_from(packet.len()) else {
            panic!("fuzz seed packet length must fit within u8");
        };
        bytes.push(packet_len);
        bytes.extend_from_slice(packet);
    }

    bytes
}

fn player_id(raw: u32) -> Result<PlayerId, Box<dyn Error>> {
    Ok(PlayerId::new(raw)?)
}

fn lobby_id(raw: u32) -> Result<LobbyId, Box<dyn Error>> {
    Ok(LobbyId::new(raw)?)
}

fn player_name(raw: &str) -> Result<PlayerName, Box<dyn Error>> {
    Ok(PlayerName::new(raw)?)
}
