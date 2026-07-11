//! Temporary verbose tracing for the desktop pairing feature.
//! Grep terminal output with `pairing-dev`. Safe to delete when pairing stabilizes.

use iroh::EndpointAddr;
use std::time::{Duration, Instant};

#[macro_export]
macro_rules! pairing_dev {
    ($step:literal $(, $($rest:tt)*)?) => {
        tracing::info!(target: "pairing-dev", step = $step $(, $($rest)*)?, "pairing-dev")
    };
}

#[macro_export]
macro_rules! pairing_dev_warn {
    ($step:literal $(, $($rest:tt)*)?) => {
        tracing::warn!(target: "pairing-dev", step = $step $(, $($rest)*)?, "pairing-dev")
    };
}

/// Summarise an [`EndpointAddr`] for logs (relay URLs, IP addrs, total hint count).
pub fn format_connect_addr(addr: &EndpointAddr) -> String {
    use iroh::TransportAddr;
    let mut relays = Vec::new();
    let mut ips = Vec::new();
    for a in &addr.addrs {
        match a {
            TransportAddr::Relay(url) => relays.push(url.to_string()),
            TransportAddr::Ip(socket) => ips.push(socket.to_string()),
            other => ips.push(format!("{other:?}")),
        }
    }
    format!(
        "endpoint={} relay_hints={relays:?} ip_hints={ips:?} hint_count={}",
        addr.id,
        addr.addrs.len()
    )
}

pub fn format_duration(d: Duration) -> String {
    if d.as_secs() >= 1 {
        format!("{:.2}s", d.as_secs_f64())
    } else {
        format!("{}ms", d.as_millis())
    }
}

pub fn elapsed_ms(start: Instant) -> u128 {
    start.elapsed().as_millis()
}

/// Log an error with full display chain (use `{err:#}` formatting).
pub fn log_pairing_error(step: &str, err: &(impl std::fmt::Display + std::fmt::Debug)) {
    tracing::warn!(
        target: "pairing-dev",
        step,
        error = %err,
        error_detail = %format!("{err:#}"),
        debug = ?err,
        "pairing-dev"
    );
}
