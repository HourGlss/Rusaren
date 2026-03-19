use game_domain::{
    LobbyId, MatchId, MatchOutcome, PlayerId, PlayerName, PlayerRecord, ReadyState, RoundNumber,
    SkillTree, TeamSide,
};

use crate::{ChannelId, PacketError, PacketHeader, PacketKind};

use super::codec::*;
use super::server_types::*;
use super::snapshots::*;
use super::{MAX_MESSAGE_BYTES, MAX_SKILL_TREE_NAME_BYTES};

impl ServerControlEvent {
    pub fn encode_packet(self, seq: u32, sim_tick: u32) -> Result<Vec<u8>, PacketError> {
        let (channel_id, packet_kind) = self.transport_metadata();
        let mut payload = Vec::new();
        payload.push(self.kind_byte());
        self.encode_body(&mut payload)?;

        let payload_len =
            u16::try_from(payload.len()).map_err(|_| PacketError::PayloadTooLarge {
                actual: payload.len(),
                maximum: usize::from(u16::MAX),
            })?;
        let header = PacketHeader::new(channel_id, packet_kind, 0, payload_len, seq, sim_tick)?;

        Ok(header.encode(&payload))
    }

    pub fn decode_packet(packet: &[u8]) -> Result<(PacketHeader, Self), PacketError> {
        let (header, payload) = PacketHeader::decode(packet)?;
        match (header.channel_id, header.packet_kind) {
            (ChannelId::Control, PacketKind::ControlEvent)
            | (
                ChannelId::Snapshot,
                PacketKind::FullSnapshot | PacketKind::DeltaSnapshot | PacketKind::EventBatch,
            ) => {}
            _ => {
                return Err(PacketError::UnexpectedPacketKind {
                    expected_channel: ChannelId::Control,
                    expected_kind: PacketKind::ControlEvent,
                    actual_channel: header.channel_id,
                    actual_kind: header.packet_kind,
                });
            }
        }

        let kind = *payload.first().ok_or(PacketError::ControlPayloadTooShort {
            kind: "ServerControlEvent",
            expected: 1,
            actual: payload.len(),
        })?;
        let mut index = 1usize;
        let event = Self::decode_body(kind, payload, &mut index)?;

        ensure_consumed(payload, index, "ServerControlEvent")?;
        Ok((header, event))
    }

    const fn transport_metadata(&self) -> (ChannelId, PacketKind) {
        match self {
            Self::ArenaStateSnapshot { .. } => (ChannelId::Snapshot, PacketKind::FullSnapshot),
            Self::ArenaDeltaSnapshot { .. } => (ChannelId::Snapshot, PacketKind::DeltaSnapshot),
            Self::ArenaEffectBatch { .. } => (ChannelId::Snapshot, PacketKind::EventBatch),
            _ => (ChannelId::Control, PacketKind::ControlEvent),
        }
    }

    const fn kind_byte(&self) -> u8 {
        match self {
            Self::Connected { .. } => 1,
            Self::GameLobbyCreated { .. } => 2,
            Self::GameLobbyJoined { .. } => 3,
            Self::GameLobbyLeft { .. } => 4,
            Self::TeamSelected { .. } => 5,
            Self::ReadyChanged { .. } => 6,
            Self::LaunchCountdownStarted { .. } => 7,
            Self::LaunchCountdownTick { .. } => 8,
            Self::MatchStarted { .. } => 9,
            Self::SkillChosen { .. } => 10,
            Self::PreCombatStarted { .. } => 11,
            Self::CombatStarted => 12,
            Self::RoundWon { .. } => 13,
            Self::MatchEnded { .. } => 14,
            Self::ReturnedToCentralLobby { .. } => 15,
            Self::Error { .. } => 16,
            Self::LobbyDirectorySnapshot { .. } => 17,
            Self::GameLobbySnapshot { .. } => 18,
            Self::ArenaStateSnapshot { .. } => 19,
            Self::ArenaDeltaSnapshot { .. } => 20,
            Self::ArenaEffectBatch { .. } => 21,
        }
    }

