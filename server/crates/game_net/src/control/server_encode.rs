use game_domain::{
    LobbyId, MatchId, MatchOutcome, PlayerId, PlayerName, PlayerRecord, ReadyState, RoundNumber,
    SkillTree, TeamSide,
};

use crate::{ChannelId, PacketError, PacketHeader, PacketKind};

use super::codec::{
    encode_match_outcome, encode_ready_state, encode_team, push_len_prefixed_string,
};
use super::server_types::{ServerControlEvent, SkillCatalogEntry};
use super::snapshots_encode::{
    encode_arena_delta_snapshot, encode_arena_effect_batch, encode_arena_state_snapshot,
    encode_game_lobby_snapshot, encode_lobby_directory_snapshot, encode_player_record,
    encode_skill_catalog,
};
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

fn encode_lobby_id_event(payload: &mut Vec<u8>, lobby_id: LobbyId) {
    payload.extend_from_slice(&lobby_id.get().to_le_bytes());
}

fn encode_lobby_and_player_event(payload: &mut Vec<u8>, lobby_id: LobbyId, player_id: PlayerId) {
    payload.extend_from_slice(&lobby_id.get().to_le_bytes());
    payload.extend_from_slice(&player_id.get().to_le_bytes());
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

fn encode_countdown_tick_event(payload: &mut Vec<u8>, lobby_id: LobbyId, seconds_remaining: u8) {
    payload.extend_from_slice(&lobby_id.get().to_le_bytes());
    payload.push(seconds_remaining);
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
