//! Pairing session constants and helpers.

pub const PAIRING_VOTE_TIMEOUT_SECS: u64 = 120;

/// When `ttl_secs` is [`None`], the pairing host stays open until explicitly stopped
/// (e.g. during an active share session). `Some(n)` starts a timed window.
pub fn pairing_host_is_persistent(ttl_secs: Option<u64>) -> bool {
    ttl_secs.is_none()
}
