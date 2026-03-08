use game_domain::PlayerId;

use crate::{ClientControlCommand, PacketError};

pub const MAX_INGRESS_PACKET_BYTES: usize = 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NetworkSessionGuard {
    bound_player: Option<PlayerId>,
}

impl NetworkSessionGuard {
    #[must_use]
    pub const fn new() -> Self {
        Self { bound_player: None }
    }

    pub fn accept_packet(&mut self, packet: &[u8]) -> Result<PlayerId, PacketError> {
        if packet.len() > MAX_INGRESS_PACKET_BYTES {
            return Err(PacketError::IngressPacketTooLarge {
                actual: packet.len(),
                maximum: MAX_INGRESS_PACKET_BYTES,
            });
        }

        match self.bound_player {
            None => {
                let (_, command) = ClientControlCommand::decode_packet(packet)?;
                match command {
                    ClientControlCommand::Connect { player_id, .. } => {
                        self.bound_player = Some(player_id);
                        Ok(player_id)
                    }
                    _ => Err(PacketError::FirstPacketMustBeConnect),
                }
            }
            Some(player_id) => {
                if matches!(
                    ClientControlCommand::decode_packet(packet),
                    Ok((_, ClientControlCommand::Connect { .. }))
                ) {
                    return Err(PacketError::ConnectCommandAfterBinding);
                }

                Ok(player_id)
            }
        }
    }

    #[must_use]
    pub const fn bound_player(self) -> Option<PlayerId> {
        self.bound_player
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_domain::{PlayerName, ReadyState, TeamSide};

    fn player_id(raw: u32) -> PlayerId {
        match PlayerId::new(raw) {
            Ok(player_id) => player_id,
            Err(error) => panic!("valid player id expected: {error}"),
        }
    }

    fn player_name(raw: &str) -> PlayerName {
        match PlayerName::new(raw) {
            Ok(player_name) => player_name,
            Err(error) => panic!("valid player name expected: {error}"),
        }
    }

    #[test]
    fn ingress_guard_requires_connect_before_other_packets() {
        let mut guard = NetworkSessionGuard::new();
        let packet = match (ClientControlCommand::SetReady {
            ready: ReadyState::Ready,
        })
        .encode_packet(1, 0)
        {
            Ok(packet) => packet,
            Err(error) => panic!("packet should encode: {error}"),
        };

        assert_eq!(
            guard.accept_packet(&packet),
            Err(PacketError::FirstPacketMustBeConnect)
        );
    }

    #[test]
    fn ingress_guard_binds_on_connect_and_rejects_rebinding() {
        let mut guard = NetworkSessionGuard::new();
        let connect = match (ClientControlCommand::Connect {
            player_id: player_id(7),
            player_name: player_name("Alice"),
        })
        .encode_packet(1, 0)
        {
            Ok(packet) => packet,
            Err(error) => panic!("packet should encode: {error}"),
        };

        assert_eq!(guard.accept_packet(&connect), Ok(player_id(7)));
        assert_eq!(guard.bound_player(), Some(player_id(7)));

        let select_team = match (ClientControlCommand::SelectTeam {
            team: TeamSide::TeamA,
        })
        .encode_packet(2, 0)
        {
            Ok(packet) => packet,
            Err(error) => panic!("packet should encode: {error}"),
        };
        assert_eq!(guard.accept_packet(&select_team), Ok(player_id(7)));

        let reconnect = match (ClientControlCommand::Connect {
            player_id: player_id(8),
            player_name: player_name("Mallory"),
        })
        .encode_packet(3, 0)
        {
            Ok(packet) => packet,
            Err(error) => panic!("packet should encode: {error}"),
        };
        assert_eq!(
            guard.accept_packet(&reconnect),
            Err(PacketError::ConnectCommandAfterBinding)
        );
    }

    #[test]
    fn ingress_guard_rejects_oversized_packets() {
        let mut guard = NetworkSessionGuard::new();
        let packet = vec![0_u8; MAX_INGRESS_PACKET_BYTES + 1];

        assert_eq!(
            guard.accept_packet(&packet),
            Err(PacketError::IngressPacketTooLarge {
                actual: MAX_INGRESS_PACKET_BYTES + 1,
                maximum: MAX_INGRESS_PACKET_BYTES,
            })
        );
    }
}
