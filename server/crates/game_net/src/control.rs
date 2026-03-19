use game_domain::MAX_SKILL_TREE_NAME_LEN;

mod client;
mod codec;
mod server_decode;
mod server_encode;
mod server_types;
mod snapshots_decode;
mod snapshots_encode;

pub use client::ClientControlCommand;
pub use server_types::*;

const MAX_MESSAGE_BYTES: usize = 200;
const MAX_SKILL_TREE_NAME_BYTES: usize = MAX_SKILL_TREE_NAME_LEN;
const MAX_SKILL_ID_BYTES: usize = 64;
const MAX_SKILL_NAME_BYTES: usize = 120;
