#![forbid(unsafe_code)]

use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};

use game_content::{parse_ascii_map, parse_skill_yaml};
use game_domain::{
    LobbyId, MatchId, MatchOutcome, PlayerId, PlayerName, ReadyState, RoundNumber, SkillTree,
    TeamSide,
};
use game_net::{
    ArenaEffectKind, ArenaEffectSnapshot, ArenaObstacleKind, ArenaObstacleSnapshot,
    ArenaPlayerSnapshot, ArenaStateSnapshot, ChannelId, ClientControlCommand, LobbyDirectoryEntry,
    LobbySnapshotPhase, LobbySnapshotPlayer, PacketHeader, PacketKind, ServerControlEvent,
    ValidatedInputFrame, BUTTON_CAST, BUTTON_PRIMARY,
};

const PROTOTYPE_ARENA_ASCII: &str = include_str!("../../../content/maps/prototype_arena.txt");
const MAGE_SKILLS_YAML: &str = include_str!("../../../content/skills/mage.yaml");

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
    write_http_route_classification_corpus(&corpus_root.join("http_route_classification"))?;
    write_observability_metrics_render_corpus(&corpus_root.join("observability_metrics_render"))?;
    write_player_record_store_parse_corpus(&corpus_root.join("player_record_store_parse"))?;
    write_skill_progression_corpus(&corpus_root.join("skill_progression"))?;
    write_ascii_map_parse_corpus(&corpus_root.join("ascii_map_parse"))?;
    write_skill_yaml_parse_corpus(&corpus_root.join("skill_yaml_parse"))?;

    println!("Seed corpora written under {}", corpus_root.display());
    Ok(())
}

fn write_packet_header_corpus(dir: &Path) -> Result<(), Box<dyn Error>> {
    recreate_dir(dir)?;

    let valid =
        PacketHeader::new(ChannelId::Control, PacketKind::ControlCommand, 0, 0, 1, 0)?.encode(&[]);
    let valid_input =
        PacketHeader::new(ChannelId::Input, PacketKind::InputFrame, 0, 16, 2, 3)?.encode(&[0; 16]);
    let mut bad_magic = valid.clone();
    bad_magic[0] = 0;
    let mut bad_version = valid.clone();
    bad_version[2] = 99;
    let mut bad_channel = valid.clone();
    bad_channel[3] = 99;
    let mut bad_kind = valid.clone();
    bad_kind[4] = 99;
    let mut bad_length = valid_input.clone();
    bad_length[6..8].copy_from_slice(&15_u16.to_le_bytes());

    write_seed(dir, "empty.bin", &[])?;
    write_seed(dir, "valid_control_header.bin", &valid)?;
    write_seed(dir, "valid_input_header.bin", &valid_input)?;
    write_seed(dir, "bad_magic.bin", &bad_magic)?;
    write_seed(dir, "bad_version.bin", &bad_version)?;
    write_seed(dir, "bad_channel.bin", &bad_channel)?;
    write_seed(dir, "bad_kind.bin", &bad_kind)?;
    write_seed(dir, "bad_length.bin", &bad_length)?;
    Ok(())
}

