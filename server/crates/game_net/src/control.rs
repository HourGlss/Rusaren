use game_domain::{
    LobbyId, MatchId, MatchOutcome, PlayerId, PlayerName, PlayerRecord, ReadyState, RoundNumber,
    SkillTree, TeamSide,
};

use crate::{ChannelId, PacketError, PacketHeader, PacketKind};

const MAX_MESSAGE_BYTES: usize = 200;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientControlCommand {
    Connect { player_name: PlayerName },
    CreateGameLobby,
    JoinGameLobby { lobby_id: LobbyId },
    LeaveGameLobby,
    SelectTeam { team: TeamSide },
    SetReady { ready: ReadyState },
    ChooseSkill { tree: SkillTree, tier: u8 },
    QuitToCentralLobby,
}

impl ClientControlCommand {
    pub fn encode_packet(self, seq: u32, sim_tick: u32) -> Result<Vec<u8>, PacketError> {
        let mut payload = Vec::new();
        payload.push(self.kind_byte());
        self.encode_body(&mut payload)?;

        let payload_len =
            u16::try_from(payload.len()).map_err(|_| PacketError::PayloadTooLarge {
                actual: payload.len(),
                maximum: usize::from(u16::MAX),
            })?;
        let header = PacketHeader::new(
            ChannelId::Control,
            PacketKind::ControlCommand,
            0,
            payload_len,
            seq,
            sim_tick,
        )?;

        Ok(header.encode(&payload))
    }

    pub fn decode_packet(packet: &[u8]) -> Result<(PacketHeader, Self), PacketError> {
        let (header, payload) = PacketHeader::decode(packet)?;
        if header.channel_id != ChannelId::Control
            || header.packet_kind != PacketKind::ControlCommand
        {
            return Err(PacketError::UnexpectedPacketKind {
                expected_channel: ChannelId::Control,
                expected_kind: PacketKind::ControlCommand,
                actual_channel: header.channel_id,
                actual_kind: header.packet_kind,
            });
        }

        let kind = *payload.first().ok_or(PacketError::ControlPayloadTooShort {
            kind: "ClientControlCommand",
            expected: 1,
            actual: payload.len(),
        })?;
        let mut index = 1usize;
        let command = Self::decode_body(kind, payload, &mut index)?;

        ensure_consumed(payload, index, "ClientControlCommand")?;
        Ok((header, command))
    }

    const fn kind_byte(&self) -> u8 {
        match self {
            Self::Connect { .. } => 1,
            Self::CreateGameLobby => 2,
            Self::JoinGameLobby { .. } => 3,
            Self::LeaveGameLobby => 4,
            Self::SelectTeam { .. } => 5,
            Self::SetReady { .. } => 6,
            Self::ChooseSkill { .. } => 7,
            Self::QuitToCentralLobby => 8,
        }
    }

    #[allow(clippy::too_many_lines)]
    #[allow(clippy::too_many_lines)]
    #[allow(clippy::too_many_lines)]
    fn encode_body(self, payload: &mut Vec<u8>) -> Result<(), PacketError> {
        match self {
            Self::Connect { player_name } => encode_connect_command(payload, &player_name),
            Self::CreateGameLobby | Self::LeaveGameLobby | Self::QuitToCentralLobby => Ok(()),
            Self::JoinGameLobby { lobby_id } => {
                payload.extend_from_slice(&lobby_id.get().to_le_bytes());
                Ok(())
            }
            Self::SelectTeam { team } => {
                payload.push(encode_team(team));
                Ok(())
            }
            Self::SetReady { ready } => {
                payload.push(encode_ready_state(ready));
                Ok(())
            }
            Self::ChooseSkill { tree, tier } => {
                payload.push(encode_skill_tree(tree));
                payload.push(tier);
                Ok(())
            }
        }
    }

