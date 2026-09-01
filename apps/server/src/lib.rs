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
pub const PROTOCOL_VERSION: u16 = 4;
/// Old cached web clients without an explicit version are treated as this frozen baseline.
pub const LEGACY_DEFAULT_PROTOCOL_VERSION: u16 = 3;
/// Inclusive protocol window served during canary, rollback, and active-match drain.
pub const MIN_SUPPORTED_PROTOCOL_VERSION: u16 = 3;
pub const MAX_SUPPORTED_PROTOCOL_VERSION: u16 = PROTOCOL_VERSION;
