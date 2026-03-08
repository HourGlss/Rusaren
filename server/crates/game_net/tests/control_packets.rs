#![allow(clippy::expect_used)]

use game_domain::{
    LobbyId, MatchId, MatchOutcome, PlayerId, PlayerName, PlayerRecord, ReadyState, RoundNumber,
    SkillTree, TeamSide,
};
use game_net::{
    ChannelId, ClientControlCommand, LobbyDirectoryEntry, LobbySnapshotPhase, LobbySnapshotPlayer,
    PacketError, PacketHeader, PacketKind, ServerControlEvent,
};

fn player_id(raw: u32) -> PlayerId {
    PlayerId::new(raw).expect("valid player id")
}

fn player_name(raw: &str) -> PlayerName {
    PlayerName::new(raw).expect("valid player name")
}

fn lobby_id(raw: u32) -> LobbyId {
    LobbyId::new(raw).expect("valid lobby id")
}

#[test]
fn client_control_command_round_trips_valid_packets() {
    let command = ClientControlCommand::Connect {
        player_id: player_id(7),
        player_name: player_name("Alice"),
    };
    let packet = command.clone().encode_packet(3, 11).expect("packet");

    let (header, decoded) = ClientControlCommand::decode_packet(&packet).expect("decode");
    assert_eq!(header.channel_id, ChannelId::Control);
    assert_eq!(header.packet_kind, PacketKind::ControlCommand);
    assert_eq!(decoded, command);
}

#[test]
fn client_control_command_round_trips_all_variants() {
    let commands = vec![
        ClientControlCommand::CreateGameLobby,
        ClientControlCommand::JoinGameLobby {
            lobby_id: lobby_id(3),
        },
        ClientControlCommand::LeaveGameLobby,
        ClientControlCommand::SelectTeam {
            team: TeamSide::TeamA,
        },
        ClientControlCommand::SetReady {
            ready: ReadyState::Ready,
        },
        ClientControlCommand::ChooseSkill {
            tree: SkillTree::Mage,
            tier: 3,
        },
        ClientControlCommand::QuitToCentralLobby,
    ];

    for (offset, command) in commands.into_iter().enumerate() {
        let packet = command
            .clone()
            .encode_packet(u32::try_from(offset + 1).expect("seq fits"), 11)
            .expect("packet");
        let (_, decoded) = ClientControlCommand::decode_packet(&packet).expect("decode");
        assert_eq!(decoded, command);
    }
}

#[test]
fn client_control_command_rejects_invalid_ids_enums_and_trailing_bytes() {
    let header = PacketHeader::new(ChannelId::Control, PacketKind::ControlCommand, 0, 6, 1, 1)
        .expect("header");

    let packet = header.encode(&[1, 0, 0, 0, 0, 0]);
    assert_eq!(
        ClientControlCommand::decode_packet(&packet),
        Err(PacketError::InvalidEncodedPlayerId(0))
    );

    let header = PacketHeader::new(ChannelId::Control, PacketKind::ControlCommand, 0, 3, 1, 1)
        .expect("header");
    let packet = header.encode(&[5, 9, 0]);
    assert_eq!(
        ClientControlCommand::decode_packet(&packet),
        Err(PacketError::InvalidEncodedTeam(9))
    );

    let header = PacketHeader::new(ChannelId::Control, PacketKind::ControlCommand, 0, 2, 1, 1)
        .expect("header");
    let packet = header.encode(&[4, 99]);
    assert_eq!(
        ClientControlCommand::decode_packet(&packet),
        Err(PacketError::UnexpectedTrailingBytes {
            kind: "ClientControlCommand",
            actual: 1,
        })
    );
}

#[test]
fn client_control_command_rejects_wrong_packet_kinds_and_bad_names() {
    let wrong = PacketHeader::new(ChannelId::Control, PacketKind::ControlEvent, 0, 1, 1, 1)
        .expect("header")
        .encode(&[2]);
    assert_eq!(
        ClientControlCommand::decode_packet(&wrong),
        Err(PacketError::UnexpectedPacketKind {
            expected_channel: ChannelId::Control,
            expected_kind: PacketKind::ControlCommand,
            actual_channel: ChannelId::Control,
            actual_kind: PacketKind::ControlEvent,
        })
    );

    let long_name = "A".repeat(game_domain::MAX_PLAYER_NAME_LEN + 1);
    let mut payload = vec![1];
    payload.extend_from_slice(&1_u32.to_le_bytes());
    payload.push(u8::try_from(long_name.len()).expect("name length should fit in u8"));
    payload.extend_from_slice(long_name.as_bytes());
    let header = PacketHeader::new(
        ChannelId::Control,
        PacketKind::ControlCommand,
        0,
        u16::try_from(payload.len()).expect("payload length should fit in u16"),
        1,
        1,
    )
    .expect("header");
    let packet = header.encode(&payload);
    assert_eq!(
        ClientControlCommand::decode_packet(&packet),
        Err(PacketError::StringLengthOutOfBounds {
            field: "player_name",
            len: game_domain::MAX_PLAYER_NAME_LEN + 1,
            max: game_domain::MAX_PLAYER_NAME_LEN,
        })
    );
}

