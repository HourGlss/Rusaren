//! Protocol, transport, and snapshot replication code.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::expect_used))]

use std::fmt;

use game_domain::DomainError;

mod control;
mod ingress;

pub use control::{
    ArenaDeltaSnapshot, ArenaEffectKind, ArenaEffectSnapshot, ArenaMatchPhase, ArenaObstacleKind,
    ArenaObstacleSnapshot, ArenaPlayerSnapshot, ArenaProjectileSnapshot, ArenaStateSnapshot,
    ArenaStatusKind, ArenaStatusSnapshot, ClientControlCommand, LobbyDirectoryEntry,
    LobbySnapshotPhase, LobbySnapshotPlayer, ServerControlEvent, SkillCatalogEntry,
};
pub use ingress::{NetworkSessionGuard, MAX_INGRESS_PACKET_BYTES};

pub const PACKET_MAGIC: u16 = 0x5241;
pub const PROTOCOL_VERSION: u8 = 2;
pub const HEADER_LEN: usize = 16;
pub const INPUT_PAYLOAD_LEN: usize = 16;
pub const INPUT_PAYLOAD_LEN_U16: u16 = 16;

pub const BUTTON_PRIMARY: u16 = 1 << 0;
pub const BUTTON_SECONDARY: u16 = 1 << 1;
pub const BUTTON_CAST: u16 = 1 << 2;
pub const BUTTON_CANCEL: u16 = 1 << 3;
pub const BUTTON_QUIT_TO_LOBBY: u16 = 1 << 4;
pub const ALLOWED_BUTTONS_MASK: u16 =
    BUTTON_PRIMARY | BUTTON_SECONDARY | BUTTON_CAST | BUTTON_CANCEL | BUTTON_QUIT_TO_LOBBY;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelId {
    Control,
    Input,
    Snapshot,
}

impl ChannelId {
    fn from_byte(raw: u8) -> Result<Self, PacketError> {
        match raw {
            0 => Ok(Self::Control),
            1 => Ok(Self::Input),
            2 => Ok(Self::Snapshot),
            _ => Err(PacketError::UnknownChannel(raw)),
        }
    }

