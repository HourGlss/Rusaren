//! HTTP and service-facing orchestration for the arena server.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::expect_used))]

mod app;
mod observability;
mod realtime;
mod records;
mod transport;

pub use app::{AppError, ServerApp};
pub use observability::{classify_http_path, HttpRouteLabel, ServerObservability};
pub use realtime::{
    spawn_dev_server, spawn_dev_server_with_options, DevServerHandle, DevServerOptions,
};
pub use records::{
    canonicalize_record_store_contents, PlayerRecordStore, RecordStoreError, MAX_RECORD_STORE_BYTES,
};
pub use transport::{AppTransport, ConnectionId, HeadlessClient, InMemoryTransport};