fn write_control_command_corpus(dir: &Path) -> Result<(), Box<dyn Error>> {
    recreate_dir(dir)?;

    let connect = ClientControlCommand::Connect {
        player_name: player_name("Alice")?,
    }
    .encode_packet(1, 0)?;
    let create = ClientControlCommand::CreateGameLobby.encode_packet(2, 0)?;
    let join = ClientControlCommand::JoinGameLobby {
        lobby_id: lobby_id(3)?,
    }
    .encode_packet(3, 0)?;
    let leave = ClientControlCommand::LeaveGameLobby.encode_packet(4, 0)?;
    let select_team = ClientControlCommand::SelectTeam {
        team: TeamSide::TeamA,
    }
    .encode_packet(5, 0)?;
    let set_ready = ClientControlCommand::SetReady {
        ready: ReadyState::Ready,
    }
    .encode_packet(6, 0)?;
    let choose_skill = ClientControlCommand::ChooseSkill {
        tree: SkillTree::Mage,
        tier: 3,
    }
    .encode_packet(7, 0)?;
    let quit = ClientControlCommand::QuitToCentralLobby.encode_packet(8, 0)?;
    let invalid_kind =
        PacketHeader::new(ChannelId::Control, PacketKind::ControlCommand, 0, 1, 4, 0)?
            .encode(&[255]);
    let truncated = connect[..connect.len() - 1].to_vec();
    let wrong_packet_kind =
        PacketHeader::new(ChannelId::Control, PacketKind::ControlEvent, 0, 1, 9, 0)?.encode(&[2]);
    let invalid_lobby_id =
        PacketHeader::new(ChannelId::Control, PacketKind::ControlCommand, 0, 5, 10, 0)?
            .encode(&[3, 0, 0, 0, 0]);
    let invalid_team =
        PacketHeader::new(ChannelId::Control, PacketKind::ControlCommand, 0, 2, 11, 0)?
            .encode(&[5, 9]);
    let invalid_ready =
        PacketHeader::new(ChannelId::Control, PacketKind::ControlCommand, 0, 2, 12, 0)?
            .encode(&[6, 9]);
    let invalid_skill_tree =
        PacketHeader::new(ChannelId::Control, PacketKind::ControlCommand, 0, 3, 13, 0)?
            .encode(&[7, 9, 1]);
    let trailing_bytes =
        PacketHeader::new(ChannelId::Control, PacketKind::ControlCommand, 0, 2, 14, 0)?
            .encode(&[4, 99]);
    let bad_name = {
        let long_name = "A".repeat(game_domain::MAX_PLAYER_NAME_LEN + 1);
        let mut payload = vec![1];
        payload.push(u8::try_from(long_name.len())?);
        payload.extend_from_slice(long_name.as_bytes());
        PacketHeader::new(
            ChannelId::Control,
            PacketKind::ControlCommand,
            0,
            u16::try_from(payload.len())?,
            15,
            0,
        )?
        .encode(&payload)
    };

    write_seed(dir, "connect_valid.bin", &connect)?;
    write_seed(dir, "create_valid.bin", &create)?;
    write_seed(dir, "join_valid.bin", &join)?;
    write_seed(dir, "leave_valid.bin", &leave)?;
    write_seed(dir, "select_team_valid.bin", &select_team)?;
    write_seed(dir, "set_ready_valid.bin", &set_ready)?;
    write_seed(dir, "choose_skill_valid.bin", &choose_skill)?;
    write_seed(dir, "quit_valid.bin", &quit)?;
    write_seed(dir, "invalid_kind.bin", &invalid_kind)?;
    write_seed(dir, "truncated_connect.bin", &truncated)?;
    write_seed(dir, "wrong_packet_kind.bin", &wrong_packet_kind)?;
    write_seed(dir, "invalid_lobby_id.bin", &invalid_lobby_id)?;
    write_seed(dir, "invalid_team.bin", &invalid_team)?;
    write_seed(dir, "invalid_ready.bin", &invalid_ready)?;
    write_seed(dir, "invalid_skill_tree.bin", &invalid_skill_tree)?;
    write_seed(dir, "trailing_bytes.bin", &trailing_bytes)?;
    write_seed(dir, "bad_name.bin", &bad_name)?;
    Ok(())
}

fn write_input_frame_corpus(dir: &Path) -> Result<(), Box<dyn Error>> {
    recreate_dir(dir)?;

    let cast =
        ValidatedInputFrame::new(3, 1, -1, 50, -50, BUTTON_CAST, 9)?.encode_packet(17, 99)?;
    let movement =
        ValidatedInputFrame::new(4, 25, -25, 0, 0, BUTTON_PRIMARY, 0)?.encode_packet(18, 100)?;
    let primary_attack =
        ValidatedInputFrame::new(5, 0, 0, 0, 0, BUTTON_PRIMARY, 0)?.encode_packet(19, 101)?;
    let truncated = cast[..cast.len() - 1].to_vec();

    let mut bad_buttons_payload = [0_u8; 16];
    bad_buttons_payload[12..14].copy_from_slice(&0x8000_u16.to_le_bytes());
    let bad_buttons = PacketHeader::new(ChannelId::Input, PacketKind::InputFrame, 0, 16, 20, 102)?
        .encode(&bad_buttons_payload);
    let [cast_button_low, cast_button_high] = BUTTON_CAST.to_le_bytes();
    let missing_context =
        PacketHeader::new(ChannelId::Input, PacketKind::InputFrame, 0, 16, 21, 102)?.encode(&[
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            cast_button_low,
            cast_button_high,
            0,
            0,
        ]);
    let unexpected_context =
        PacketHeader::new(ChannelId::Input, PacketKind::InputFrame, 0, 16, 22, 102)?
            .encode(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 7, 0]);
    let wrong_packet_kind = PacketHeader::new(
        ChannelId::Control,
        PacketKind::ControlCommand,
        0,
        16,
        23,
        102,
    )?
    .encode(&[0; 16]);
    let bad_length = PacketHeader::new(ChannelId::Input, PacketKind::InputFrame, 0, 15, 24, 102)?
        .encode(&[0; 15]);

    write_seed(dir, "cast_valid.bin", &cast)?;
    write_seed(dir, "movement_valid.bin", &movement)?;
    write_seed(dir, "primary_attack_valid.bin", &primary_attack)?;
    write_seed(dir, "truncated_cast.bin", &truncated)?;
    write_seed(dir, "invalid_buttons.bin", &bad_buttons)?;
    write_seed(dir, "missing_context.bin", &missing_context)?;
    write_seed(dir, "unexpected_context.bin", &unexpected_context)?;
    write_seed(dir, "wrong_packet_kind.bin", &wrong_packet_kind)?;
    write_seed(dir, "bad_length.bin", &bad_length)?;
    Ok(())
}

