use crate::error::PacketError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelId {
    Control,
    Input,
    Snapshot,
}

impl ChannelId {
    pub(crate) fn from_byte(raw: u8) -> Result<Self, PacketError> {
        match raw {
            0 => Ok(Self::Control),
            1 => Ok(Self::Input),
            2 => Ok(Self::Snapshot),
            _ => Err(PacketError::UnknownChannel(raw)),
        }
    }

    pub(crate) const fn to_byte(self) -> u8 {
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
    pub(crate) fn from_byte(channel: ChannelId, raw: u8) -> Result<Self, PacketError> {
        let kind = match channel {
            ChannelId::Control => Self::control_from_byte(raw),
            ChannelId::Input => Self::input_from_byte(raw),
            ChannelId::Snapshot => Self::snapshot_from_byte(raw),
        };

        kind.ok_or(PacketError::UnknownPacketKind {
            channel,
            raw_kind: raw,
        })
    }

    const fn control_from_byte(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::ControlSnapshot),
            1 => Some(Self::ControlDelta),
            2 => Some(Self::LaunchCountdownStarted),
            3 => Some(Self::MatchStarted),
            4 => Some(Self::MatchAborted),
            5 => Some(Self::MatchStatistics),
            6 => Some(Self::ControlCommand),
            7 => Some(Self::ControlEvent),
            _ => None,
        }
    }

    const fn input_from_byte(raw: u8) -> Option<Self> {
        match raw {
            16 => Some(Self::InputFrame),
            _ => None,
        }
    }

    const fn snapshot_from_byte(raw: u8) -> Option<Self> {
        match raw {
            32 => Some(Self::FullSnapshot),
            33 => Some(Self::DeltaSnapshot),
            34 => Some(Self::EventBatch),
            _ => None,
        }
    }

    pub(crate) const fn to_byte(self) -> u8 {
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
