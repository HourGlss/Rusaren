use game_domain::{LobbyId, PlayerName, ReadyState, SkillTree, TeamSide};

use crate::{ChannelId, PacketError, PacketHeader, PacketKind};

use super::codec::{
    encode_ready_state, encode_team, ensure_consumed, push_len_prefixed_string, read_lobby_id,
    read_player_name, read_ready_state, read_skill_tree, read_team, read_u8,
};
use super::MAX_SKILL_TREE_NAME_BYTES;

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
                push_len_prefixed_string(
                    payload,
                    "skill_tree",
                    tree.as_str(),
                    MAX_SKILL_TREE_NAME_BYTES,
                )?;
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