fn write_session_ingress_corpus(dir: &Path) -> Result<(), Box<dyn Error>> {
    recreate_dir(dir)?;

    let connect = ClientControlCommand::Connect {
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
        player_name: player_name("Eve")?,
    }
    .encode_packet(4, 0)?;
    let choose_skill = ClientControlCommand::ChooseSkill {
        tree: SkillTree::Mage,
        tier: 1,
    }
    .encode_packet(5, 0)?;
    let stale_ready = ClientControlCommand::SetReady {
        ready: ReadyState::Ready,
    }
    .encode_packet(1, 0)?;

    let valid_stream = prefix_packets(&[connect.clone(), select_team, set_ready]);
    let invalid_first = prefix_packets(&[choose_skill]);
    let rebinding = prefix_packets(&[connect, reconnect]);
    let stale_sequence = prefix_packets(&[connect_valid_ingress_bind()?, stale_ready]);

    write_seed(dir, "valid_bind_then_ready.bin", &valid_stream)?;
    write_seed(dir, "invalid_first_packet.bin", &invalid_first)?;
    write_seed(dir, "rebinding_attempt.bin", &rebinding)?;
    write_seed(dir, "stale_sequence.bin", &stale_sequence)?;
    Ok(())
}

#[allow(clippy::too_many_lines)]
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
    let created = ServerControlEvent::GameLobbyCreated {
        lobby_id: lobby_id(3)?,
    }
    .encode_packet(2, 10)?;
    let joined = ServerControlEvent::GameLobbyJoined {
        lobby_id: lobby_id(3)?,
        player_id: player_id(8)?,
    }
    .encode_packet(3, 10)?;
    let left = ServerControlEvent::GameLobbyLeft {
        lobby_id: lobby_id(3)?,
        player_id: player_id(8)?,
    }
    .encode_packet(4, 10)?;
    let team_selected = ServerControlEvent::TeamSelected {
        player_id: player_id(8)?,
        team: TeamSide::TeamB,
        ready_reset: true,
    }
    .encode_packet(5, 10)?;
    let ready_changed = ServerControlEvent::ReadyChanged {
        player_id: player_id(8)?,
        ready: ReadyState::Ready,
    }
    .encode_packet(6, 10)?;
    let countdown_started = ServerControlEvent::LaunchCountdownStarted {
        lobby_id: lobby_id(3)?,
        seconds_remaining: 5,
        roster_size: 2,
    }
    .encode_packet(7, 10)?;
    let countdown_tick = ServerControlEvent::LaunchCountdownTick {
        lobby_id: lobby_id(3)?,
        seconds_remaining: 4,
    }
    .encode_packet(8, 10)?;
    let match_started = ServerControlEvent::MatchStarted {
        match_id: match_id(9)?,
        round: round_number(1)?,
        skill_pick_seconds: 25,
    }
    .encode_packet(9, 11)?;
    let skill_chosen = ServerControlEvent::SkillChosen {
        player_id: player_id(8)?,
        tree: SkillTree::Rogue,
        tier: 3,
    }
    .encode_packet(10, 11)?;
    let precombat_started = ServerControlEvent::PreCombatStarted {
        seconds_remaining: 5,
    }
    .encode_packet(11, 11)?;
    let combat_started = ServerControlEvent::CombatStarted.encode_packet(12, 11)?;
    let round_won = ServerControlEvent::RoundWon {
        round: round_number(1)?,
        winning_team: TeamSide::TeamA,
        score_a: 1,
        score_b: 0,
    }
    .encode_packet(13, 11)?;
    let match_ended = ServerControlEvent::MatchEnded {
        outcome: MatchOutcome::NoContest,
        score_a: 1,
        score_b: 0,
        message: String::from("Bob has disconnected. Game is over."),
    }
    .encode_packet(14, 12)?;
    let returned = ServerControlEvent::ReturnedToCentralLobby {
        record: game_domain::PlayerRecord {
            wins: 1,
            losses: 0,
            no_contests: 1,
        },
    }
    .encode_packet(15, 12)?;
    let error = ServerControlEvent::Error {
        message: String::from("bad packet"),
    }
    .encode_packet(16, 12)?;
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
    let arena_state = ServerControlEvent::ArenaStateSnapshot {
        snapshot: ArenaStateSnapshot {
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
                ArenaObstacleSnapshot {
                    kind: ArenaObstacleKind::Shrub,
                    center_x: 220,
                    center_y: 150,
                    half_width: 92,
                    half_height: 92,
                },
                ArenaObstacleSnapshot {
                    kind: ArenaObstacleKind::Pillar,
                    center_x: 220,
                    center_y: 150,
                    half_width: 70,
                    half_height: 70,
                },
            ],
            players: vec![
                ArenaPlayerSnapshot {
                    player_id: player_id(7)?,
                    player_name: player_name("Alice")?,
                    team: TeamSide::TeamA,
                    x: -640,
                    y: 220,
                    aim_x: 120,
                    aim_y: 0,
                    hit_points: 100,
                    max_hit_points: 100,
                    alive: true,
                    unlocked_skill_slots: 3,
                    primary_cooldown_remaining_ms: 250,
                    primary_cooldown_total_ms: 650,
                    slot_cooldown_remaining_ms: [100, 0, 900, 0, 0],
                    slot_cooldown_total_ms: [700, 1700, 2200, 0, 0],
                },
                ArenaPlayerSnapshot {
                    player_id: player_id(8)?,
                    player_name: player_name("Bob")?,
                    team: TeamSide::TeamB,
                    x: 640,
                    y: 220,
                    aim_x: -120,
                    aim_y: 0,
                    hit_points: 100,
                    max_hit_points: 100,
                    alive: true,
                    unlocked_skill_slots: 3,
                    primary_cooldown_remaining_ms: 0,
                    primary_cooldown_total_ms: 450,
                    slot_cooldown_remaining_ms: [0, 0, 0, 0, 0],
                    slot_cooldown_total_ms: [650, 1500, 1900, 0, 0],
                },
            ],
            projectiles: vec![],
        },
    }
    .encode_packet(4, 12)?;
    let arena_effects = ServerControlEvent::ArenaEffectBatch {
        effects: vec![
            ArenaEffectSnapshot {
                kind: ArenaEffectKind::SkillShot,
                owner: player_id(7)?,
                slot: 1,
                x: -640,
                y: 220,
                target_x: 640,
                target_y: 220,
                radius: 28,
            },
            ArenaEffectSnapshot {
                kind: ArenaEffectKind::MeleeSwing,
                owner: player_id(8)?,
                slot: 0,
                x: 640,
                y: 220,
                target_x: 530,
                target_y: 220,
                radius: 48,
            },
        ],
    }
    .encode_packet(5, 12)?;
    let truncated = snapshot[..snapshot.len() - 1].to_vec();
    let wrong_packet_kind =
        PacketHeader::new(ChannelId::Control, PacketKind::ControlCommand, 0, 1, 4, 12)?
            .encode(&[16]);

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
    let invalid_lobby_phase = {
        let mut payload = vec![17];
        payload.extend_from_slice(&1_u16.to_le_bytes());
        payload.extend_from_slice(&1_u32.to_le_bytes());
        payload.extend_from_slice(&0_u16.to_le_bytes());
        payload.extend_from_slice(&0_u16.to_le_bytes());
        payload.extend_from_slice(&0_u16.to_le_bytes());
        payload.extend_from_slice(&0_u16.to_le_bytes());
        payload.push(9);
        PacketHeader::new(
            ChannelId::Control,
            PacketKind::ControlEvent,
            0,
            u16::try_from(payload.len())?,
            5,
            12,
        )?
        .encode(&payload)
    };
    let invalid_bool =
        PacketHeader::new(ChannelId::Control, PacketKind::ControlEvent, 0, 7, 6, 12)?
            .encode(&[5, 8, 0, 0, 0, 2, 9]);
    let invalid_ready =
        PacketHeader::new(ChannelId::Control, PacketKind::ControlEvent, 0, 6, 7, 12)?
            .encode(&[6, 8, 0, 0, 0, 9]);
    let invalid_team =
        PacketHeader::new(ChannelId::Control, PacketKind::ControlEvent, 0, 5, 8, 12)?
            .encode(&[13, 1, 9, 1, 0]);
    let invalid_match_outcome =
        PacketHeader::new(ChannelId::Control, PacketKind::ControlEvent, 0, 4, 9, 12)?
            .encode(&[14, 9, 0, 0]);
    let invalid_arena_obstacle_kind = {
        let mut payload = vec![19];
        payload.extend_from_slice(&1800_u16.to_le_bytes());
        payload.extend_from_slice(&1200_u16.to_le_bytes());
        payload.extend_from_slice(&1_u16.to_le_bytes());
        payload.push(9);
        payload.extend_from_slice(&0_i16.to_le_bytes());
        payload.extend_from_slice(&0_i16.to_le_bytes());
        payload.extend_from_slice(&32_u16.to_le_bytes());
        payload.extend_from_slice(&32_u16.to_le_bytes());
        payload.extend_from_slice(&0_u16.to_le_bytes());
        PacketHeader::new(
            ChannelId::Control,
            PacketKind::ControlEvent,
            0,
            u16::try_from(payload.len())?,
            10,
            12,
        )?
        .encode(&payload)
    };
    let invalid_arena_effect_kind = {
        let mut payload = vec![20];
        payload.extend_from_slice(&1_u16.to_le_bytes());
        payload.push(9);
        payload.extend_from_slice(&7_u32.to_le_bytes());
        payload.push(1);
        payload.extend_from_slice(&0_i16.to_le_bytes());
        payload.extend_from_slice(&0_i16.to_le_bytes());
        payload.extend_from_slice(&0_i16.to_le_bytes());
        payload.extend_from_slice(&0_i16.to_le_bytes());
        payload.extend_from_slice(&28_u16.to_le_bytes());
        PacketHeader::new(
            ChannelId::Control,
            PacketKind::ControlEvent,
            0,
            u16::try_from(payload.len())?,
            11,
            12,
        )?
        .encode(&payload)
    };

    write_seed(dir, "connected_valid.bin", &connected)?;
    write_seed(dir, "created_valid.bin", &created)?;
    write_seed(dir, "joined_valid.bin", &joined)?;
    write_seed(dir, "left_valid.bin", &left)?;
    write_seed(dir, "team_selected_valid.bin", &team_selected)?;
    write_seed(dir, "ready_changed_valid.bin", &ready_changed)?;
    write_seed(dir, "countdown_started_valid.bin", &countdown_started)?;
    write_seed(dir, "countdown_tick_valid.bin", &countdown_tick)?;
    write_seed(dir, "match_started_valid.bin", &match_started)?;
    write_seed(dir, "skill_chosen_valid.bin", &skill_chosen)?;
    write_seed(dir, "precombat_valid.bin", &precombat_started)?;
    write_seed(dir, "combat_started_valid.bin", &combat_started)?;
    write_seed(dir, "round_won_valid.bin", &round_won)?;
    write_seed(dir, "match_ended_valid.bin", &match_ended)?;
    write_seed(dir, "returned_valid.bin", &returned)?;
    write_seed(dir, "error_valid.bin", &error)?;
    write_seed(dir, "directory_valid.bin", &directory)?;
    write_seed(dir, "snapshot_valid.bin", &snapshot)?;
    write_seed(dir, "arena_state_valid.bin", &arena_state)?;
    write_seed(dir, "arena_effects_valid.bin", &arena_effects)?;
    write_seed(dir, "truncated_snapshot.bin", &truncated)?;
    write_seed(dir, "wrong_packet_kind.bin", &wrong_packet_kind)?;
    write_seed(dir, "invalid_optional_team.bin", &invalid_optional_team)?;
    write_seed(dir, "invalid_lobby_phase.bin", &invalid_lobby_phase)?;
    write_seed(dir, "invalid_bool.bin", &invalid_bool)?;
    write_seed(dir, "invalid_ready.bin", &invalid_ready)?;
    write_seed(dir, "invalid_team.bin", &invalid_team)?;
    write_seed(dir, "invalid_match_outcome.bin", &invalid_match_outcome)?;
    write_seed(
        dir,
        "invalid_arena_obstacle_kind.bin",
        &invalid_arena_obstacle_kind,
    )?;
    write_seed(
        dir,
        "invalid_arena_effect_kind.bin",
        &invalid_arena_effect_kind,
    )?;
    Ok(())
}

