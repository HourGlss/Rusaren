use game_domain::{
    LobbyId, MatchId, MatchOutcome, PlayerId, PlayerName, PlayerRecord, ReadyState, RoundNumber,
    SkillTree, TeamSide,
};

use crate::{ChannelId, PacketError, PacketHeader, PacketKind};

const MAX_MESSAGE_BYTES: usize = 200;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientControlCommand {
    Connect {
        player_id: PlayerId,
        player_name: PlayerName,
    },
    CreateGameLobby,
    JoinGameLobby {
        lobby_id: LobbyId,
    },
    LeaveGameLobby,
    SelectTeam {
        team: TeamSide,
    },
    SetReady {
        ready: ReadyState,
    },
    ChooseSkill {
        tree: SkillTree,
        tier: u8,
    },
    QuitToCentralLobby,
}

impl ClientControlCommand {
    pub fn encode_packet(self, seq: u32, sim_tick: u32) -> Result<Vec<u8>, PacketError> {
        let mut payload = Vec::new();

        match self {
            Self::Connect {
                player_id,
                player_name,
            } => {
                payload.push(1);
                payload.extend_from_slice(&player_id.get().to_le_bytes());
                push_len_prefixed_string(
                    &mut payload,
                    "player_name",
                    player_name.as_str(),
                    game_domain::MAX_PLAYER_NAME_LEN,
                )?;
            }
            Self::CreateGameLobby => payload.push(2),
            Self::JoinGameLobby { lobby_id } => {
                payload.push(3);
                payload.extend_from_slice(&lobby_id.get().to_le_bytes());
            }
            Self::LeaveGameLobby => payload.push(4),
            Self::SelectTeam { team } => {
                payload.push(5);
                payload.push(encode_team(team));
            }
            Self::SetReady { ready } => {
                payload.push(6);
                payload.push(encode_ready_state(ready));
            }
            Self::ChooseSkill { tree, tier } => {
                payload.push(7);
                payload.push(encode_skill_tree(tree));
                payload.push(tier);
            }
            Self::QuitToCentralLobby => payload.push(8),
        }

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

    #[allow(clippy::too_many_lines)]
    #[allow(clippy::too_many_lines)]
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

        let command = match kind {
            1 => {
                let player_id = read_player_id(payload, &mut index, "Connect")?;
                let player_name = read_player_name(payload, &mut index, "Connect")?;
                Self::Connect {
                    player_id,
                    player_name,
                }
            }
            2 => Self::CreateGameLobby,
            3 => Self::JoinGameLobby {
                lobby_id: read_lobby_id(payload, &mut index, "JoinGameLobby")?,
            },
            4 => Self::LeaveGameLobby,
            5 => Self::SelectTeam {
                team: read_team(payload, &mut index, "SelectTeam")?,
            },
            6 => Self::SetReady {
                ready: read_ready_state(payload, &mut index, "SetReady")?,
            },
            7 => {
                let tree = read_skill_tree(payload, &mut index, "ChooseSkill")?;
                let tier = read_u8(payload, &mut index, "ChooseSkill")?;
                Self::ChooseSkill { tree, tier }
            }
            8 => Self::QuitToCentralLobby,
            other => return Err(PacketError::UnknownControlCommand(other)),
        };

        ensure_consumed(payload, index, "ClientControlCommand")?;
        Ok((header, command))
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
    LaunchCountdown {
        seconds_remaining: u8,
    },
}

#[allow(clippy::too_many_lines)]
impl ServerControlEvent {
    #[allow(clippy::too_many_lines)]
    pub fn encode_packet(self, seq: u32, sim_tick: u32) -> Result<Vec<u8>, PacketError> {
        let mut payload = Vec::new();

        match self {
            Self::Connected {
                player_id,
                player_name,
                record,
            } => {
                payload.push(1);
                payload.extend_from_slice(&player_id.get().to_le_bytes());
                push_len_prefixed_string(
                    &mut payload,
                    "player_name",
                    player_name.as_str(),
                    game_domain::MAX_PLAYER_NAME_LEN,
                )?;
                payload.extend_from_slice(&record.wins.to_le_bytes());
                payload.extend_from_slice(&record.losses.to_le_bytes());
                payload.extend_from_slice(&record.no_contests.to_le_bytes());
            }
            Self::GameLobbyCreated { lobby_id } => {
                payload.push(2);
                payload.extend_from_slice(&lobby_id.get().to_le_bytes());
            }
            Self::GameLobbyJoined {
                lobby_id,
                player_id,
            } => {
                payload.push(3);
                payload.extend_from_slice(&lobby_id.get().to_le_bytes());
                payload.extend_from_slice(&player_id.get().to_le_bytes());
            }
            Self::GameLobbyLeft {
                lobby_id,
                player_id,
            } => {
                payload.push(4);
                payload.extend_from_slice(&lobby_id.get().to_le_bytes());
                payload.extend_from_slice(&player_id.get().to_le_bytes());
            }
            Self::TeamSelected {
                player_id,
                team,
                ready_reset,
            } => {
                payload.push(5);
                payload.extend_from_slice(&player_id.get().to_le_bytes());
                payload.push(encode_team(team));
                payload.push(u8::from(ready_reset));
            }
            Self::ReadyChanged { player_id, ready } => {
                payload.push(6);
                payload.extend_from_slice(&player_id.get().to_le_bytes());
                payload.push(encode_ready_state(ready));
            }
            Self::LaunchCountdownStarted {
                lobby_id,
                seconds_remaining,
                roster_size,
            } => {
                payload.push(7);
                payload.extend_from_slice(&lobby_id.get().to_le_bytes());
                payload.push(seconds_remaining);
                payload.extend_from_slice(&roster_size.to_le_bytes());
            }
            Self::LaunchCountdownTick {
                lobby_id,
                seconds_remaining,
            } => {
                payload.push(8);
                payload.extend_from_slice(&lobby_id.get().to_le_bytes());
                payload.push(seconds_remaining);
            }
            Self::MatchStarted {
                match_id,
                round,
                skill_pick_seconds,
            } => {
                payload.push(9);
                payload.extend_from_slice(&match_id.get().to_le_bytes());
                payload.push(round.get());
                payload.push(skill_pick_seconds);
            }
            Self::SkillChosen {
                player_id,
                tree,
                tier,
            } => {
                payload.push(10);
                payload.extend_from_slice(&player_id.get().to_le_bytes());
                payload.push(encode_skill_tree(tree));
                payload.push(tier);
            }
            Self::PreCombatStarted { seconds_remaining } => {
                payload.push(11);
                payload.push(seconds_remaining);
            }
            Self::CombatStarted => payload.push(12),
            Self::RoundWon {
                round,
                winning_team,
                score_a,
                score_b,
            } => {
                payload.push(13);
                payload.push(round.get());
                payload.push(encode_team(winning_team));
                payload.push(score_a);
                payload.push(score_b);
            }
            Self::MatchEnded {
                outcome,
                score_a,
                score_b,
                message,
            } => {
                payload.push(14);
                payload.push(encode_match_outcome(outcome));
                payload.push(score_a);
                payload.push(score_b);
                push_len_prefixed_string(&mut payload, "message", &message, MAX_MESSAGE_BYTES)?;
            }
            Self::ReturnedToCentralLobby { record } => {
                payload.push(15);
                payload.extend_from_slice(&record.wins.to_le_bytes());
                payload.extend_from_slice(&record.losses.to_le_bytes());
                payload.extend_from_slice(&record.no_contests.to_le_bytes());
            }
            Self::LobbyDirectorySnapshot { lobbies } => {
                payload.push(17);
                let lobby_count =
                    u16::try_from(lobbies.len()).map_err(|_| PacketError::PayloadTooLarge {
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
                    encode_lobby_snapshot_phase(&mut payload, lobby.phase);
                }
            }
            Self::GameLobbySnapshot {
                lobby_id,
                phase,
                players,
            } => {
                payload.push(18);
                payload.extend_from_slice(&lobby_id.get().to_le_bytes());
                encode_lobby_snapshot_phase(&mut payload, phase);
                let player_count =
                    u16::try_from(players.len()).map_err(|_| PacketError::PayloadTooLarge {
                        actual: players.len(),
                        maximum: usize::from(u16::MAX),
                    })?;
                payload.extend_from_slice(&player_count.to_le_bytes());
                for player in players {
                    payload.extend_from_slice(&player.player_id.get().to_le_bytes());
                    push_len_prefixed_string(
                        &mut payload,
                        "player_name",
                        player.player_name.as_str(),
                        game_domain::MAX_PLAYER_NAME_LEN,
                    )?;
                    payload.extend_from_slice(&player.record.wins.to_le_bytes());
                    payload.extend_from_slice(&player.record.losses.to_le_bytes());
                    payload.extend_from_slice(&player.record.no_contests.to_le_bytes());
                    payload.push(encode_optional_team(player.team));
                    payload.push(encode_ready_state(player.ready));
                }
            }
            Self::Error { message } => {
                payload.push(16);
                push_len_prefixed_string(&mut payload, "message", &message, MAX_MESSAGE_BYTES)?;
            }
        }

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

        let event = match kind {
            1 => Self::Connected {
                player_id: read_player_id(payload, &mut index, "Connected")?,
                player_name: read_player_name(payload, &mut index, "Connected")?,
                record: read_player_record(payload, &mut index, "Connected")?,
            },
            2 => Self::GameLobbyCreated {
                lobby_id: read_lobby_id(payload, &mut index, "GameLobbyCreated")?,
            },
            3 => Self::GameLobbyJoined {
                lobby_id: read_lobby_id(payload, &mut index, "GameLobbyJoined")?,
                player_id: read_player_id(payload, &mut index, "GameLobbyJoined")?,
            },
            4 => Self::GameLobbyLeft {
                lobby_id: read_lobby_id(payload, &mut index, "GameLobbyLeft")?,
                player_id: read_player_id(payload, &mut index, "GameLobbyLeft")?,
            },
            5 => Self::TeamSelected {
                player_id: read_player_id(payload, &mut index, "TeamSelected")?,
                team: read_team(payload, &mut index, "TeamSelected")?,
                ready_reset: read_bool(payload, &mut index, "TeamSelected")?,
            },
            6 => Self::ReadyChanged {
                player_id: read_player_id(payload, &mut index, "ReadyChanged")?,
                ready: read_ready_state(payload, &mut index, "ReadyChanged")?,
            },
            7 => Self::LaunchCountdownStarted {
                lobby_id: read_lobby_id(payload, &mut index, "LaunchCountdownStarted")?,
                seconds_remaining: read_u8(payload, &mut index, "LaunchCountdownStarted")?,
                roster_size: read_u16(payload, &mut index, "LaunchCountdownStarted")?,
            },
            8 => Self::LaunchCountdownTick {
                lobby_id: read_lobby_id(payload, &mut index, "LaunchCountdownTick")?,
                seconds_remaining: read_u8(payload, &mut index, "LaunchCountdownTick")?,
            },
            9 => Self::MatchStarted {
                match_id: read_match_id(payload, &mut index, "MatchStarted")?,
                round: read_round(payload, &mut index, "MatchStarted")?,
                skill_pick_seconds: read_u8(payload, &mut index, "MatchStarted")?,
            },
            10 => Self::SkillChosen {
                player_id: read_player_id(payload, &mut index, "SkillChosen")?,
                tree: read_skill_tree(payload, &mut index, "SkillChosen")?,
                tier: read_u8(payload, &mut index, "SkillChosen")?,
            },
            11 => Self::PreCombatStarted {
                seconds_remaining: read_u8(payload, &mut index, "PreCombatStarted")?,
            },
            12 => Self::CombatStarted,
            13 => Self::RoundWon {
                round: read_round(payload, &mut index, "RoundWon")?,
                winning_team: read_team(payload, &mut index, "RoundWon")?,
                score_a: read_u8(payload, &mut index, "RoundWon")?,
                score_b: read_u8(payload, &mut index, "RoundWon")?,
            },
            14 => Self::MatchEnded {
                outcome: read_match_outcome(payload, &mut index, "MatchEnded")?,
                score_a: read_u8(payload, &mut index, "MatchEnded")?,
                score_b: read_u8(payload, &mut index, "MatchEnded")?,
                message: read_string(
                    payload,
                    &mut index,
                    "MatchEnded",
                    "message",
                    MAX_MESSAGE_BYTES,
                )?,
            },
            15 => Self::ReturnedToCentralLobby {
                record: read_player_record(payload, &mut index, "ReturnedToCentralLobby")?,
            },
            17 => {
                let lobby_count = usize::from(read_u16(
                    payload,
                    &mut index,
                    "LobbyDirectorySnapshot",
                )?);
                let mut lobbies = Vec::with_capacity(lobby_count);
                for _ in 0..lobby_count {
                    lobbies.push(LobbyDirectoryEntry {
                        lobby_id: read_lobby_id(payload, &mut index, "LobbyDirectorySnapshot")?,
                        player_count: read_u16(payload, &mut index, "LobbyDirectorySnapshot")?,
                        team_a_count: read_u16(payload, &mut index, "LobbyDirectorySnapshot")?,
                        team_b_count: read_u16(payload, &mut index, "LobbyDirectorySnapshot")?,
                        ready_count: read_u16(payload, &mut index, "LobbyDirectorySnapshot")?,
                        phase: read_lobby_snapshot_phase(
                            payload,
                            &mut index,
                            "LobbyDirectorySnapshot",
                        )?,
                    });
                }
                Self::LobbyDirectorySnapshot { lobbies }
            }
            18 => {
                let lobby_id = read_lobby_id(payload, &mut index, "GameLobbySnapshot")?;
                let phase = read_lobby_snapshot_phase(payload, &mut index, "GameLobbySnapshot")?;
                let player_count = usize::from(read_u16(payload, &mut index, "GameLobbySnapshot")?);
                let mut players = Vec::with_capacity(player_count);
                for _ in 0..player_count {
                    players.push(LobbySnapshotPlayer {
                        player_id: read_player_id(payload, &mut index, "GameLobbySnapshot")?,
                        player_name: read_player_name(payload, &mut index, "GameLobbySnapshot")?,
                        record: read_player_record(payload, &mut index, "GameLobbySnapshot")?,
                        team: read_optional_team(payload, &mut index, "GameLobbySnapshot")?,
                        ready: read_ready_state(payload, &mut index, "GameLobbySnapshot")?,
                    });
                }
                Self::GameLobbySnapshot {
                    lobby_id,
                    phase,
                    players,
                }
            }
            16 => Self::Error {
                message: read_string(payload, &mut index, "Error", "message", MAX_MESSAGE_BYTES)?,
            },
            other => return Err(PacketError::UnknownServerEvent(other)),
        };

        ensure_consumed(payload, index, "ServerControlEvent")?;
        Ok((header, event))
    }
}

fn read_u8(payload: &[u8], index: &mut usize, kind: &'static str) -> Result<u8, PacketError> {
    ensure_available(payload, *index, 1, kind)?;
    let value = payload[*index];
    *index += 1;
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