    fn decode_body(kind: u8, payload: &[u8], index: &mut usize) -> Result<Self, PacketError> {
        match kind {
            1 => decode_connect_command(payload, index),
            2 => Ok(Self::CreateGameLobby),
            3 => Ok(Self::JoinGameLobby {
                lobby_id: read_lobby_id(payload, index, "JoinGameLobby")?,
            }),
            4 => Ok(Self::LeaveGameLobby),
            5 => Ok(Self::SelectTeam {
                team: read_team(payload, index, "SelectTeam")?,
            }),
            6 => Ok(Self::SetReady {
                ready: read_ready_state(payload, index, "SetReady")?,
            }),
            7 => decode_choose_skill_command(payload, index),
            8 => Ok(Self::QuitToCentralLobby),
            other => Err(PacketError::UnknownControlCommand(other)),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerControlEvent {
    Connected {
        player_id: PlayerId,
        player_name: PlayerName,
        record: PlayerRecord,
    },
    GameLobbyCreated {
        lobby_id: LobbyId,
    },
    GameLobbyJoined {
        lobby_id: LobbyId,
        player_id: PlayerId,
    },
    GameLobbyLeft {
        lobby_id: LobbyId,
        player_id: PlayerId,
    },
    TeamSelected {
        player_id: PlayerId,
        team: TeamSide,
        ready_reset: bool,
    },
    ReadyChanged {
        player_id: PlayerId,
        ready: ReadyState,
    },
    LaunchCountdownStarted {
        lobby_id: LobbyId,
        seconds_remaining: u8,
        roster_size: u16,
    },
    LaunchCountdownTick {
        lobby_id: LobbyId,
        seconds_remaining: u8,
    },
    MatchStarted {
        match_id: MatchId,
        round: RoundNumber,
        skill_pick_seconds: u8,
    },
    SkillChosen {
        player_id: PlayerId,
        tree: SkillTree,
        tier: u8,
    },
    PreCombatStarted {
        seconds_remaining: u8,
    },
    CombatStarted,
    RoundWon {
        round: RoundNumber,
        winning_team: TeamSide,
        score_a: u8,
        score_b: u8,
    },
    MatchEnded {
        outcome: MatchOutcome,
        score_a: u8,
        score_b: u8,
        message: String,
    },
    ReturnedToCentralLobby {
        record: PlayerRecord,
    },
    LobbyDirectorySnapshot {
        lobbies: Vec<LobbyDirectoryEntry>,
    },
    GameLobbySnapshot {
        lobby_id: LobbyId,
        phase: LobbySnapshotPhase,
        players: Vec<LobbySnapshotPlayer>,
    },
    ArenaStateSnapshot {
        snapshot: ArenaStateSnapshot,
    },
    ArenaEffectBatch {
        effects: Vec<ArenaEffectSnapshot>,
    },
    Error {
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LobbyDirectoryEntry {
    pub lobby_id: LobbyId,
    pub player_count: u16,
    pub team_a_count: u16,
    pub team_b_count: u16,
    pub ready_count: u16,
    pub phase: LobbySnapshotPhase,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LobbySnapshotPlayer {
    pub player_id: PlayerId,
    pub player_name: PlayerName,
    pub record: PlayerRecord,
    pub team: Option<TeamSide>,
    pub ready: ReadyState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LobbySnapshotPhase {
    Open,
    LaunchCountdown { seconds_remaining: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArenaObstacleKind {
    Pillar,
    Shrub,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArenaObstacleSnapshot {
    pub kind: ArenaObstacleKind,
    pub center_x: i16,
    pub center_y: i16,
    pub half_width: u16,
    pub half_height: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArenaPlayerSnapshot {
    pub player_id: PlayerId,
    pub player_name: PlayerName,
    pub team: TeamSide,
    pub x: i16,
    pub y: i16,
    pub aim_x: i16,
    pub aim_y: i16,
    pub hit_points: u16,
    pub max_hit_points: u16,
    pub alive: bool,
    pub unlocked_skill_slots: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArenaEffectKind {
    MeleeSwing,
    SkillShot,
    DashTrail,
    Burst,
    Nova,
    Beam,
    HitSpark,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArenaEffectSnapshot {
    pub kind: ArenaEffectKind,
    pub owner: PlayerId,
    pub slot: u8,
    pub x: i16,
    pub y: i16,
    pub target_x: i16,
    pub target_y: i16,
    pub radius: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArenaStateSnapshot {
    pub width: u16,
    pub height: u16,
    pub obstacles: Vec<ArenaObstacleSnapshot>,
    pub players: Vec<ArenaPlayerSnapshot>,
}

impl ServerControlEvent {
    pub fn encode_packet(self, seq: u32, sim_tick: u32) -> Result<Vec<u8>, PacketError> {
        let mut payload = Vec::new();
        payload.push(self.kind_byte());
        self.encode_body(&mut payload)?;

        let payload_len =
            u16::try_from(payload.len()).map_err(|_| PacketError::PayloadTooLarge {
                actual: payload.len(),
                maximum: usize::from(u16::MAX),
            })?;
        let header = PacketHeader::new(
            ChannelId::Control,
            PacketKind::ControlEvent,
            0,
            payload_len,
            seq,
            sim_tick,
        )?;

        Ok(header.encode(&payload))
    }

    pub fn decode_packet(packet: &[u8]) -> Result<(PacketHeader, Self), PacketError> {
        let (header, payload) = PacketHeader::decode(packet)?;
        if header.channel_id != ChannelId::Control || header.packet_kind != PacketKind::ControlEvent
        {
            return Err(PacketError::UnexpectedPacketKind {
                expected_channel: ChannelId::Control,
                expected_kind: PacketKind::ControlEvent,
                actual_channel: header.channel_id,
                actual_kind: header.packet_kind,
            });
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
            Self::ArenaEffectBatch { .. } => 20,
        }
    }

    #[allow(clippy::too_many_lines)]
    fn encode_body(self, payload: &mut Vec<u8>) -> Result<(), PacketError> {
        match self {
            Self::Connected {
                player_id,
                player_name,
                record,
            } => encode_connected_event(payload, player_id, &player_name, record),
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
            } => {
                encode_skill_chosen_event(payload, player_id, tree, tier);
                Ok(())
            }
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
            20 => decode_arena_effect_batch(payload, index),
            other => Err(PacketError::UnknownServerEvent(other)),
        }
    }
}

fn encode_connect_command(
    payload: &mut Vec<u8>,
    player_name: &PlayerName,
) -> Result<(), PacketError> {
    push_len_prefixed_string(
        payload,
        "player_name",
        player_name.as_str(),
        game_domain::MAX_PLAYER_NAME_LEN,
    )
}

fn decode_connect_command(
    payload: &[u8],
    index: &mut usize,
) -> Result<ClientControlCommand, PacketError> {
    Ok(ClientControlCommand::Connect {
        player_name: read_player_name(payload, index, "Connect")?,
    })
}

fn decode_choose_skill_command(
    payload: &[u8],
    index: &mut usize,
) -> Result<ClientControlCommand, PacketError> {
    Ok(ClientControlCommand::ChooseSkill {
        tree: read_skill_tree(payload, index, "ChooseSkill")?,
        tier: read_u8(payload, index, "ChooseSkill")?,
    })
}

fn encode_connected_event(
    payload: &mut Vec<u8>,
    player_id: PlayerId,
    player_name: &PlayerName,
    record: PlayerRecord,
) -> Result<(), PacketError> {
    payload.extend_from_slice(&player_id.get().to_le_bytes());
    push_len_prefixed_string(
        payload,
        "player_name",
        player_name.as_str(),
        game_domain::MAX_PLAYER_NAME_LEN,
    )?;
    encode_player_record(payload, record);
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
    tree: SkillTree,
    tier: u8,
) {
    payload.extend_from_slice(&player_id.get().to_le_bytes());
    payload.push(encode_skill_tree(tree));
    payload.push(tier);
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

fn encode_player_record(payload: &mut Vec<u8>, record: PlayerRecord) {
    payload.extend_from_slice(&record.wins.to_le_bytes());
    payload.extend_from_slice(&record.losses.to_le_bytes());
    payload.extend_from_slice(&record.no_contests.to_le_bytes());
}

fn encode_lobby_directory_snapshot(
    payload: &mut Vec<u8>,
    lobbies: &[LobbyDirectoryEntry],
) -> Result<(), PacketError> {
    let lobby_count = u16::try_from(lobbies.len()).map_err(|_| PacketError::PayloadTooLarge {
        actual: lobbies.len(),
        maximum: usize::from(u16::MAX),
    })?;
    payload.extend_from_slice(&lobby_count.to_le_bytes());
    for lobby in lobbies {
        payload.extend_from_slice(&lobby.lobby_id.get().to_le_bytes());
        payload.extend_from_slice(&lobby.player_count.to_le_bytes());
        payload.extend_from_slice(&lobby.team_a_count.to_le_bytes());
        payload.extend_from_slice(&lobby.team_b_count.to_le_bytes());
        payload.extend_from_slice(&lobby.ready_count.to_le_bytes());
        encode_lobby_snapshot_phase(payload, lobby.phase);
    }

    Ok(())
}

fn decode_lobby_directory_snapshot(
    payload: &[u8],
    index: &mut usize,
) -> Result<ServerControlEvent, PacketError> {
    let lobby_count = usize::from(read_u16(payload, index, "LobbyDirectorySnapshot")?);
    let mut lobbies = Vec::with_capacity(lobby_count);
    for _ in 0..lobby_count {
        lobbies.push(LobbyDirectoryEntry {
            lobby_id: read_lobby_id(payload, index, "LobbyDirectorySnapshot")?,
            player_count: read_u16(payload, index, "LobbyDirectorySnapshot")?,
            team_a_count: read_u16(payload, index, "LobbyDirectorySnapshot")?,
            team_b_count: read_u16(payload, index, "LobbyDirectorySnapshot")?,
            ready_count: read_u16(payload, index, "LobbyDirectorySnapshot")?,
            phase: read_lobby_snapshot_phase(payload, index, "LobbyDirectorySnapshot")?,
        });
    }

    Ok(ServerControlEvent::LobbyDirectorySnapshot { lobbies })
}

fn encode_game_lobby_snapshot(
    payload: &mut Vec<u8>,
    lobby_id: LobbyId,
    phase: LobbySnapshotPhase,
    players: &[LobbySnapshotPlayer],
) -> Result<(), PacketError> {
    payload.extend_from_slice(&lobby_id.get().to_le_bytes());
    encode_lobby_snapshot_phase(payload, phase);

    let player_count = u16::try_from(players.len()).map_err(|_| PacketError::PayloadTooLarge {
        actual: players.len(),
        maximum: usize::from(u16::MAX),
    })?;
    payload.extend_from_slice(&player_count.to_le_bytes());
    for player in players {
        payload.extend_from_slice(&player.player_id.get().to_le_bytes());
        push_len_prefixed_string(
            payload,
            "player_name",
            player.player_name.as_str(),
            game_domain::MAX_PLAYER_NAME_LEN,
        )?;
        encode_player_record(payload, player.record);
        payload.push(encode_optional_team(player.team));
        payload.push(encode_ready_state(player.ready));
    }

    Ok(())
}

fn decode_game_lobby_snapshot(
    payload: &[u8],
    index: &mut usize,
) -> Result<ServerControlEvent, PacketError> {
    let lobby_id = read_lobby_id(payload, index, "GameLobbySnapshot")?;
    let phase = read_lobby_snapshot_phase(payload, index, "GameLobbySnapshot")?;
    let player_count = usize::from(read_u16(payload, index, "GameLobbySnapshot")?);
    let mut players = Vec::with_capacity(player_count);
    for _ in 0..player_count {
        players.push(LobbySnapshotPlayer {
            player_id: read_player_id(payload, index, "GameLobbySnapshot")?,
            player_name: read_player_name(payload, index, "GameLobbySnapshot")?,
            record: read_player_record(payload, index, "GameLobbySnapshot")?,
            team: read_optional_team(payload, index, "GameLobbySnapshot")?,
            ready: read_ready_state(payload, index, "GameLobbySnapshot")?,
        });
    }

    Ok(ServerControlEvent::GameLobbySnapshot {
        lobby_id,
        phase,
        players,
    })
}

fn encode_arena_state_snapshot(
    payload: &mut Vec<u8>,
    snapshot: &ArenaStateSnapshot,
) -> Result<(), PacketError> {
    payload.extend_from_slice(&snapshot.width.to_le_bytes());
    payload.extend_from_slice(&snapshot.height.to_le_bytes());

    let obstacle_count =
        u16::try_from(snapshot.obstacles.len()).map_err(|_| PacketError::PayloadTooLarge {
            actual: snapshot.obstacles.len(),
            maximum: usize::from(u16::MAX),
        })?;
    payload.extend_from_slice(&obstacle_count.to_le_bytes());
    for obstacle in &snapshot.obstacles {
        payload.push(encode_arena_obstacle_kind(obstacle.kind));
        payload.extend_from_slice(&obstacle.center_x.to_le_bytes());
        payload.extend_from_slice(&obstacle.center_y.to_le_bytes());
        payload.extend_from_slice(&obstacle.half_width.to_le_bytes());
        payload.extend_from_slice(&obstacle.half_height.to_le_bytes());
    }

    let player_count =
        u16::try_from(snapshot.players.len()).map_err(|_| PacketError::PayloadTooLarge {
            actual: snapshot.players.len(),
            maximum: usize::from(u16::MAX),
        })?;
    payload.extend_from_slice(&player_count.to_le_bytes());
    for player in &snapshot.players {
        payload.extend_from_slice(&player.player_id.get().to_le_bytes());
        push_len_prefixed_string(
            payload,
            "player_name",
            player.player_name.as_str(),
            game_domain::MAX_PLAYER_NAME_LEN,
        )?;
        payload.push(encode_team(player.team));
        payload.extend_from_slice(&player.x.to_le_bytes());
        payload.extend_from_slice(&player.y.to_le_bytes());
        payload.extend_from_slice(&player.aim_x.to_le_bytes());
        payload.extend_from_slice(&player.aim_y.to_le_bytes());
        payload.extend_from_slice(&player.hit_points.to_le_bytes());
        payload.extend_from_slice(&player.max_hit_points.to_le_bytes());
        payload.push(u8::from(player.alive));
        payload.push(player.unlocked_skill_slots);
    }

    Ok(())
}

fn decode_arena_state_snapshot(
    payload: &[u8],
    index: &mut usize,
) -> Result<ServerControlEvent, PacketError> {
    let width = read_u16(payload, index, "ArenaStateSnapshot")?;
    let height = read_u16(payload, index, "ArenaStateSnapshot")?;
    let obstacle_count = usize::from(read_u16(payload, index, "ArenaStateSnapshot")?);
    let mut obstacles = Vec::with_capacity(obstacle_count);
    for _ in 0..obstacle_count {
        obstacles.push(ArenaObstacleSnapshot {
            kind: read_arena_obstacle_kind(payload, index, "ArenaStateSnapshot")?,
            center_x: read_i16(payload, index, "ArenaStateSnapshot")?,
            center_y: read_i16(payload, index, "ArenaStateSnapshot")?,
            half_width: read_u16(payload, index, "ArenaStateSnapshot")?,
            half_height: read_u16(payload, index, "ArenaStateSnapshot")?,
        });
    }

    let player_count = usize::from(read_u16(payload, index, "ArenaStateSnapshot")?);
    let mut players = Vec::with_capacity(player_count);
    for _ in 0..player_count {
        players.push(ArenaPlayerSnapshot {
            player_id: read_player_id(payload, index, "ArenaStateSnapshot")?,
            player_name: read_player_name(payload, index, "ArenaStateSnapshot")?,
            team: read_team(payload, index, "ArenaStateSnapshot")?,
            x: read_i16(payload, index, "ArenaStateSnapshot")?,
            y: read_i16(payload, index, "ArenaStateSnapshot")?,
            aim_x: read_i16(payload, index, "ArenaStateSnapshot")?,
            aim_y: read_i16(payload, index, "ArenaStateSnapshot")?,
            hit_points: read_u16(payload, index, "ArenaStateSnapshot")?,
            max_hit_points: read_u16(payload, index, "ArenaStateSnapshot")?,
            alive: read_bool(payload, index, "ArenaStateSnapshot")?,
            unlocked_skill_slots: read_u8(payload, index, "ArenaStateSnapshot")?,
        });
    }

    Ok(ServerControlEvent::ArenaStateSnapshot {
        snapshot: ArenaStateSnapshot {
            width,
            height,
            obstacles,
            players,
        },
    })
}

fn encode_arena_effect_batch(
    payload: &mut Vec<u8>,
    effects: &[ArenaEffectSnapshot],
) -> Result<(), PacketError> {
    let effect_count = u16::try_from(effects.len()).map_err(|_| PacketError::PayloadTooLarge {
        actual: effects.len(),
        maximum: usize::from(u16::MAX),
    })?;
    payload.extend_from_slice(&effect_count.to_le_bytes());
    for effect in effects {
        payload.push(encode_arena_effect_kind(effect.kind));
        payload.extend_from_slice(&effect.owner.get().to_le_bytes());
        payload.push(effect.slot);
        payload.extend_from_slice(&effect.x.to_le_bytes());
        payload.extend_from_slice(&effect.y.to_le_bytes());
        payload.extend_from_slice(&effect.target_x.to_le_bytes());
        payload.extend_from_slice(&effect.target_y.to_le_bytes());
        payload.extend_from_slice(&effect.radius.to_le_bytes());
    }
    Ok(())
}

fn decode_arena_effect_batch(
    payload: &[u8],
    index: &mut usize,
) -> Result<ServerControlEvent, PacketError> {
    let effect_count = usize::from(read_u16(payload, index, "ArenaEffectBatch")?);
    let mut effects = Vec::with_capacity(effect_count);
    for _ in 0..effect_count {
        effects.push(ArenaEffectSnapshot {
            kind: read_arena_effect_kind(payload, index, "ArenaEffectBatch")?,
            owner: read_player_id(payload, index, "ArenaEffectBatch")?,
            slot: read_u8(payload, index, "ArenaEffectBatch")?,
            x: read_i16(payload, index, "ArenaEffectBatch")?,
            y: read_i16(payload, index, "ArenaEffectBatch")?,
            target_x: read_i16(payload, index, "ArenaEffectBatch")?,
            target_y: read_i16(payload, index, "ArenaEffectBatch")?,
            radius: read_u16(payload, index, "ArenaEffectBatch")?,
        });
    }

    Ok(ServerControlEvent::ArenaEffectBatch { effects })
}

fn read_u8(payload: &[u8], index: &mut usize, kind: &'static str) -> Result<u8, PacketError> {
    ensure_available(payload, *index, 1, kind)?;
    let value = payload[*index];
    *index += 1;
    Ok(value)
}

fn read_i16(payload: &[u8], index: &mut usize, kind: &'static str) -> Result<i16, PacketError> {
    ensure_available(payload, *index, 2, kind)?;
    let value = i16::from_le_bytes([payload[*index], payload[*index + 1]]);
    *index += 2;
    Ok(value)
}

fn read_u16(payload: &[u8], index: &mut usize, kind: &'static str) -> Result<u16, PacketError> {
    ensure_available(payload, *index, 2, kind)?;
    let value = u16::from_le_bytes([payload[*index], payload[*index + 1]]);
    *index += 2;
    Ok(value)
}

fn read_u32(payload: &[u8], index: &mut usize, kind: &'static str) -> Result<u32, PacketError> {
    ensure_available(payload, *index, 4, kind)?;
    let value = u32::from_le_bytes([
        payload[*index],
        payload[*index + 1],
        payload[*index + 2],
        payload[*index + 3],
    ]);
    *index += 4;
    Ok(value)
}

fn read_bool(payload: &[u8], index: &mut usize, kind: &'static str) -> Result<bool, PacketError> {
    match read_u8(payload, index, kind)? {
        0 => Ok(false),
        1 => Ok(true),
        other => Err(PacketError::InvalidEncodedBoolean(other)),
    }
}

fn read_player_id(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<PlayerId, PacketError> {
    let raw = read_u32(payload, index, kind)?;
    PlayerId::new(raw).map_err(|_| PacketError::InvalidEncodedPlayerId(raw))
}

fn read_lobby_id(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<LobbyId, PacketError> {
    let raw = read_u32(payload, index, kind)?;
    LobbyId::new(raw).map_err(|_| PacketError::InvalidEncodedLobbyId(raw))
}

fn read_match_id(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<MatchId, PacketError> {
    let raw = read_u32(payload, index, kind)?;
    MatchId::new(raw).map_err(|_| PacketError::InvalidEncodedMatchId(raw))
}

fn read_round(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<RoundNumber, PacketError> {
    let raw = read_u8(payload, index, kind)?;
    RoundNumber::new(raw).map_err(|_| PacketError::InvalidEncodedRound(raw))
}

fn read_player_name(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<PlayerName, PacketError> {
    let name = read_string(
        payload,
        index,
        kind,
        "player_name",
        game_domain::MAX_PLAYER_NAME_LEN,
    )?;
    PlayerName::new(name).map_err(PacketError::InvalidEncodedPlayerName)
}

fn read_string(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
    field: &'static str,
    max_len: usize,
) -> Result<String, PacketError> {
    let len = usize::from(read_u8(payload, index, kind)?);
    if len > max_len {
        return Err(PacketError::StringLengthOutOfBounds {
            field,
            len,
            max: max_len,
        });
    }

    ensure_available(payload, *index, len, kind)?;
    let bytes = &payload[*index..*index + len];
    *index += len;

    String::from_utf8(bytes.to_vec()).map_err(|_| PacketError::InvalidUtf8String { field })
}

fn read_player_record(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<PlayerRecord, PacketError> {
    Ok(PlayerRecord {
        wins: read_u16(payload, index, kind)?,
        losses: read_u16(payload, index, kind)?,
        no_contests: read_u16(payload, index, kind)?,
    })
}

fn read_team(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<TeamSide, PacketError> {
    match read_u8(payload, index, kind)? {
        1 => Ok(TeamSide::TeamA),
        2 => Ok(TeamSide::TeamB),
        other => Err(PacketError::InvalidEncodedTeam(other)),
    }
}

fn read_optional_team(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<Option<TeamSide>, PacketError> {
    match read_u8(payload, index, kind)? {
        0 => Ok(None),
        1 => Ok(Some(TeamSide::TeamA)),
        2 => Ok(Some(TeamSide::TeamB)),
        other => Err(PacketError::InvalidEncodedOptionalTeam(other)),
    }
}

fn read_ready_state(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<ReadyState, PacketError> {
    match read_u8(payload, index, kind)? {
        0 => Ok(ReadyState::NotReady),
        1 => Ok(ReadyState::Ready),
        other => Err(PacketError::InvalidEncodedReadyState(other)),
    }
}

fn read_skill_tree(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<SkillTree, PacketError> {
    match read_u8(payload, index, kind)? {
        1 => Ok(SkillTree::Warrior),
        2 => Ok(SkillTree::Rogue),
        3 => Ok(SkillTree::Mage),
        4 => Ok(SkillTree::Cleric),
        other => Err(PacketError::InvalidEncodedSkillTree(other)),
    }
}

fn read_match_outcome(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<MatchOutcome, PacketError> {
    match read_u8(payload, index, kind)? {
        1 => Ok(MatchOutcome::TeamAWin),
        2 => Ok(MatchOutcome::TeamBWin),
        3 => Ok(MatchOutcome::NoContest),
        other => Err(PacketError::InvalidEncodedMatchOutcome(other)),
    }
}

fn read_lobby_snapshot_phase(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<LobbySnapshotPhase, PacketError> {
    match read_u8(payload, index, kind)? {
        0 => Ok(LobbySnapshotPhase::Open),
        1 => Ok(LobbySnapshotPhase::LaunchCountdown {
            seconds_remaining: read_u8(payload, index, kind)?,
        }),
        other => Err(PacketError::InvalidEncodedLobbyPhase(other)),
    }
}

fn encode_team(team: TeamSide) -> u8 {
    match team {
        TeamSide::TeamA => 1,
        TeamSide::TeamB => 2,
    }
}

fn encode_optional_team(team: Option<TeamSide>) -> u8 {
    match team {
        None => 0,
        Some(TeamSide::TeamA) => 1,
        Some(TeamSide::TeamB) => 2,
    }
}

fn encode_ready_state(ready: ReadyState) -> u8 {
    match ready {
        ReadyState::NotReady => 0,
        ReadyState::Ready => 1,
    }
}

fn encode_skill_tree(tree: SkillTree) -> u8 {
    match tree {
        SkillTree::Warrior => 1,
        SkillTree::Rogue => 2,
        SkillTree::Mage => 3,
        SkillTree::Cleric => 4,
    }
}

fn encode_match_outcome(outcome: MatchOutcome) -> u8 {
    match outcome {
        MatchOutcome::TeamAWin => 1,
        MatchOutcome::TeamBWin => 2,
        MatchOutcome::NoContest => 3,
    }
}

fn encode_arena_obstacle_kind(kind: ArenaObstacleKind) -> u8 {
    match kind {
        ArenaObstacleKind::Pillar => 1,
        ArenaObstacleKind::Shrub => 2,
    }
}

fn encode_arena_effect_kind(kind: ArenaEffectKind) -> u8 {
    match kind {
        ArenaEffectKind::MeleeSwing => 1,
        ArenaEffectKind::SkillShot => 2,
        ArenaEffectKind::DashTrail => 3,
        ArenaEffectKind::Burst => 4,
        ArenaEffectKind::Nova => 5,
        ArenaEffectKind::Beam => 6,
        ArenaEffectKind::HitSpark => 7,
    }
}

fn read_arena_obstacle_kind(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<ArenaObstacleKind, PacketError> {
    match read_u8(payload, index, kind)? {
        1 => Ok(ArenaObstacleKind::Pillar),
        2 => Ok(ArenaObstacleKind::Shrub),
        other => Err(PacketError::InvalidEncodedArenaObstacleKind(other)),
    }
}

fn read_arena_effect_kind(
    payload: &[u8],
    index: &mut usize,
    kind: &'static str,
) -> Result<ArenaEffectKind, PacketError> {
    match read_u8(payload, index, kind)? {
        1 => Ok(ArenaEffectKind::MeleeSwing),
        2 => Ok(ArenaEffectKind::SkillShot),
        3 => Ok(ArenaEffectKind::DashTrail),
        4 => Ok(ArenaEffectKind::Burst),
        5 => Ok(ArenaEffectKind::Nova),
        6 => Ok(ArenaEffectKind::Beam),
        7 => Ok(ArenaEffectKind::HitSpark),
        other => Err(PacketError::InvalidEncodedArenaEffectKind(other)),
    }
}

fn encode_lobby_snapshot_phase(payload: &mut Vec<u8>, phase: LobbySnapshotPhase) {
    match phase {
        LobbySnapshotPhase::Open => payload.push(0),
        LobbySnapshotPhase::LaunchCountdown { seconds_remaining } => {
            payload.push(1);
            payload.push(seconds_remaining);
        }
    }
}

fn push_len_prefixed_string(
    payload: &mut Vec<u8>,
    field: &'static str,
    value: &str,
    max_len: usize,
) -> Result<(), PacketError> {
    let bytes = value.as_bytes();
    if bytes.len() > max_len {
        return Err(PacketError::StringLengthOutOfBounds {
            field,
            len: bytes.len(),
            max: max_len,
        });
    }

    let Ok(len) = u8::try_from(bytes.len()) else {
        return Err(PacketError::StringLengthOutOfBounds {
            field,
            len: bytes.len(),
            max: usize::from(u8::MAX),
        });
    };

    payload.push(len);
    payload.extend_from_slice(bytes);
    Ok(())
}

fn ensure_available(
    payload: &[u8],
    index: usize,
    needed: usize,
    kind: &'static str,
) -> Result<(), PacketError> {
    let expected = index.saturating_add(needed);
    if payload.len() < expected {
        return Err(PacketError::ControlPayloadTooShort {
            kind,
            expected,
            actual: payload.len(),
        });
    }

    Ok(())
}

fn ensure_consumed(payload: &[u8], index: usize, kind: &'static str) -> Result<(), PacketError> {
    if payload.len() != index {
        return Err(PacketError::UnexpectedTrailingBytes {
            kind,
            actual: payload.len() - index,
        });
    }

    Ok(())
}
