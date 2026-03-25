//! Protocol, transport, and snapshot replication code.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::expect_used))]

mod control;
mod error;
mod header;
mod ingress;
mod input;
mod packet_types;

pub use control::{
    ArenaDeltaSnapshot, ArenaEffectKind, ArenaEffectSnapshot, ArenaMatchPhase, ArenaObstacleKind,
    ArenaObstacleSnapshot, ArenaPlayerSnapshot, ArenaProjectileSnapshot, ArenaStateSnapshot,
    ArenaStatusKind, ArenaStatusSnapshot, ClientControlCommand, LobbyDirectoryEntry,
    LobbySnapshotPhase, LobbySnapshotPlayer, ServerControlEvent, SkillCatalogEntry,
};
pub use error::PacketError;
pub use header::PacketHeader;
pub use ingress::{NetworkSessionGuard, MAX_INGRESS_PACKET_BYTES};
pub use input::{SequenceTracker, ValidatedInputFrame};
pub use packet_types::{ChannelId, PacketKind};

pub const PACKET_MAGIC: u16 = 0x5241;
pub const PROTOCOL_VERSION: u8 = 3;
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
