use arbitrary::{Arbitrary, Unstructured};
use game_net::{ClientControlCommand, NetworkSessionGuard, MAX_INGRESS_PACKET_BYTES};

use super::common::{
    normalize_lobby_id, normalize_skill_tier, sanitize_player_name, take_vec, truncate_bytes,
    FuzzReadyState, FuzzSkillTree, FuzzTeamSide,
};

const MAX_INGRESS_STEPS: usize = 16;
const MAX_RAW_PACKET_BYTES: usize = 128;

#[derive(Arbitrary, Clone, Debug)]
enum FuzzClientCommand {
    Connect { player_name: Vec<u8> },
    CreateGameLobby,
    JoinGameLobby { lobby_id: u32 },
    LeaveGameLobby,
    SelectTeam { team: FuzzTeamSide },
    SetReady { ready: FuzzReadyState },
    ChooseSkill { tree: FuzzSkillTree, tier: u8 },
    QuitToCentralLobby,
}

impl FuzzClientCommand {
    fn into_real(self) -> ClientControlCommand {
        match self {
            Self::Connect { player_name } => ClientControlCommand::Connect {
                player_name: sanitize_player_name(&player_name),
            },
            Self::CreateGameLobby => ClientControlCommand::CreateGameLobby,
            Self::JoinGameLobby { lobby_id } => ClientControlCommand::JoinGameLobby {
                lobby_id: normalize_lobby_id(lobby_id),
            },
            Self::LeaveGameLobby => ClientControlCommand::LeaveGameLobby,
            Self::SelectTeam { team } => ClientControlCommand::SelectTeam {
                team: team.into_real(),
            },
            Self::SetReady { ready } => ClientControlCommand::SetReady {
                ready: ready.into_real(),
            },
            Self::ChooseSkill { tree, tier } => ClientControlCommand::ChooseSkill {
                tree: tree.into_real(),
                tier: normalize_skill_tier(tier),
            },
            Self::QuitToCentralLobby => ClientControlCommand::QuitToCentralLobby,
        }
    }
}

#[derive(Arbitrary, Clone, Debug)]
enum FuzzIngressPacket {
    Valid {
        command: FuzzClientCommand,
        seq: u32,
        sim_tick: u32,
    },
    WrongPacketKind {
        command: FuzzClientCommand,
        seq: u32,
        sim_tick: u32,
    },
    Truncated {
        command: FuzzClientCommand,
        seq: u32,
        sim_tick: u32,
        cut: u8,
    },
    Oversized {
        command: FuzzClientCommand,
        seq: u32,
        sim_tick: u32,
        extra: Vec<u8>,
    },
    Raw {
        bytes: Vec<u8>,
    },
}

#[derive(Arbitrary, Clone, Debug)]
struct FuzzIngressSequence {
    packets: Vec<FuzzIngressPacket>,
}

impl FuzzIngressPacket {
    fn into_bytes(self) -> Vec<u8> {
        match self {
            Self::Valid {
                command,
                seq,
                sim_tick,
            } => encode_command(command, seq, sim_tick),
            Self::WrongPacketKind {
                command,
                seq,
                sim_tick,
            } => {
                let mut bytes = encode_command(command, seq, sim_tick);
                if bytes.len() > 4 {
                    bytes[4] = 7;
                }
                bytes
            }
            Self::Truncated {
                command,
                seq,
                sim_tick,
                cut,
            } => {
                let mut bytes = encode_command(command, seq, sim_tick);
                let truncation = usize::from(cut).min(bytes.len().saturating_sub(1));
                bytes.truncate(bytes.len().saturating_sub(truncation.max(1)));
                bytes
            }
            Self::Oversized {
                command,
                seq,
                sim_tick,
                extra,
            } => {
                let mut bytes = encode_command(command, seq, sim_tick);
                let extra_len = extra.len().clamp(1, 64);
                bytes.resize(MAX_INGRESS_PACKET_BYTES + extra_len, 0xAB);
                bytes
            }
            Self::Raw { bytes } => truncate_bytes(bytes, MAX_RAW_PACKET_BYTES),
        }
    }
}

pub fn run_prefixed_session_ingress_stream(bytes: &[u8]) {
    let mut guard = NetworkSessionGuard::new();
    let mut index = 0_usize;
    let mut packets_seen = 0_usize;

    while index < bytes.len() && packets_seen < MAX_INGRESS_STEPS {
        let declared_len = usize::from(bytes[index]);
        index += 1;

        let remaining = bytes.len().saturating_sub(index);
        let packet_len = declared_len.min(remaining);
        let packet = &bytes[index..index + packet_len];
        accept_with_binding(&mut guard, packet);

        index += packet_len;
        packets_seen += 1;
    }
}

pub fn run_session_ingress_sequence(bytes: &[u8]) {
    let Some(sequence) = parse_input::<FuzzIngressSequence>(bytes) else {
        return;
    };
    let mut guard = NetworkSessionGuard::new();

    for packet in take_vec(sequence.packets, MAX_INGRESS_STEPS) {
        let bytes = packet.into_bytes();
        accept_with_binding(&mut guard, &bytes);
    }
}

fn accept_with_binding(guard: &mut NetworkSessionGuard, packet: &[u8]) {
    let accepted = guard.accept_packet(packet);
    if accepted.is_ok()
        && !guard.is_bound()
        && matches!(
            ClientControlCommand::decode_packet(packet),
            Ok((_, ClientControlCommand::Connect { .. }))
        )
    {
        guard.mark_bound();
    }
}

fn parse_input<T>(bytes: &[u8]) -> Option<T>
where
    T: for<'a> Arbitrary<'a>,
{
    let mut unstructured = Unstructured::new(bytes);
    T::arbitrary(&mut unstructured).ok()
}

fn encode_command(command: FuzzClientCommand, seq: u32, sim_tick: u32) -> Vec<u8> {
    command
        .into_real()
        .encode_packet(seq, sim_tick)
        .unwrap_or_default()
}