#[test]
fn server_control_event_round_trips_valid_packets() {
    let event = ServerControlEvent::MatchEnded {
        outcome: MatchOutcome::NoContest,
        score_a: 2,
        score_b: 1,
        message: String::from("Bob has disconnected. Game is over."),
    };
    let packet = event.clone().encode_packet(8, 21).expect("packet");

    let (header, decoded) = ServerControlEvent::decode_packet(&packet).expect("decode");
    assert_eq!(header.channel_id, ChannelId::Control);
    assert_eq!(header.packet_kind, PacketKind::ControlEvent);
    assert_eq!(decoded, event);
}

#[test]
fn server_control_event_round_trips_all_scalar_variants() {
    let events = vec![
        ServerControlEvent::GameLobbyCreated {
            lobby_id: lobby_id(1),
        },
        ServerControlEvent::GameLobbyJoined {
            lobby_id: lobby_id(1),
            player_id: player_id(7),
        },
        ServerControlEvent::GameLobbyLeft {
            lobby_id: lobby_id(1),
            player_id: player_id(7),
        },
        ServerControlEvent::TeamSelected {
            player_id: player_id(7),
            team: TeamSide::TeamA,
            ready_reset: true,
        },
        ServerControlEvent::ReadyChanged {
            player_id: player_id(7),
            ready: ReadyState::Ready,
        },
        ServerControlEvent::LaunchCountdownStarted {
            lobby_id: lobby_id(1),
            seconds_remaining: 5,
            roster_size: 2,
        },
        ServerControlEvent::LaunchCountdownTick {
            lobby_id: lobby_id(1),
            seconds_remaining: 4,
        },
        ServerControlEvent::MatchStarted {
            match_id: MatchId::new(9).expect("match id"),
            round: RoundNumber::new(1).expect("round"),
            skill_pick_seconds: 25,
        },
        ServerControlEvent::SkillChosen {
            player_id: player_id(7),
            tree: SkillTree::Rogue,
            tier: 4,
        },
        ServerControlEvent::PreCombatStarted {
            seconds_remaining: 5,
        },
        ServerControlEvent::CombatStarted,
        ServerControlEvent::RoundWon {
            round: RoundNumber::new(1).expect("round"),
            winning_team: TeamSide::TeamB,
            score_a: 0,
            score_b: 1,
        },
        ServerControlEvent::ReturnedToCentralLobby {
            record: PlayerRecord {
                wins: 1,
                losses: 0,
                no_contests: 1,
            },
        },
        ServerControlEvent::Error {
            message: String::from("bad packet"),
        },
    ];

    for (offset, event) in events.into_iter().enumerate() {
        let packet = event
            .clone()
            .encode_packet(u32::try_from(offset + 1).expect("seq fits"), 42)
            .expect("packet");
        let (_, decoded) = ServerControlEvent::decode_packet(&packet).expect("decode");
        assert_eq!(decoded, event);
    }
}

#[test]
fn server_control_event_round_trips_lobby_directory_and_snapshot_packets() {
    let directory_event = ServerControlEvent::LobbyDirectorySnapshot {
        lobbies: vec![LobbyDirectoryEntry {
            lobby_id: lobby_id(3),
            player_count: 4,
            team_a_count: 1,
            team_b_count: 2,
            ready_count: 3,
            phase: LobbySnapshotPhase::LaunchCountdown {
                seconds_remaining: 4,
            },
        }],
    };
    let snapshot_event = ServerControlEvent::GameLobbySnapshot {
        lobby_id: lobby_id(3),
        phase: LobbySnapshotPhase::Open,
        players: vec![LobbySnapshotPlayer {
            player_id: player_id(7),
            player_name: player_name("Alice"),
            record: PlayerRecord {
                wins: 1,
                losses: 2,
                no_contests: 3,
            },
            team: Some(TeamSide::TeamA),
            ready: ReadyState::Ready,
        }],
    };

    let directory_packet = directory_event.clone().encode_packet(2, 3).expect("packet");
    let snapshot_packet = snapshot_event.clone().encode_packet(4, 5).expect("packet");

    let (_, decoded_directory) =
        ServerControlEvent::decode_packet(&directory_packet).expect("decode");
    let (_, decoded_snapshot) =
        ServerControlEvent::decode_packet(&snapshot_packet).expect("decode");

    assert_eq!(decoded_directory, directory_event);
    assert_eq!(decoded_snapshot, snapshot_event);
}

