use crate::error::PacketError;
use crate::packet_types::{ChannelId, PacketKind};
use crate::{HEADER_LEN, PACKET_MAGIC, PROTOCOL_VERSION};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PacketHeader {
    pub version: u8,
    pub channel_id: ChannelId,
    pub packet_kind: PacketKind,
    pub flags: u8,
    pub payload_len: u16,
    pub seq: u32,
    pub sim_tick: u32,
}

impl PacketHeader {
    /// # Errors
    ///
    /// Returns a [`PacketError`] when `packet_kind` does not belong to
    /// `channel_id`.
    pub fn new(
        channel_id: ChannelId,
        packet_kind: PacketKind,
        flags: u8,
        payload_len: u16,
        seq: u32,
        sim_tick: u32,
    ) -> Result<Self, PacketError> {
        let decoded_kind = PacketKind::from_byte(channel_id, packet_kind.to_byte())?;
        debug_assert_eq!(decoded_kind, packet_kind);

        Ok(Self {
            version: PROTOCOL_VERSION,
            channel_id,
            packet_kind,
            flags,
            payload_len,
            seq,
            sim_tick,
        })
    }

    #[must_use]
    pub fn encode(self, payload: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(HEADER_LEN + payload.len());
        bytes.extend_from_slice(&PACKET_MAGIC.to_le_bytes());
        bytes.push(self.version);
        bytes.push(self.channel_id.to_byte());
        bytes.push(self.packet_kind.to_byte());
        bytes.push(self.flags);
        bytes.extend_from_slice(&self.payload_len.to_le_bytes());
        bytes.extend_from_slice(&self.seq.to_le_bytes());
        bytes.extend_from_slice(&self.sim_tick.to_le_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    /// # Errors
    ///
    /// Returns a [`PacketError`] when the packet header is truncated, the magic
    /// bytes or version are wrong, the channel or packet kind is unknown, or the
    /// declared payload length does not match the received bytes.
    pub fn decode(packet: &[u8]) -> Result<(Self, &[u8]), PacketError> {
        if packet.len() < HEADER_LEN {
            return Err(PacketError::PacketTooShort {
                actual: packet.len(),
                minimum: HEADER_LEN,
            });
        }

        let magic = u16::from_le_bytes([packet[0], packet[1]]);
        if magic != PACKET_MAGIC {
            return Err(PacketError::MagicMismatch {
                expected: PACKET_MAGIC,
                actual: magic,
            });
        }

        let version = packet[2];
        if version != PROTOCOL_VERSION {
            return Err(PacketError::VersionMismatch {
                expected: PROTOCOL_VERSION,
                actual: version,
            });
        }

        let channel_id = ChannelId::from_byte(packet[3])?;
        let packet_kind = PacketKind::from_byte(channel_id, packet[4])?;
        let flags = packet[5];
        let payload_len = u16::from_le_bytes([packet[6], packet[7]]);
        let seq = u32::from_le_bytes([packet[8], packet[9], packet[10], packet[11]]);
        let sim_tick = u32::from_le_bytes([packet[12], packet[13], packet[14], packet[15]]);
        let payload = &packet[HEADER_LEN..];

        if payload.len() != usize::from(payload_len) {
            return Err(PacketError::PayloadLengthMismatch {
                declared: payload_len,
                actual: payload.len(),
            });
        }

        Ok((
            Self {
                version,
                channel_id,
                packet_kind,
                flags,
                payload_len,
                seq,
                sim_tick,
            },
            payload,
        ))
    }
}
