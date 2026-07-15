//! Portable ZIP distribution detection for Windows.
//!
//! The CI packaging step drops a `.portable` marker next to the executable
//! inside the zip. Installed NSIS/MSI builds never include that file, so this
//! is a reliable way to disable installer-style auto-updates for zip users.

#[cfg(target_os = "windows")]
use std::path::PathBuf;

/// Sentinel file placed beside the portable executable.
#[cfg(target_os = "windows")]
pub const PORTABLE_MARKER: &str = ".portable";

#[cfg(target_os = "windows")]
fn exe_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let canonical = dunce::canonicalize(&exe).unwrap_or(exe);
    canonical.parent().map(PathBuf::from)
}

/// Returns true when this process is running from the Windows portable ZIP.
#[cfg(target_os = "windows")]
pub fn is_portable() -> bool {
    exe_dir()
        .map(|dir| dir.join(PORTABLE_MARKER).is_file())
        .unwrap_or(false)
}

#[cfg(not(target_os = "windows"))]
pub fn is_portable() -> bool {
    false
}