fn write_http_route_classification_corpus(dir: &Path) -> Result<(), Box<dyn Error>> {
    recreate_dir(dir)?;

    write_seed(dir, "empty.bin", &[])?;
    write_seed(dir, "root.bin", b"/")?;
    write_seed(dir, "healthz.bin", b"/healthz")?;
    write_seed(dir, "metrics.bin", b"/metrics")?;
    write_seed(dir, "websocket.bin", b"/ws")?;
    write_seed(dir, "index_js.bin", b"/index.js")?;
    write_seed(dir, "nested_asset.bin", b"/assets/shell/index.wasm")?;
    write_seed(dir, "healthz_suffix.bin", b"/healthz/extra")?;
    write_seed(dir, "invalid_utf8.bin", &[0x2F, 0xFF, 0x00, 0x41])?;
    Ok(())
}

fn write_observability_metrics_render_corpus(dir: &Path) -> Result<(), Box<dyn Error>> {
    recreate_dir(dir)?;

    write_seed(dir, "empty.bin", &[])?;
    write_seed(
        dir,
        "version_only.bin",
        &observability_metrics_seed(b"0.6.0", &[]),
    )?;
    write_seed(
        dir,
        "escaped_version_and_all_ops.bin",
        &observability_metrics_seed(
            b"0.6.0-\"beta\"\\canary\nbuild",
            &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
        ),
    )?;
    write_seed(
        dir,
        "disconnect_heavy.bin",
        &observability_metrics_seed(b"disconnect-heavy", &[7, 7, 7, 6, 7, 8, 9, 10]),
    )?;
    write_seed(
        dir,
        "tick_heavy.bin",
        &observability_metrics_seed(b"ticks", &[11, 12, 12, 11, 4, 2, 0]),
    )?;
    Ok(())
}