    #[allow(clippy::too_many_lines)]
    fn encode_body(self, payload: &mut Vec<u8>) -> Result<(), PacketError> {
        match self {
            Self::Connected {
                player_id,
                player_name,
                record,
                skill_catalog,
            } => encode_connected_event(payload, player_id, &player_name, record, &skill_catalog),
            Self::GameLobbyCreated { lobby_id } => {
                encode_lobby_id_event(payload, lobby_id);
                Ok(())
            }
            Self::GameLobbyJoined {
                lobby_id,
                player_id,
            }
            | Self::GameLobbyLeft {
                lobby_id,
                player_id,
            } => {
                encode_lobby_and_player_event(payload, lobby_id, player_id);
                Ok(())
            }
            Self::TeamSelected {
                player_id,
                team,
                ready_reset,
            } => {
                encode_team_selected_event(payload, player_id, team, ready_reset);
                Ok(())
            }
            Self::ReadyChanged { player_id, ready } => {
                encode_ready_changed_event(payload, player_id, ready);
                Ok(())
            }
            Self::LaunchCountdownStarted {
                lobby_id,
                seconds_remaining,
                roster_size,
            } => {
                encode_countdown_started_event(payload, lobby_id, seconds_remaining, roster_size);
                Ok(())
            }
            Self::LaunchCountdownTick {
                lobby_id,
                seconds_remaining,
            } => {
                encode_countdown_tick_event(payload, lobby_id, seconds_remaining);
                Ok(())
            }
            Self::MatchStarted {
                match_id,
                round,
                skill_pick_seconds,
            } => {
                encode_match_started_event(payload, match_id, round, skill_pick_seconds);
                Ok(())
            }
            Self::SkillChosen {
                player_id,
                tree,
                tier,
            } => encode_skill_chosen_event(payload, player_id, &tree, tier),
            Self::PreCombatStarted { seconds_remaining } => {
                payload.push(seconds_remaining);
                Ok(())
            }
            Self::CombatStarted => Ok(()),
            Self::RoundWon {
                round,
                winning_team,
                score_a,
                score_b,
            } => {
                encode_round_won_event(payload, round, winning_team, score_a, score_b);
                Ok(())
            }
            Self::MatchEnded {
                outcome,
                score_a,
                score_b,
                message,
            } => encode_match_ended_event(payload, outcome, score_a, score_b, &message),
            Self::ReturnedToCentralLobby { record } => {
                encode_player_record(payload, record);
                Ok(())
            }
            Self::LobbyDirectorySnapshot { lobbies } => {
                encode_lobby_directory_snapshot(payload, &lobbies)
            }
            Self::GameLobbySnapshot {
                lobby_id,
                phase,
                players,
            } => encode_game_lobby_snapshot(payload, lobby_id, phase, &players),
            Self::ArenaStateSnapshot { snapshot } => {
                encode_arena_state_snapshot(payload, &snapshot)
            }
            Self::ArenaDeltaSnapshot { snapshot } => {
                encode_arena_delta_snapshot(payload, &snapshot)
            }
            Self::ArenaEffectBatch { effects } => encode_arena_effect_batch(payload, &effects),
            Self::Error { message } => {
                push_len_prefixed_string(payload, "message", &message, MAX_MESSAGE_BYTES)
            }
        }
    }

    fn decode_body(kind: u8, payload: &[u8], index: &mut usize) -> Result<Self, PacketError> {
        match kind {
            1 => decode_connected_event(payload, index),
            2 => Ok(Self::GameLobbyCreated {
                lobby_id: read_lobby_id(payload, index, "GameLobbyCreated")?,
            }),
            3 => decode_lobby_and_player_event(
                payload,
                index,
                "GameLobbyJoined",
                |lobby_id, player_id| Self::GameLobbyJoined {
                    lobby_id,
                    player_id,
                },
            ),
            4 => decode_lobby_and_player_event(
                payload,
                index,
                "GameLobbyLeft",
                |lobby_id, player_id| Self::GameLobbyLeft {
                    lobby_id,
                    player_id,
                },
            ),
            5 => decode_team_selected_event(payload, index),
            6 => Ok(Self::ReadyChanged {
                player_id: read_player_id(payload, index, "ReadyChanged")?,
                ready: read_ready_state(payload, index, "ReadyChanged")?,
            }),
            7 => decode_countdown_started_event(payload, index),
            8 => decode_countdown_tick_event(payload, index),
            9 => decode_match_started_event(payload, index),
            10 => decode_skill_chosen_event(payload, index),
            11 => Ok(Self::PreCombatStarted {
                seconds_remaining: read_u8(payload, index, "PreCombatStarted")?,
            }),
            12 => Ok(Self::CombatStarted),
            13 => decode_round_won_event(payload, index),
            14 => decode_match_ended_event(payload, index),
            15 => Ok(Self::ReturnedToCentralLobby {
                record: read_player_record(payload, index, "ReturnedToCentralLobby")?,
            }),
            16 => Ok(Self::Error {
                message: read_string(payload, index, "Error", "message", MAX_MESSAGE_BYTES)?,
            }),
            17 => decode_lobby_directory_snapshot(payload, index),
            18 => decode_game_lobby_snapshot(payload, index),
            19 => decode_arena_state_snapshot(payload, index),
            20 => decode_arena_delta_snapshot(payload, index),
            21 => decode_arena_effect_batch(payload, index),
            other => Err(PacketError::UnknownServerEvent(other)),
        }
    }
}

