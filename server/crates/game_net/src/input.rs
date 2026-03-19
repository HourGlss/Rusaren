use crate::error::PacketError;
use crate::header::PacketHeader;
use crate::packet_types::{ChannelId, PacketKind};
use crate::{ALLOWED_BUTTONS_MASK, BUTTON_CAST, INPUT_PAYLOAD_LEN, INPUT_PAYLOAD_LEN_U16};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidatedInputFrame {
    pub client_input_tick: u32,
    pub move_horizontal_q: i16,
    pub move_vertical_q: i16,
    pub aim_horizontal_q: i16,
    pub aim_vertical_q: i16,
    pub buttons: u16,
    pub ability_or_context: u16,
}

impl ValidatedInputFrame {
    /// # Errors
    ///
    /// Returns a [`PacketError`] when unknown button bits are present, when cast
    /// input omits ability context, or when non-cast input includes ability
    /// context.
    pub fn new(
        client_input_tick: u32,
        move_horizontal_q: i16,
        move_vertical_q: i16,
        aim_horizontal_q: i16,
        aim_vertical_q: i16,
        buttons: u16,
        ability_or_context: u16,
    ) -> Result<Self, PacketError> {
        if buttons & !ALLOWED_BUTTONS_MASK != 0 {
            return Err(PacketError::UnknownButtonBits {
                provided: buttons,
                allowed_mask: ALLOWED_BUTTONS_MASK,
            });
        }

        let cast_requested = buttons & BUTTON_CAST != 0;
        match (cast_requested, ability_or_context) {
            (true, 0) => return Err(PacketError::MissingAbilityContext),
            (false, non_zero) if non_zero != 0 => {
                return Err(PacketError::UnexpectedAbilityContext(non_zero))
            }
            _ => {}
        }

        Ok(Self {
            client_input_tick,
            move_horizontal_q,
            move_vertical_q,
            aim_horizontal_q,
            aim_vertical_q,
            buttons,
            ability_or_context,
        })
    }

    /// # Errors
    ///
    /// Returns a [`PacketError`] when header construction fails.
    pub fn encode_packet(self, seq: u32, sim_tick: u32) -> Result<Vec<u8>, PacketError> {
        let mut payload = Vec::with_capacity(INPUT_PAYLOAD_LEN);
        payload.extend_from_slice(&self.client_input_tick.to_le_bytes());
        payload.extend_from_slice(&self.move_horizontal_q.to_le_bytes());
        payload.extend_from_slice(&self.move_vertical_q.to_le_bytes());
        payload.extend_from_slice(&self.aim_horizontal_q.to_le_bytes());
        payload.extend_from_slice(&self.aim_vertical_q.to_le_bytes());
        payload.extend_from_slice(&self.buttons.to_le_bytes());
        payload.extend_from_slice(&self.ability_or_context.to_le_bytes());

        let header = PacketHeader::new(
            ChannelId::Input,
            PacketKind::InputFrame,
            0,
            INPUT_PAYLOAD_LEN_U16,
            seq,
            sim_tick,
        )?;

        Ok(header.encode(&payload))
    }

    /// # Errors
    ///
    /// Returns a [`PacketError`] when the packet header is malformed, the packet
    /// is not an input-frame packet, or the input payload fails validation.
    pub fn decode_packet(packet: &[u8]) -> Result<(PacketHeader, Self), PacketError> {
        let (header, payload) = PacketHeader::decode(packet)?;
        if header.channel_id != ChannelId::Input || header.packet_kind != PacketKind::InputFrame {
            return Err(PacketError::UnexpectedPacketKind {
                expected_channel: ChannelId::Input,
                expected_kind: PacketKind::InputFrame,
                actual_channel: header.channel_id,
                actual_kind: header.packet_kind,
            });
        }

        if payload.len() != INPUT_PAYLOAD_LEN {
            return Err(PacketError::InputPayloadLengthMismatch {
                expected: INPUT_PAYLOAD_LEN,
                actual: payload.len(),
            });
        }

        let client_input_tick =
            u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
        let move_horizontal_q = i16::from_le_bytes([payload[4], payload[5]]);
        let move_vertical_q = i16::from_le_bytes([payload[6], payload[7]]);
        let aim_horizontal_q = i16::from_le_bytes([payload[8], payload[9]]);
        let aim_vertical_q = i16::from_le_bytes([payload[10], payload[11]]);
        let buttons = u16::from_le_bytes([payload[12], payload[13]]);
        let ability_or_context = u16::from_le_bytes([payload[14], payload[15]]);

        let frame = Self::new(
            client_input_tick,
            move_horizontal_q,
            move_vertical_q,
            aim_horizontal_q,
            aim_vertical_q,
            buttons,
            ability_or_context,
        )?;

        Ok((header, frame))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SequenceTracker {
    newest_seq: Option<u32>,
}

impl SequenceTracker {
    #[must_use]
    pub const fn new() -> Self {
        Self { newest_seq: None }
    }

    /// # Errors
    ///
    /// Returns [`PacketError::StaleSequence`] when `seq` is not newer than the
    /// newest observed sequence.
    pub fn observe(&mut self, seq: u32) -> Result<(), PacketError> {
        if let Some(newest_seq) = self.newest_seq {
            if seq <= newest_seq {
                return Err(PacketError::StaleSequence {
                    incoming: seq,
                    newest: newest_seq,
                });
            }
        }

        self.newest_seq = Some(seq);
        Ok(())
    }

    #[must_use]
    pub const fn newest(self) -> Option<u32> {
        self.newest_seq
    }
}