fn write_player_record_store_parse_corpus(dir: &Path) -> Result<(), Box<dyn Error>> {
    recreate_dir(dir)?;

    write_seed(dir, "empty.bin", &[])?;
    write_seed(dir, "single_valid_row.tsv", b"Alice\t0\t0\t0\n")?;
    write_seed(
        dir,
        "unsorted_valid_rows.tsv",
        b"Bob\t1\t2\t3\nAlice\t0\t0\t0\n",
    )?;
    write_seed(
        dir,
        "duplicate_name.tsv",
        b"Alice\t0\t0\t0\nAlice\t1\t1\t1\n",
    )?;
    write_seed(
        dir,
        "legacy_valid_rows.tsv",
        b"2\tBob\t1\t2\t3\n1\tAlice\t0\t0\t0\n",
    )?;
    write_seed(
        dir,
        "legacy_duplicate_id.tsv",
        b"1\tAlice\t0\t0\t0\n1\tBob\t1\t1\t1\n",
    )?;
    write_seed(dir, "bad_field_count.tsv", b"Alice\t0\t0\n")?;
    write_seed(dir, "bad_player_id.tsv", b"0\tAlice\t0\t0\t0\n")?;
    write_seed(dir, "bad_counter.tsv", b"Alice\t999999\t0\t0\n")?;
    write_seed(dir, "bad_name.tsv", b"bad name with spaces\t0\t0\t0\n")?;
    Ok(())
}