    const fn to_byte(self) -> u8 {
        match self {
            Self::Control => 0,
            Self::Input => 1,
            Self::Snapshot => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PacketKind {
    ControlSnapshot,
    ControlDelta,
    LaunchCountdownStarted,
    MatchStarted,
    MatchAborted,
    MatchStatistics,
    ControlCommand,
    ControlEvent,
    InputFrame,
    FullSnapshot,
    DeltaSnapshot,
    EventBatch,
}

impl PacketKind {
    fn from_byte(channel: ChannelId, raw: u8) -> Result<Self, PacketError> {
        match (channel, raw) {
            (ChannelId::Control, 0) => Ok(Self::ControlSnapshot),
            (ChannelId::Control, 1) => Ok(Self::ControlDelta),
            (ChannelId::Control, 2) => Ok(Self::LaunchCountdownStarted),
            (ChannelId::Control, 3) => Ok(Self::MatchStarted),
            (ChannelId::Control, 4) => Ok(Self::MatchAborted),
            (ChannelId::Control, 5) => Ok(Self::MatchStatistics),
            (ChannelId::Control, 6) => Ok(Self::ControlCommand),
            (ChannelId::Control, 7) => Ok(Self::ControlEvent),
            (ChannelId::Input, 16) => Ok(Self::InputFrame),
            (ChannelId::Snapshot, 32) => Ok(Self::FullSnapshot),
            (ChannelId::Snapshot, 33) => Ok(Self::DeltaSnapshot),
            (ChannelId::Snapshot, 34) => Ok(Self::EventBatch),
            _ => Err(PacketError::UnknownPacketKind {
                channel,
                raw_kind: raw,
            }),
        }
    }

    const fn to_byte(self) -> u8 {
        match self {
            Self::ControlSnapshot => 0,
            Self::ControlDelta => 1,
            Self::LaunchCountdownStarted => 2,
            Self::MatchStarted => 3,
            Self::MatchAborted => 4,
            Self::MatchStatistics => 5,
            Self::ControlCommand => 6,
            Self::ControlEvent => 7,
            Self::InputFrame => 16,
            Self::FullSnapshot => 32,
            Self::DeltaSnapshot => 33,
            Self::EventBatch => 34,
        }
    }
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PacketError {
    PacketTooShort {
        actual: usize,
        minimum: usize,
    },
    MagicMismatch {
        expected: u16,
        actual: u16,
    },
    VersionMismatch {
        expected: u8,
        actual: u8,
    },
    UnknownChannel(u8),
    UnknownPacketKind {
        channel: ChannelId,
        raw_kind: u8,
    },
    PayloadLengthMismatch {
        declared: u16,
        actual: usize,
    },
    UnexpectedPacketKind {
        expected_channel: ChannelId,
        expected_kind: PacketKind,
        actual_channel: ChannelId,
        actual_kind: PacketKind,
    },
    InputPayloadLengthMismatch {
        expected: usize,
        actual: usize,
    },
    UnknownButtonBits {
        provided: u16,
        allowed_mask: u16,
    },
    MissingAbilityContext,
    UnexpectedAbilityContext(u16),
    ControlPayloadTooShort {
        kind: &'static str,
        expected: usize,
        actual: usize,
    },
    UnexpectedTrailingBytes {
        kind: &'static str,
        actual: usize,
    },
    UnknownControlCommand(u8),
    UnknownServerEvent(u8),
    InvalidEncodedPlayerId(u32),
    InvalidEncodedLobbyId(u32),
    InvalidEncodedMatchId(u32),
    InvalidEncodedRound(u8),
    InvalidEncodedTeam(u8),
    InvalidEncodedOptionalTeam(u8),
    InvalidEncodedReadyState(u8),
    InvalidEncodedSkillTree(u8),
    InvalidEncodedMatchOutcome(u8),
    InvalidEncodedLobbyPhase(u8),
    InvalidEncodedArenaMatchPhase(u8),
    InvalidEncodedArenaObstacleKind(u8),
    InvalidEncodedArenaEffectKind(u8),
    InvalidEncodedArenaStatusKind(u8),
    InvalidEncodedBoolean(u8),
    InvalidEncodedPlayerName(DomainError),
    InvalidUtf8String {
        field: &'static str,
    },
    StringLengthOutOfBounds {
        field: &'static str,
        len: usize,
        max: usize,
    },
    IngressPacketTooLarge {
        actual: usize,
        maximum: usize,
    },
    FirstPacketMustBeConnect,
    ConnectCommandAfterBinding,
    PayloadTooLarge {
        actual: usize,
        maximum: usize,
    },
    StaleSequence {
        incoming: u32,
        newest: u32,
    },
}

impl PacketError {
    fn fmt_protocol_error(&self, f: &mut fmt::Formatter<'_>) -> Option<fmt::Result> {
        match self {
            Self::PacketTooShort { actual, minimum } => Some(write!(
                f,
                "packet length {actual} is below the minimum header length {minimum}"
            )),
            Self::MagicMismatch { expected, actual } => Some(write!(
                f,
                "packet magic {actual:#06x} does not match expected {expected:#06x}"
            )),
            Self::VersionMismatch { expected, actual } => Some(write!(
                f,
                "protocol version {actual} does not match expected version {expected}"
            )),
            Self::UnknownChannel(raw) => Some(write!(f, "unknown channel id {raw}")),
            Self::UnknownPacketKind { channel, raw_kind } => {
                Some(write!(f, "unknown packet kind {raw_kind} on channel {channel:?}"))
            }
            Self::PayloadLengthMismatch { declared, actual } => Some(write!(
                f,
                "payload length declared {declared} but actual bytes were {actual}"
            )),
            Self::UnexpectedPacketKind {
                expected_channel,
                expected_kind,
                actual_channel,
                actual_kind,
            } => Some(write!(
                f,
                "expected {expected_channel:?}/{expected_kind:?} but received {actual_channel:?}/{actual_kind:?}"
            )),
            Self::PayloadTooLarge { actual, maximum } => Some(write!(
                f,
                "payload length {actual} exceeds maximum encodable {maximum}"
            )),
            _ => None,
        }
    }

    fn fmt_input_error(&self, f: &mut fmt::Formatter<'_>) -> Option<fmt::Result> {
        match self {
            Self::InputPayloadLengthMismatch { expected, actual } => Some(write!(
                f,
                "input payload length {actual} does not match expected length {expected}"
            )),
            Self::UnknownButtonBits {
                provided,
                allowed_mask,
            } => Some(write!(
                f,
                "button bits {provided:#06x} exceed allowed mask {allowed_mask:#06x}"
            )),
            Self::MissingAbilityContext => {
                Some(f.write_str("cast packets must provide a non-zero ability_or_context"))
            }
            Self::UnexpectedAbilityContext(value) => Some(write!(
                f,
                "ability_or_context must be zero when cast is not requested, got {value}"
            )),
            Self::StaleSequence { incoming, newest } => Some(write!(
                f,
                "incoming sequence {incoming} is not newer than {newest}"
            )),
            _ => None,
        }
    }

    fn fmt_control_error(&self, f: &mut fmt::Formatter<'_>) -> Option<fmt::Result> {
        match self {
            Self::ControlPayloadTooShort {
                kind,
                expected,
                actual,
            } => Some(write!(
                f,
                "{kind} payload expected at least {expected} bytes but received {actual}"
            )),
            Self::UnexpectedTrailingBytes { kind, actual } => Some(write!(
                f,
                "{kind} payload contained {actual} unexpected trailing bytes"
            )),
            Self::UnknownControlCommand(raw) => Some(write!(f, "unknown control command {raw}")),
            Self::UnknownServerEvent(raw) => Some(write!(f, "unknown server event {raw}")),
            Self::InvalidUtf8String { field } => Some(write!(f, "{field} contained invalid utf-8")),
            Self::StringLengthOutOfBounds { field, len, max } => {
                Some(write!(f, "{field} length {len} exceeds maximum {max}"))
            }
            _ => None,
        }
    }

    fn fmt_encoded_value_error(&self, f: &mut fmt::Formatter<'_>) -> Option<fmt::Result> {
        match self {
            Self::InvalidEncodedPlayerId(raw) => {
                Some(write!(f, "encoded player id {raw} is invalid"))
            }
            Self::InvalidEncodedLobbyId(raw) => {
                Some(write!(f, "encoded lobby id {raw} is invalid"))
            }
            Self::InvalidEncodedMatchId(raw) => {
                Some(write!(f, "encoded match id {raw} is invalid"))
            }
            Self::InvalidEncodedRound(raw) => Some(write!(f, "encoded round {raw} is invalid")),
            Self::InvalidEncodedTeam(raw) => Some(write!(f, "encoded team {raw} is invalid")),
            Self::InvalidEncodedOptionalTeam(raw) => {
                Some(write!(f, "encoded optional team {raw} is invalid"))
            }
            Self::InvalidEncodedReadyState(raw) => {
                Some(write!(f, "encoded ready state {raw} is invalid"))
            }
            Self::InvalidEncodedSkillTree(raw) => {
                Some(write!(f, "encoded skill tree {raw} is invalid"))
            }
            Self::InvalidEncodedMatchOutcome(raw) => {
                Some(write!(f, "encoded match outcome {raw} is invalid"))
            }
            Self::InvalidEncodedLobbyPhase(raw) => {
                Some(write!(f, "encoded lobby phase {raw} is invalid"))
            }
            Self::InvalidEncodedArenaMatchPhase(raw) => {
                Some(write!(f, "encoded arena match phase {raw} is invalid"))
            }
            Self::InvalidEncodedArenaObstacleKind(raw) => {
                Some(write!(f, "encoded arena obstacle kind {raw} is invalid"))
            }
            Self::InvalidEncodedArenaEffectKind(raw) => {
                Some(write!(f, "encoded arena effect kind {raw} is invalid"))
            }
            Self::InvalidEncodedArenaStatusKind(raw) => {
                Some(write!(f, "encoded arena status kind {raw} is invalid"))
            }
            Self::InvalidEncodedBoolean(raw) => Some(write!(f, "encoded boolean {raw} is invalid")),
            Self::InvalidEncodedPlayerName(error) => Some(fmt::Display::fmt(error, f)),
            _ => None,
        }
    }

    fn fmt_ingress_error(&self, f: &mut fmt::Formatter<'_>) -> Option<fmt::Result> {
        match self {
            Self::IngressPacketTooLarge { actual, maximum } => Some(write!(
                f,
                "ingress packet length {actual} exceeds maximum {maximum}"
            )),
            Self::FirstPacketMustBeConnect => {
                Some(f.write_str("the first packet on a network session must be a connect command"))
            }
            Self::ConnectCommandAfterBinding => Some(
                f.write_str("connect commands are not accepted after a network session is bound"),
            ),
            _ => None,
        }
    }
}

impl fmt::Display for PacketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(result) = self.fmt_protocol_error(f) {
            return result;
        }
        if let Some(result) = self.fmt_input_error(f) {
            return result;
        }
        if let Some(result) = self.fmt_control_error(f) {
            return result;
        }
        if let Some(result) = self.fmt_encoded_value_error(f) {
            return result;
        }
        if let Some(result) = self.fmt_ingress_error(f) {
            return result;
        }

        Err(fmt::Error)
    }
}

impl std::error::Error for PacketError {}