fn encode_connected_event(
    payload: &mut Vec<u8>,
    player_id: PlayerId,
    player_name: &PlayerName,
    record: PlayerRecord,
    skill_catalog: &[SkillCatalogEntry],
) -> Result<(), PacketError> {
    payload.extend_from_slice(&player_id.get().to_le_bytes());
    push_len_prefixed_string(
        payload,
        "player_name",
        player_name.as_str(),
        game_domain::MAX_PLAYER_NAME_LEN,
    )?;
    encode_player_record(payload, record);
    encode_skill_catalog(payload, skill_catalog)?;
    Ok(())
}

fn decode_connected_event(
    payload: &[u8],
    index: &mut usize,
) -> Result<ServerControlEvent, PacketError> {
    Ok(ServerControlEvent::Connected {
        player_id: read_player_id(payload, index, "Connected")?,
        player_name: read_player_name(payload, index, "Connected")?,
        record: read_player_record(payload, index, "Connected")?,
        skill_catalog: decode_skill_catalog(payload, index, "Connected")?,
    })
}

fn encode_lobby_id_event(payload: &mut Vec<u8>, lobby_id: LobbyId) {
    payload.extend_from_slice(&lobby_id.get().to_le_bytes());
}

fn encode_lobby_and_player_event(payload: &mut Vec<u8>, lobby_id: LobbyId, player_id: PlayerId) {
    payload.extend_from_slice(&lobby_id.get().to_le_bytes());
    payload.extend_from_slice(&player_id.get().to_le_bytes());
}

fn decode_lobby_and_player_event<F>(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
    constructor: F,
) -> Result<ServerControlEvent, PacketError>
where
    F: FnOnce(LobbyId, PlayerId) -> ServerControlEvent,
{
    Ok(constructor(
        read_lobby_id(payload, index, kind)?,
        read_player_id(payload, index, kind)?,
    ))
}

fn encode_team_selected_event(
    payload: &mut Vec<u8>,
    player_id: PlayerId,
    team: TeamSide,
    ready_reset: bool,
) {
    payload.extend_from_slice(&player_id.get().to_le_bytes());
    payload.push(encode_team(team));
    payload.push(u8::from(ready_reset));
}

fn decode_team_selected_event(
    payload: &[u8],
    index: &mut usize,
) -> Result<ServerControlEvent, PacketError> {
    Ok(ServerControlEvent::TeamSelected {
        player_id: read_player_id(payload, index, "TeamSelected")?,
        team: read_team(payload, index, "TeamSelected")?,
        ready_reset: read_bool(payload, index, "TeamSelected")?,
    })
}

fn encode_ready_changed_event(payload: &mut Vec<u8>, player_id: PlayerId, ready: ReadyState) {
    payload.extend_from_slice(&player_id.get().to_le_bytes());
    payload.push(encode_ready_state(ready));
}

fn encode_countdown_started_event(
    payload: &mut Vec<u8>,
    lobby_id: LobbyId,
    seconds_remaining: u8,
    roster_size: u16,
) {
    payload.extend_from_slice(&lobby_id.get().to_le_bytes());
    payload.push(seconds_remaining);
    payload.extend_from_slice(&roster_size.to_le_bytes());
}

