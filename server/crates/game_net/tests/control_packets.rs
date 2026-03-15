#![allow(clippy::expect_used)]

use game_domain::{
    LobbyId, MatchId, MatchOutcome, PlayerId, PlayerName, PlayerRecord, ReadyState, RoundNumber,
    SkillTree, TeamSide,
};
use game_net::{
    ArenaDeltaSnapshot, ArenaEffectKind, ArenaEffectSnapshot, ArenaMatchPhase, ArenaObstacleKind,
    ArenaObstacleSnapshot, ArenaPlayerSnapshot, ArenaStateSnapshot, ArenaStatusKind,
    ArenaStatusSnapshot, ChannelId, ClientControlCommand, LobbyDirectoryEntry, LobbySnapshotPhase,
    LobbySnapshotPlayer, PacketError, PacketHeader, PacketKind, ServerControlEvent,
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
    let header = PacketHeader::new(ChannelId::Control, PacketKind::ControlCommand, 0, 5, 1, 1)
        .expect("header");
    let packet = header.encode(&[3, 0, 0, 0, 0]);
    assert_eq!(
        ClientControlCommand::decode_packet(&packet),
        Err(PacketError::InvalidEncodedLobbyId(0))
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
fn server_control_event_round_trips_full_arena_snapshot() {
    let arena_state = sample_full_arena_snapshot_event();
    let arena_packet = arena_state.clone().encode_packet(6, 22).expect("packet");
    let (arena_header, decoded_arena_state) =
        ServerControlEvent::decode_packet(&arena_packet).expect("decode");

    assert_eq!(arena_header.channel_id, ChannelId::Snapshot);
    assert_eq!(arena_header.packet_kind, PacketKind::FullSnapshot);
    assert_eq!(decoded_arena_state, arena_state);
}

#[test]
fn server_control_event_round_trips_delta_arena_snapshot() {
    let arena_delta = sample_delta_arena_snapshot_event();
    let delta_packet = arena_delta.clone().encode_packet(7, 22).expect("packet");
    let (delta_header, decoded_arena_delta) =
        ServerControlEvent::decode_packet(&delta_packet).expect("decode");

    assert_eq!(delta_header.channel_id, ChannelId::Snapshot);
    assert_eq!(delta_header.packet_kind, PacketKind::DeltaSnapshot);
    assert_eq!(decoded_arena_delta, arena_delta);
}

#[test]
fn arena_status_kinds_round_trip_for_all_runtime_statuses() {
    let statuses = [
        ArenaStatusKind::Poison,
        ArenaStatusKind::Hot,
        ArenaStatusKind::Chill,
        ArenaStatusKind::Root,
        ArenaStatusKind::Haste,
        ArenaStatusKind::Silence,
        ArenaStatusKind::Stun,
    ];

    for (index, kind) in statuses.into_iter().enumerate() {
        let event = ServerControlEvent::ArenaDeltaSnapshot {
            snapshot: ArenaDeltaSnapshot {
                phase: ArenaMatchPhase::Combat,
                phase_seconds_remaining: None,
                players: vec![ArenaPlayerSnapshot {
                    player_id: player_id(7),
                    player_name: player_name("Alice"),
                    team: TeamSide::TeamA,
                    x: -620,
                    y: 220,
                    aim_x: 90,
                    aim_y: -40,
                    hit_points: 92,
                    max_hit_points: 100,
                    mana: 64,
                    max_mana: 100,
                    alive: true,
                    unlocked_skill_slots: 3,
                    primary_cooldown_remaining_ms: 180,
                    primary_cooldown_total_ms: 650,
                    slot_cooldown_remaining_ms: [0, 0, 800, 0, 0],
                    slot_cooldown_total_ms: [700, 1700, 2200, 0, 0],
                    active_statuses: vec![ArenaStatusSnapshot {
                        source: player_id(8),
                        slot: 2,
                        kind,
                        stacks: 2,
                        remaining_ms: 1400,
                    }],
                }],
                projectiles: vec![],
            },
        };

        let packet = event
            .clone()
            .encode_packet(u32::try_from(index + 1).expect("index should fit"), 22)
            .expect("packet");
        let (_, decoded) = ServerControlEvent::decode_packet(&packet).expect("decode");
        assert_eq!(decoded, event);
    }
}

#[test]
fn server_control_event_round_trips_arena_effect_batch() {
    let effect_batch = sample_arena_effect_batch_event();
    let effects_packet = effect_batch.clone().encode_packet(8, 22).expect("packet");
    let (effects_header, decoded_effect_batch) =
        ServerControlEvent::decode_packet(&effects_packet).expect("decode");

    assert_eq!(effects_header.channel_id, ChannelId::Snapshot);
    assert_eq!(effects_header.packet_kind, PacketKind::EventBatch);
    assert_eq!(decoded_effect_batch, effect_batch);
}

fn sample_full_arena_snapshot_event() -> ServerControlEvent {
    ServerControlEvent::ArenaStateSnapshot {
        snapshot: ArenaStateSnapshot {
            phase: ArenaMatchPhase::Combat,
            phase_seconds_remaining: None,
            width: 1800,
            height: 1200,
            obstacles: vec![
                ArenaObstacleSnapshot {
                    kind: ArenaObstacleKind::Shrub,
                    center_x: -220,
                    center_y: -150,
                    half_width: 92,
                    half_height: 92,
                },
                ArenaObstacleSnapshot {
                    kind: ArenaObstacleKind::Pillar,
                    center_x: -220,
                    center_y: -150,
                    half_width: 70,
                    half_height: 70,
                },
            ],
            players: vec![ArenaPlayerSnapshot {
                player_id: player_id(7),
                player_name: player_name("Alice"),
                team: TeamSide::TeamA,
                x: -640,
                y: 220,
                aim_x: 120,
                aim_y: 0,
                hit_points: 100,
                max_hit_points: 100,
                mana: 72,
                max_mana: 100,
                alive: true,
                unlocked_skill_slots: 3,
                primary_cooldown_remaining_ms: 250,
                primary_cooldown_total_ms: 650,
                slot_cooldown_remaining_ms: [100, 0, 900, 0, 0],
                slot_cooldown_total_ms: [700, 1700, 2200, 0, 0],
                active_statuses: vec![ArenaStatusSnapshot {
                    source: player_id(8),
                    slot: 2,
                    kind: ArenaStatusKind::Poison,
                    stacks: 2,
                    remaining_ms: 1800,
                }],
            }],
            projectiles: vec![],
        },
    }
}

fn sample_delta_arena_snapshot_event() -> ServerControlEvent {
    ServerControlEvent::ArenaDeltaSnapshot {
        snapshot: ArenaDeltaSnapshot {
            phase: ArenaMatchPhase::Combat,
            phase_seconds_remaining: None,
            players: vec![ArenaPlayerSnapshot {
                player_id: player_id(7),
                player_name: player_name("Alice"),
                team: TeamSide::TeamA,
                x: -620,
                y: 220,
                aim_x: 90,
                aim_y: -40,
                hit_points: 92,
                max_hit_points: 100,
                mana: 64,
                max_mana: 100,
                alive: true,
                unlocked_skill_slots: 3,
                primary_cooldown_remaining_ms: 180,
                primary_cooldown_total_ms: 650,
                slot_cooldown_remaining_ms: [0, 0, 800, 0, 0],
                slot_cooldown_total_ms: [700, 1700, 2200, 0, 0],
                active_statuses: vec![ArenaStatusSnapshot {
                    source: player_id(8),
                    slot: 2,
                    kind: ArenaStatusKind::Poison,
                    stacks: 3,
                    remaining_ms: 1400,
                }],
            }],
            projectiles: vec![],
        },
    }
}

fn sample_arena_effect_batch_event() -> ServerControlEvent {
    ServerControlEvent::ArenaEffectBatch {
        effects: vec![ArenaEffectSnapshot {
            kind: ArenaEffectKind::SkillShot,
            owner: player_id(7),
            slot: 1,
            x: -640,
            y: 220,
            target_x: 640,
            target_y: 220,
            radius: 28,
        }],
    }
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
fn server_control_event_rejects_invalid_arena_kinds() {
    let mut arena_payload = vec![19, 3, 0];
    arena_payload.extend_from_slice(&1800_u16.to_le_bytes());
    arena_payload.extend_from_slice(&1200_u16.to_le_bytes());
    arena_payload.extend_from_slice(&1_u16.to_le_bytes());
    arena_payload.push(9);
    arena_payload.extend_from_slice(&0_i16.to_le_bytes());
    arena_payload.extend_from_slice(&0_i16.to_le_bytes());
    arena_payload.extend_from_slice(&32_u16.to_le_bytes());
    arena_payload.extend_from_slice(&32_u16.to_le_bytes());
    arena_payload.extend_from_slice(&0_u16.to_le_bytes());
    let header = PacketHeader::new(
        ChannelId::Snapshot,
        PacketKind::FullSnapshot,
        0,
        u16::try_from(arena_payload.len()).expect("payload length should fit"),
        3,
        1,
    )
    .expect("header");
    let packet = header.encode(&arena_payload);
    assert_eq!(
        ServerControlEvent::decode_packet(&packet),
        Err(PacketError::InvalidEncodedArenaObstacleKind(9))
    );

    let mut effect_payload = vec![21];
    effect_payload.extend_from_slice(&1_u16.to_le_bytes());
    effect_payload.push(9);
    effect_payload.extend_from_slice(&7_u32.to_le_bytes());
    effect_payload.push(1);
    effect_payload.extend_from_slice(&0_i16.to_le_bytes());
    effect_payload.extend_from_slice(&0_i16.to_le_bytes());
    effect_payload.extend_from_slice(&0_i16.to_le_bytes());
    effect_payload.extend_from_slice(&0_i16.to_le_bytes());
    effect_payload.extend_from_slice(&28_u16.to_le_bytes());
    let header = PacketHeader::new(
        ChannelId::Snapshot,
        PacketKind::EventBatch,
        0,
        u16::try_from(effect_payload.len()).expect("payload length should fit"),
        4,
        1,
    )
    .expect("header");
    let packet = header.encode(&effect_payload);
    assert_eq!(
        ServerControlEvent::decode_packet(&packet),
        Err(PacketError::InvalidEncodedArenaEffectKind(9))
    );
}

#[test]
fn server_control_event_rejects_invalid_delta_snapshot_phase_and_status_values() {
    let mut bad_phase_payload = vec![20, 9, 0];
    bad_phase_payload.extend_from_slice(&0_u16.to_le_bytes());
    bad_phase_payload.extend_from_slice(&0_u16.to_le_bytes());
    let header = PacketHeader::new(
        ChannelId::Snapshot,
        PacketKind::DeltaSnapshot,
        0,
        u16::try_from(bad_phase_payload.len()).expect("payload length should fit"),
        5,
        1,
    )
    .expect("header");
    let packet = header.encode(&bad_phase_payload);
    assert_eq!(
        ServerControlEvent::decode_packet(&packet),
        Err(PacketError::InvalidEncodedArenaMatchPhase(9))
    );

    let mut bad_status_payload = vec![20, 3, 0];
    bad_status_payload.extend_from_slice(&1_u16.to_le_bytes());
    bad_status_payload.extend_from_slice(&7_u32.to_le_bytes());
    bad_status_payload.push(5);
    bad_status_payload.extend_from_slice(b"Alice");
    bad_status_payload.push(1);
    bad_status_payload.extend_from_slice(&0_i16.to_le_bytes());
    bad_status_payload.extend_from_slice(&0_i16.to_le_bytes());
    bad_status_payload.extend_from_slice(&120_i16.to_le_bytes());
    bad_status_payload.extend_from_slice(&0_i16.to_le_bytes());
    bad_status_payload.extend_from_slice(&100_u16.to_le_bytes());
    bad_status_payload.extend_from_slice(&100_u16.to_le_bytes());
    bad_status_payload.extend_from_slice(&80_u16.to_le_bytes());
    bad_status_payload.extend_from_slice(&100_u16.to_le_bytes());
    bad_status_payload.push(1);
    bad_status_payload.push(3);
    bad_status_payload.extend_from_slice(&0_u16.to_le_bytes());
    bad_status_payload.extend_from_slice(&600_u16.to_le_bytes());
    for _ in 0..5 {
        bad_status_payload.extend_from_slice(&0_u16.to_le_bytes());
    }
    for _ in 0..5 {
        bad_status_payload.extend_from_slice(&0_u16.to_le_bytes());
    }
    bad_status_payload.push(1);
    bad_status_payload.extend_from_slice(&8_u32.to_le_bytes());
    bad_status_payload.push(1);
    bad_status_payload.push(9);
    bad_status_payload.push(2);
    bad_status_payload.extend_from_slice(&1400_u16.to_le_bytes());
    bad_status_payload.extend_from_slice(&0_u16.to_le_bytes());
    let header = PacketHeader::new(
        ChannelId::Snapshot,
        PacketKind::DeltaSnapshot,
        0,
        u16::try_from(bad_status_payload.len()).expect("payload length should fit"),
        6,
        1,
    )
    .expect("header");
    let packet = header.encode(&bad_status_payload);
    assert_eq!(
        ServerControlEvent::decode_packet(&packet),
        Err(PacketError::InvalidEncodedArenaStatusKind(9))
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