fn write_skill_progression_corpus(dir: &Path) -> Result<(), Box<dyn Error>> {
    recreate_dir(dir)?;

    write_seed(dir, "valid_opening.bin", &[2, 1])?;
    write_seed(dir, "invalid_new_player_skip.bin", &[2, 5])?;
    write_seed(dir, "valid_then_gap.bin", &[2, 1, 2, 3])?;
    write_seed(dir, "valid_switch_tree.bin", &[2, 1, 0, 1, 2, 2])?;
    write_seed(dir, "repeated_same_tier.bin", &[1, 1, 1, 1])?;
    write_seed(dir, "out_of_range_tier.bin", &[3, 0, 3, 6])?;
    Ok(())
}

fn write_ascii_map_parse_corpus(dir: &Path) -> Result<(), Box<dyn Error>> {
    recreate_dir(dir)?;

    let missing_team_b = "....\n.A..\n.##.\n....\n";
    let ragged = "A..\n..B\n.\n";
    let invalid_glyph = "A..\n.%.\n..B\n";
    let duplicate_anchor = "A..B\n.A..\n....\n";

    let _ = parse_ascii_map("maps/prototype_arena.txt", PROTOTYPE_ARENA_ASCII)?;

    write_seed(dir, "prototype_arena.txt", PROTOTYPE_ARENA_ASCII.as_bytes())?;
    write_seed(dir, "missing_team_b.txt", missing_team_b.as_bytes())?;
    write_seed(dir, "ragged.txt", ragged.as_bytes())?;
    write_seed(dir, "invalid_glyph.txt", invalid_glyph.as_bytes())?;
    write_seed(dir, "duplicate_anchor.txt", duplicate_anchor.as_bytes())?;
    write_seed(dir, "empty.bin", &[])?;
    Ok(())
}