#[test]
fn server_control_event_rejects_bad_payloads_and_unknown_variants() {
    let header = PacketHeader::new(ChannelId::Control, PacketKind::ControlEvent, 0, 1, 1, 1)
        .expect("header");
    let packet = header.encode(&[99]);
    assert_eq!(
        ServerControlEvent::decode_packet(&packet),
        Err(PacketError::UnknownServerEvent(99))
    );

    let header = PacketHeader::new(ChannelId::Control, PacketKind::ControlEvent, 0, 4, 1, 1)
        .expect("header");
    let packet = header.encode(&[14, 9, 0, 0]);
    assert_eq!(
        ServerControlEvent::decode_packet(&packet),
        Err(PacketError::InvalidEncodedMatchOutcome(9))
    );
}

#[test]
fn server_control_event_rejects_invalid_snapshot_phase_and_optional_team_values() {
    let mut payload = vec![17];
    payload.extend_from_slice(&1_u16.to_le_bytes());
    payload.extend_from_slice(&1_u32.to_le_bytes());
    payload.extend_from_slice(&0_u16.to_le_bytes());
    payload.extend_from_slice(&0_u16.to_le_bytes());
    payload.extend_from_slice(&0_u16.to_le_bytes());
    payload.extend_from_slice(&0_u16.to_le_bytes());
    payload.push(9);
    let header = PacketHeader::new(
        ChannelId::Control,
        PacketKind::ControlEvent,
        0,
        u16::try_from(payload.len()).expect("payload length should fit in u16"),
        1,
        1,
    )
    .expect("header");
    let packet = header.encode(&payload);
    assert_eq!(
        ServerControlEvent::decode_packet(&packet),
        Err(PacketError::InvalidEncodedLobbyPhase(9))
    );

    let mut payload = vec![18];
    payload.extend_from_slice(&1_u32.to_le_bytes());
    payload.push(0);
    payload.extend_from_slice(&1_u16.to_le_bytes());
    payload.extend_from_slice(&7_u32.to_le_bytes());
    payload.push(5);
    payload.extend_from_slice(b"Alice");
    payload.extend_from_slice(&0_u16.to_le_bytes());
    payload.extend_from_slice(&0_u16.to_le_bytes());
    payload.extend_from_slice(&0_u16.to_le_bytes());
    payload.push(9);
    payload.push(0);
    let header = PacketHeader::new(
        ChannelId::Control,
        PacketKind::ControlEvent,
        0,
        u16::try_from(payload.len()).expect("payload length should fit in u16"),
        1,
        1,
    )
    .expect("header");
    let packet = header.encode(&payload);
    assert_eq!(
        ServerControlEvent::decode_packet(&packet),
        Err(PacketError::InvalidEncodedOptionalTeam(9))
    );
}

#[test]
fn server_control_event_rejects_invalid_records_and_wrong_packet_kind() {
    let wrong = PacketHeader::new(ChannelId::Control, PacketKind::ControlCommand, 0, 1, 1, 1)
        .expect("header")
        .encode(&[16]);
    assert_eq!(
        ServerControlEvent::decode_packet(&wrong),
        Err(PacketError::UnexpectedPacketKind {
            expected_channel: ChannelId::Control,
            expected_kind: PacketKind::ControlEvent,
            actual_channel: ChannelId::Control,
            actual_kind: PacketKind::ControlCommand,
        })
    );

    let event = ServerControlEvent::GameLobbyCreated {
        lobby_id: lobby_id(1),
    };
    let packet = event.clone().encode_packet(1, 1).expect("packet");
    let (_, decoded) = ServerControlEvent::decode_packet(&packet).expect("decode");
    assert_eq!(decoded, event);
}
