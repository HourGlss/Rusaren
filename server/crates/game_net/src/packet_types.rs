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