fn write_skill_yaml_parse_corpus(dir: &Path) -> Result<(), Box<dyn Error>> {
    recreate_dir(dir)?;

    let unknown_tree = r"
tree: Druid
skills:
  - tier: 1
    id: druid_sprout
    name: Sprout
    description: nope
    behavior:
      kind: line
      effect: skill_shot
      range: 10
      damage: 1
";
    let duplicate_tier = r"
tree: Mage
skills:
  - tier: 1
    id: mage_a
    name: A
    description: A
    behavior:
      kind: line
      effect: skill_shot
      range: 10
      damage: 1
  - tier: 1
    id: mage_b
    name: B
    description: B
    behavior:
      kind: line
      effect: beam
      range: 20
      damage: 2
";
    let missing_damage = r"
tree: Mage
skills:
  - tier: 1
    id: mage_arc
    name: Arc
    description: Missing damage.
    behavior:
      kind: line
      effect: skill_shot
      range: 10
";
    let invalid_behavior = r"
tree: Mage
skills:
  - tier: 1
    id: mage_arc
    name: Arc
    description: Unknown behavior.
    behavior:
      kind: wall
      effect: skill_shot
      range: 10
      damage: 1
";

    let _ = parse_skill_yaml("skills/mage.yaml", MAGE_SKILLS_YAML)?;

    write_seed(dir, "mage.yaml", MAGE_SKILLS_YAML.as_bytes())?;
    write_seed(dir, "unknown_tree.yaml", unknown_tree.as_bytes())?;
    write_seed(dir, "duplicate_tier.yaml", duplicate_tier.as_bytes())?;
    write_seed(dir, "missing_damage.yaml", missing_damage.as_bytes())?;
    write_seed(dir, "invalid_behavior.yaml", invalid_behavior.as_bytes())?;
    write_seed(dir, "empty.bin", &[])?;
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

fn observability_metrics_seed(version: &[u8], operations: &[u8]) -> Vec<u8> {
    let capped_version_len = version.len().min(32);
    let Ok(version_len) = u8::try_from(capped_version_len) else {
        panic!("observability metrics seed version length must fit within u8");
    };

    let mut bytes = Vec::with_capacity(1 + capped_version_len + operations.len());
    bytes.push(version_len);
    bytes.extend_from_slice(&version[..capped_version_len]);
    bytes.extend_from_slice(operations);
    bytes
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

fn match_id(raw: u32) -> Result<MatchId, Box<dyn Error>> {
    Ok(MatchId::new(raw)?)
}

fn player_name(raw: &str) -> Result<PlayerName, Box<dyn Error>> {
    Ok(PlayerName::new(raw)?)
}

fn round_number(raw: u8) -> Result<RoundNumber, Box<dyn Error>> {
    Ok(RoundNumber::new(raw)?)
}

fn connect_valid_ingress_bind() -> Result<Vec<u8>, Box<dyn Error>> {
    ClientControlCommand::Connect {
        player_name: player_name("Mallory")?,
    }
    .encode_packet(1, 0)
    .map_err(Into::into)
}
