use crate::{ClientControlCommand, PacketError};

pub const MAX_INGRESS_PACKET_BYTES: usize = 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NetworkSessionGuard {
    is_bound: bool,
}

impl NetworkSessionGuard {
    #[must_use]
    pub const fn new() -> Self {
        Self { is_bound: false }
    }

    pub fn accept_packet(self, packet: &[u8]) -> Result<(), PacketError> {
        if packet.len() > MAX_INGRESS_PACKET_BYTES {
            return Err(PacketError::IngressPacketTooLarge {
                actual: packet.len(),
                maximum: MAX_INGRESS_PACKET_BYTES,
            });
        }

        if !self.is_bound {
            let (_, command) = ClientControlCommand::decode_packet(packet)?;
            return match command {
                ClientControlCommand::Connect { .. } => Ok(()),
                _ => Err(PacketError::FirstPacketMustBeConnect),
            };
        }

        if matches!(
            ClientControlCommand::decode_packet(packet),
            Ok((_, ClientControlCommand::Connect { .. }))
        ) {
            return Err(PacketError::ConnectCommandAfterBinding);
        }

        Ok(())
    }

    pub fn mark_bound(&mut self) {
        self.is_bound = true;
    }

    #[must_use]
    pub const fn is_bound(self) -> bool {
        self.is_bound
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_domain::{PlayerName, ReadyState, TeamSide};

    fn player_name(raw: &str) -> PlayerName {
        match PlayerName::new(raw) {
            Ok(player_name) => player_name,
            Err(error) => panic!("valid player name expected: {error}"),
        }
    }

    #[test]
    fn ingress_guard_requires_connect_before_other_packets() {
        let guard = NetworkSessionGuard::new();
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
            player_name: player_name("Alice"),
        })
        .encode_packet(1, 0)
        {
            Ok(packet) => packet,
            Err(error) => panic!("packet should encode: {error}"),
        };

        assert_eq!(guard.accept_packet(&connect), Ok(()));
        assert!(!guard.is_bound());
        guard.mark_bound();
        assert!(guard.is_bound());

        let select_team = match (ClientControlCommand::SelectTeam {
            team: TeamSide::TeamA,
        })
        .encode_packet(2, 0)
        {
            Ok(packet) => packet,
            Err(error) => panic!("packet should encode: {error}"),
        };
        assert_eq!(guard.accept_packet(&select_team), Ok(()));

        let reconnect = match (ClientControlCommand::Connect {
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
        let guard = NetworkSessionGuard::new();
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