fn decode_countdown_started_event(
    payload: &[u8],
    index: &mut usize,
) -> Result<ServerControlEvent, PacketError> {
    Ok(ServerControlEvent::LaunchCountdownStarted {
        lobby_id: read_lobby_id(payload, index, "LaunchCountdownStarted")?,
        seconds_remaining: read_u8(payload, index, "LaunchCountdownStarted")?,
        roster_size: read_u16(payload, index, "LaunchCountdownStarted")?,
    })
}

fn encode_countdown_tick_event(payload: &mut Vec<u8>, lobby_id: LobbyId, seconds_remaining: u8) {
    payload.extend_from_slice(&lobby_id.get().to_le_bytes());
    payload.push(seconds_remaining);
}

fn decode_countdown_tick_event(
    payload: &[u8],
    index: &mut usize,
) -> Result<ServerControlEvent, PacketError> {
    Ok(ServerControlEvent::LaunchCountdownTick {
        lobby_id: read_lobby_id(payload, index, "LaunchCountdownTick")?,
        seconds_remaining: read_u8(payload, index, "LaunchCountdownTick")?,
    })
}

fn encode_match_started_event(
    payload: &mut Vec<u8>,
    match_id: MatchId,
    round: RoundNumber,
    skill_pick_seconds: u8,
) {
    payload.extend_from_slice(&match_id.get().to_le_bytes());
    payload.push(round.get());
    payload.push(skill_pick_seconds);
}

fn decode_match_started_event(
    payload: &[u8],
    index: &mut usize,
) -> Result<ServerControlEvent, PacketError> {
    Ok(ServerControlEvent::MatchStarted {
        match_id: read_match_id(payload, index, "MatchStarted")?,
        round: read_round(payload, index, "MatchStarted")?,
        skill_pick_seconds: read_u8(payload, index, "MatchStarted")?,
    })
}

fn encode_skill_chosen_event(
    payload: &mut Vec<u8>,
    player_id: PlayerId,
    tree: &SkillTree,
    tier: u8,
) -> Result<(), PacketError> {
    payload.extend_from_slice(&player_id.get().to_le_bytes());
    push_len_prefixed_string(
        payload,
        "skill_tree",
        tree.as_str(),
        MAX_SKILL_TREE_NAME_BYTES,
    )?;
    payload.push(tier);
    Ok(())
}

fn decode_skill_chosen_event(
    payload: &[u8],
    index: &mut usize,
) -> Result<ServerControlEvent, PacketError> {
    Ok(ServerControlEvent::SkillChosen {
        player_id: read_player_id(payload, index, "SkillChosen")?,
        tree: read_skill_tree(payload, index, "SkillChosen")?,
        tier: read_u8(payload, index, "SkillChosen")?,
    })
}

fn encode_round_won_event(
    payload: &mut Vec<u8>,
    round: RoundNumber,
    winning_team: TeamSide,
    score_a: u8,
    score_b: u8,
) {
    payload.push(round.get());
    payload.push(encode_team(winning_team));
    payload.push(score_a);
    payload.push(score_b);
}

fn decode_round_won_event(
    payload: &[u8],
    index: &mut usize,
) -> Result<ServerControlEvent, PacketError> {
    Ok(ServerControlEvent::RoundWon {
        round: read_round(payload, index, "RoundWon")?,
        winning_team: read_team(payload, index, "RoundWon")?,
        score_a: read_u8(payload, index, "RoundWon")?,
        score_b: read_u8(payload, index, "RoundWon")?,
    })
}

fn encode_match_ended_event(
    payload: &mut Vec<u8>,
    outcome: MatchOutcome,
    score_a: u8,
    score_b: u8,
    message: &str,
) -> Result<(), PacketError> {
    payload.push(encode_match_outcome(outcome));
    payload.push(score_a);
    payload.push(score_b);
    push_len_prefixed_string(payload, "message", message, MAX_MESSAGE_BYTES)
}

fn decode_match_ended_event(
    payload: &[u8],
    index: &mut usize,
) -> Result<ServerControlEvent, PacketError> {
    Ok(ServerControlEvent::MatchEnded {
        outcome: read_match_outcome(payload, index, "MatchEnded")?,
        score_a: read_u8(payload, index, "MatchEnded")?,
        score_b: read_u8(payload, index, "MatchEnded")?,
        message: read_string(payload, index, "MatchEnded", "message", MAX_MESSAGE_BYTES)?,
    })
}
