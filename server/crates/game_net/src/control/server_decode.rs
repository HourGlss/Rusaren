use game_domain::{LobbyId, PlayerId};

use crate::{ChannelId, PacketError, PacketHeader, PacketKind};

use super::codec::{
    decode_skill_catalog, ensure_consumed, read_bool, read_lobby_id, read_match_id,
    read_match_outcome, read_player_id, read_player_name, read_player_record, read_ready_state,
    read_round, read_skill_tree, read_string, read_team, read_u16, read_u8,
};
use super::server_types::ServerControlEvent;
use super::snapshots_decode::{
    decode_arena_delta_snapshot, decode_arena_effect_batch, decode_arena_state_snapshot,
    decode_game_lobby_snapshot, decode_lobby_directory_snapshot,
};
use super::MAX_MESSAGE_BYTES;

impl ServerControlEvent {
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

fn decode_countdown_tick_event(
    payload: &[u8],
    index: &mut usize,
) -> Result<ServerControlEvent, PacketError> {
    Ok(ServerControlEvent::LaunchCountdownTick {
        lobby_id: read_lobby_id(payload, index, "LaunchCountdownTick")?,
        seconds_remaining: read_u8(payload, index, "LaunchCountdownTick")?,
    })
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
