//! # `engine` — stable public API for Tauri and integration tests
//!
//! ## Canonical imports
//!
//! ```ignore
//! use engine::{
//!     download, fetch_metadata, get_or_create_secret, start_share_items,
//!     FileMetadata, ReceiveOptions, SendOptions, SendResult,
//! };
//! ```
//!
//! Desktop/mobile builds use the `native` platform crate (re-exported here).
//! Browser transfer logic lives in `wasm-io` and is reached via the root-level
//! `wasm-bridge` crate, not through this facade.
//!
//! Workspace layout: `protocol` (shared P2P logic) · `native` (disk I/O) · `wasm-io` (memory I/O).

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

#[cfg(target_arch = "wasm32")]
pub use wasm_io::*;

/// Shared protocol helpers not re-exported by the platform crates.
pub use protocol::{
    build_relay_mode, download_to_store, get_relay_status, relay_fallback_policy,
    resolve_relay_mode_with_fallback, run_share_session, verify_relays, DownloadToStoreResult,
    RelayConfigArg, RelayFallbackPolicy, RelayStatusResponse, ShareSessionOutcome,
    VerifyRelaysResponse, METADATA_ALPN,
};
