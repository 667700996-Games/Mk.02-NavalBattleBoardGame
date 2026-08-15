pub mod api;
pub mod app;
pub mod config;
pub mod domain;
pub mod error;
pub mod protocol;
pub mod rate_limit;
pub mod store;
pub mod ws;

pub use app::{AppState, build_router};

/// Increment when the public HTTP/WebSocket snapshot contract is not backward compatible.
pub const PROTOCOL_VERSION: u16 = 2;
