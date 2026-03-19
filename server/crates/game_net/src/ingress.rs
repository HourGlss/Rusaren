use crate::{ClientControlCommand, PacketError};

/// Absolute size ceiling for one inbound network packet before decode.
pub const MAX_INGRESS_PACKET_BYTES: usize = 1024;

/// Tracks whether a connection has completed its required initial `Connect` packet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NetworkSessionGuard {
    is_bound: bool,
}

impl NetworkSessionGuard {
    /// Creates a new unbound session guard.
    #[must_use]
    pub const fn new() -> Self {
        Self { is_bound: false }
    }

    // VERIFIED MODEL: server/verus/network_ingress_model.rs mirrors the two security
    // invariants enforced here:
    // 1. the first accepted packet must decode as Connect; and
    // 2. once bound, another Connect packet is always rejected.
    ///
    /// # Errors
    ///
    /// Returns a [`PacketError`] when the packet is too large, fails to decode,
    /// does not start with `Connect`, or attempts to re-bind an established session.
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

    /// Marks the session as bound after the transport associates it with a player.
    pub fn mark_bound(&mut self) {
        self.is_bound = true;
    }

    /// Returns whether the session has already completed its initial bind.
    #[must_use]
    pub const fn is_bound(self) -> bool {
        self.is_bound
    }
}

#[cfg(test)]
mod tests;
