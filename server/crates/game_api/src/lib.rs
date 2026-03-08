//! HTTP and service-facing orchestration for the arena server.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::expect_used))]

mod app;
mod transport;

pub use app::{AppError, ServerApp};
pub use transport::{AppTransport, HeadlessClient, InMemoryTransport};
