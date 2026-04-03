use std::fmt;

use game_domain::DomainError;

use crate::packet_types::{ChannelId, PacketKind};

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
    SelfCastWithoutCast,
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
    InvalidEncodedSkillTree(game_domain::DomainError),
    InvalidEncodedMatchOutcome(u8),
    InvalidEncodedLobbyPhase(u8),
    InvalidEncodedArenaMatchPhase(u8),
    InvalidEncodedArenaObstacleKind(u8),
    InvalidEncodedArenaEffectKind(u8),
    InvalidEncodedArenaCombatTextStyle(u8),
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
            Self::SelfCastWithoutCast => {
                Some(f.write_str("self-cast requires cast to be requested"))
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
            Self::InvalidEncodedPlayerId(_)
            | Self::InvalidEncodedLobbyId(_)
            | Self::InvalidEncodedMatchId(_)
            | Self::InvalidEncodedRound(_) => self.fmt_encoded_identifier_error(f),
            Self::InvalidEncodedTeam(_)
            | Self::InvalidEncodedOptionalTeam(_)
            | Self::InvalidEncodedReadyState(_)
            | Self::InvalidEncodedMatchOutcome(_)
            | Self::InvalidEncodedLobbyPhase(_)
            | Self::InvalidEncodedArenaMatchPhase(_)
            | Self::InvalidEncodedArenaObstacleKind(_)
            | Self::InvalidEncodedArenaEffectKind(_)
            | Self::InvalidEncodedArenaCombatTextStyle(_)
            | Self::InvalidEncodedArenaStatusKind(_)
            | Self::InvalidEncodedBoolean(_) => self.fmt_encoded_variant_error(f),
            Self::InvalidEncodedSkillTree(error) | Self::InvalidEncodedPlayerName(error) => {
                Some(fmt::Display::fmt(error, f))
            }
            _ => None,
        }
    }

    fn fmt_encoded_identifier_error(&self, f: &mut fmt::Formatter<'_>) -> Option<fmt::Result> {
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
            _ => None,
        }
    }

    fn fmt_encoded_variant_error(&self, f: &mut fmt::Formatter<'_>) -> Option<fmt::Result> {
        match self {
            Self::InvalidEncodedTeam(raw) => Some(write!(f, "encoded team {raw} is invalid")),
            Self::InvalidEncodedOptionalTeam(raw) => {
                Some(write!(f, "encoded optional team {raw} is invalid"))
            }
            Self::InvalidEncodedReadyState(raw) => {
                Some(write!(f, "encoded ready state {raw} is invalid"))
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
            Self::InvalidEncodedArenaCombatTextStyle(raw) => Some(write!(
                f,
                "encoded arena combat text style {raw} is invalid"
            )),
            Self::InvalidEncodedArenaStatusKind(raw) => {
                Some(write!(f, "encoded arena status kind {raw} is invalid"))
            }
            Self::InvalidEncodedBoolean(raw) => Some(write!(f, "encoded boolean {raw} is invalid")),
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
