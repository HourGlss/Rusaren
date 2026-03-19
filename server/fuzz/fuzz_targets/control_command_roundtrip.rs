#![no_main]

use arbitrary::Arbitrary;
use game_domain::{
    LobbyId, PlayerName, ReadyState, SkillTree, TeamSide, MAX_PLAYER_NAME_LEN, MAX_SKILL_TIER,
};
use game_net::ClientControlCommand;
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug)]
enum FuzzTeamSide {
    TeamA,
    TeamB,
}

#[derive(Arbitrary, Debug)]
enum FuzzSkillTree {
    Warrior,
    Rogue,
    Mage,
    Cleric,
}

#[derive(Arbitrary, Debug)]
enum FuzzClientControlCommand {
    Connect { player_name: Vec<u8> },
    CreateGameLobby,
    JoinGameLobby { lobby_id: u32 },
    LeaveGameLobby,
    SelectTeam { team: FuzzTeamSide },
    SetReady { ready: bool },
    ChooseSkill { tree: FuzzSkillTree, tier: u8 },
    QuitToCentralLobby,
}

impl FuzzClientControlCommand {
    fn into_real(self) -> Option<ClientControlCommand> {
        match self {
            Self::Connect { player_name } => Some(ClientControlCommand::Connect {
                player_name: sanitized_player_name(&player_name)?,
            }),
            Self::CreateGameLobby => Some(ClientControlCommand::CreateGameLobby),
            Self::JoinGameLobby { lobby_id } => Some(ClientControlCommand::JoinGameLobby {
                lobby_id: LobbyId::new(lobby_id.max(1)).ok()?,
            }),
            Self::LeaveGameLobby => Some(ClientControlCommand::LeaveGameLobby),
            Self::SelectTeam { team } => Some(ClientControlCommand::SelectTeam {
                team: match team {
                    FuzzTeamSide::TeamA => TeamSide::TeamA,
                    FuzzTeamSide::TeamB => TeamSide::TeamB,
                },
            }),
            Self::SetReady { ready } => Some(ClientControlCommand::SetReady {
                ready: if ready {
                    ReadyState::Ready
                } else {
                    ReadyState::NotReady
                },
            }),
            Self::ChooseSkill { tree, tier } => Some(ClientControlCommand::ChooseSkill {
                tree: match tree {
                    FuzzSkillTree::Warrior => SkillTree::Warrior,
                    FuzzSkillTree::Rogue => SkillTree::Rogue,
                    FuzzSkillTree::Mage => SkillTree::Mage,
                    FuzzSkillTree::Cleric => SkillTree::Cleric,
                },
                tier: normalize_skill_tier(tier),
            }),
            Self::QuitToCentralLobby => Some(ClientControlCommand::QuitToCentralLobby),
        }
    }
}

fn sanitized_player_name(raw: &[u8]) -> Option<PlayerName> {
    let bytes = if raw.is_empty() {
        b"Player".as_slice()
    } else {
        raw
    };
    let mut normalized = String::with_capacity(bytes.len().min(MAX_PLAYER_NAME_LEN));
    for byte in bytes.iter().copied().take(MAX_PLAYER_NAME_LEN) {
        let mapped = match byte % 64 {
            0..=9 => char::from(b'0' + (byte % 10)),
            10..=35 => char::from(b'A' + ((byte - 10) % 26)),
            36..=61 => char::from(b'a' + ((byte - 36) % 26)),
            62 => '_',
            _ => '-',
        };
        normalized.push(mapped);
    }

    PlayerName::new(normalized).ok()
}

fn normalize_skill_tier(raw: u8) -> u8 {
    (raw % MAX_SKILL_TIER).saturating_add(1)
}

fuzz_target!(|input: FuzzClientControlCommand| {
    let Some(command) = input.into_real() else {
        return;
    };

    let seq = 7;
    let sim_tick = 11;
    let packet = command
        .clone()
        .encode_packet(seq, sim_tick)
        .expect("valid fuzz command should encode");
    let (header, decoded) =
        ClientControlCommand::decode_packet(&packet).expect("encoded fuzz command should decode");

    assert_eq!(decoded, command);
    assert_eq!(header.seq, seq);
    assert_eq!(header.sim_tick, sim_tick);
});
