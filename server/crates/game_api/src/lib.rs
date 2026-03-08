//! HTTP and service-facing orchestration for the arena server.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::expect_used))]

mod app;
mod realtime;
mod transport;

pub use app::{AppError, ServerApp};
pub use realtime::{spawn_dev_server, DevServerHandle};
pub use transport::{AppTransport, HeadlessClient, InMemoryTransport};
