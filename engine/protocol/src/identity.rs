use serde::{Deserialize, Serialize};

/// Public device metadata persisted on disk (no secret key).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceMetaFile {
    pub version: u32,
    pub endpoint_id: String,
    pub display_name: String,
    /// Form factor: `laptop` | `desktop` | `phone` | `tablet` | `unknown`.
    pub device_type: String,
    /// OS family: `macos` | `windows` | `linux` | `ios` | `android` | …
    #[serde(default)]
    pub os: String,
    pub created_at: u64,
    /// True once the user has set a custom display name.
    #[serde(default)]
    pub name_is_custom: bool,
}

impl DeviceMetaFile {
    pub const VERSION: u32 = 2;
    pub const MAX_DISPLAY_NAME_CHARS: usize = 64;

    pub fn new(endpoint_id: String, display_name: String, device_type: String) -> Self {
        Self {
            version: Self::VERSION,
            endpoint_id,
            display_name,
            device_type,
            os: detect_os(),
            created_at: unix_now_ms(),
            name_is_custom: false,
        }
    }

    /// Fill missing fields on older `device.json` files.
    pub fn migrate(&mut self) {
        if self.os.trim().is_empty() {
            self.os = detect_os();
        }
        if self.device_type.trim().is_empty() {
            self.device_type = default_device_type();
        }
        if self.version < Self::VERSION {
            self.version = Self::VERSION;
        }
    }
}

/// Pairing relationship status for a stored remote device.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum PairingStatus {
    #[default]
    Active,
    UnpairedRemotely,
    /// Local endpoint identity changed; peers still have the previous endpoint id.
    StaleLocalIdentity,
}

impl PairingStatus {
    pub fn is_active(self) -> bool {
        matches!(self, Self::Active)
    }

    /// Devices that should keep outbound presence loops and allowlist entries.
    pub fn is_connectable(self) -> bool {
        matches!(self, Self::Active | Self::StaleLocalIdentity)
    }
}

/// Paired remote device record (persisted).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PairedDevice {
    pub endpoint_id: String,
    pub display_name: String,
    pub device_type: String,
    #[serde(default)]
    pub os: String,
    pub paired_at: u64,
    pub last_seen_at: u64,
    /// Relay URL last known for this peer (from pairing ticket or discovery).
    #[serde(default)]
    pub relay_url: Option<String>,
    #[serde(default)]
    pub pairing_status: PairingStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PairedDeviceList {
    pub devices: Vec<PairedDevice>,
}

pub fn unix_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn detect_os() -> String {
    match std::env::consts::OS {
        "macos" => "macos".to_string(),
        "windows" => "windows".to_string(),
        "linux" => "linux".to_string(),
        "ios" => "ios".to_string(),
        "android" => "android".to_string(),
        other => other.to_string(),
    }
}

pub fn default_display_name() -> String {
    let raw = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "AltSendme Device".to_string());
    let trimmed = raw.trim_end_matches(".local").trim();
    if trimmed.is_empty() {
        "AltSendme Device".to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn default_device_type() -> String {
    if cfg!(any(target_os = "ios", target_os = "android")) {
        "phone".to_string()
    } else if cfg!(target_os = "macos") {
        // Most Macs running this desktop app are laptops; desktop Macs still
        // read clearly as "Mac" via the `os` field.
        "laptop".to_string()
    } else {
        "desktop".to_string()
    }
}

pub fn normalize_display_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Device name cannot be empty".to_string());
    }
    if trimmed.chars().count() > DeviceMetaFile::MAX_DISPLAY_NAME_CHARS {
        return Err(format!(
            "Device name must be at most {} characters",
            DeviceMetaFile::MAX_DISPLAY_NAME_CHARS
        ));
    }
    Ok(trimmed.to_string())
}
