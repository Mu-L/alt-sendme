use std::path::{Path, PathBuf};
use std::sync::RwLock;

use anyhow::Context;
use data_encoding::HEXLOWER;
use iroh::SecretKey;
use protocol::{
    default_device_type, default_display_name, detect_os, normalize_display_name, DeviceMetaFile,
    PairedDevice, PairedDeviceList,
};
use serde::{Deserialize, Serialize};

use crate::secret_store;

#[derive(Debug)]
pub struct DeviceIdentity {
    pub secret_key: SecretKey,
    meta: RwLock<DeviceMetaFile>,
    meta_path: PathBuf,
}

impl DeviceIdentity {
    pub fn endpoint_id(&self) -> String {
        self.meta.read().expect("device meta lock").endpoint_id.clone()
    }

    pub fn display_name(&self) -> String {
        self.meta
            .read()
            .expect("device meta lock")
            .display_name
            .clone()
    }

    pub fn device_type(&self) -> String {
        self.meta
            .read()
            .expect("device meta lock")
            .device_type
            .clone()
    }

    pub fn os(&self) -> String {
        self.meta.read().expect("device meta lock").os.clone()
    }

    pub fn set_display_name(&self, name: &str) -> anyhow::Result<DeviceInfo> {
        let normalized = normalize_display_name(name).map_err(anyhow::Error::msg)?;
        {
            let mut meta = self.meta.write().expect("device meta lock");
            meta.display_name = normalized;
            meta.name_is_custom = true;
            meta.migrate();
            Self::write_meta(&self.meta_path, &meta)?;
        }
        Ok(DeviceInfo::from(self))
    }
}

pub struct PairedDeviceStore {
    path: PathBuf,
}

impl PairedDeviceStore {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join("paired-devices.json"),
        }
    }

    pub fn list(&self) -> anyhow::Result<Vec<PairedDevice>> {
        let file = self.read_file()?;
        Ok(file.devices)
    }

    pub fn get(&self, endpoint_id: &str) -> anyhow::Result<Option<PairedDevice>> {
        let id = endpoint_id.to_lowercase();
        Ok(self
            .list()?
            .into_iter()
            .find(|d| d.endpoint_id.to_lowercase() == id))
    }

    pub fn remember(&self, device: PairedDevice) -> anyhow::Result<PairedDevice> {
        tracing::info!(
            target: "pairing-dev",
            step = "store.remember.start",
            endpoint_id = %device.endpoint_id,
            display_name = %device.display_name,
            device_type = %device.device_type,
            os = %device.os,
            relay_url = ?device.relay_url,
            "pairing-dev"
        );
        let mut file = self.read_file()?;
        let id = device.endpoint_id.to_lowercase();
        if let Some(existing) = file
            .devices
            .iter_mut()
            .find(|d| d.endpoint_id.to_lowercase() == id)
        {
            existing.display_name = device.display_name;
            existing.device_type = device.device_type;
            existing.os = device.os;
            existing.last_seen_at = device.last_seen_at;
            if device.relay_url.is_some() {
                existing.relay_url = device.relay_url;
            }
            let saved = existing.clone();
            self.write_file(&file)?;
            tracing::info!(
                target: "pairing-dev",
                step = "store.remember.updated",
                endpoint_id = %saved.endpoint_id,
                "pairing-dev"
            );
            return Ok(saved);
        }
        file.devices.push(device.clone());
        self.write_file(&file)?;
        tracing::info!(
            target: "pairing-dev",
            step = "store.remember.inserted",
            endpoint_id = %device.endpoint_id,
            total_devices = file.devices.len(),
            "pairing-dev"
        );
        Ok(device)
    }

    pub fn rename(&self, endpoint_id: &str, display_name: &str) -> anyhow::Result<PairedDevice> {
        let normalized = normalize_display_name(display_name).map_err(anyhow::Error::msg)?;
        let mut file = self.read_file()?;
        let id = endpoint_id.to_lowercase();
        let existing = file
            .devices
            .iter_mut()
            .find(|d| d.endpoint_id.to_lowercase() == id)
            .context("paired device not found")?;
        existing.display_name = normalized;
        let saved = existing.clone();
        self.write_file(&file)?;
        Ok(saved)
    }

    pub fn forget(&self, endpoint_id: &str) -> anyhow::Result<()> {
        tracing::info!(
            target: "pairing-dev",
            step = "store.forget",
            endpoint_id = %endpoint_id,
            "pairing-dev"
        );
        let mut file = self.read_file()?;
        let id = endpoint_id.to_lowercase();
        file.devices
            .retain(|d| d.endpoint_id.to_lowercase() != id);
        self.write_file(&file)?;
        Ok(())
    }

    pub fn touch(&self, endpoint_id: &str, last_seen_at: u64) -> anyhow::Result<()> {
        tracing::info!(
            target: "pairing-dev",
            step = "store.touch",
            endpoint_id = %endpoint_id,
            last_seen_at,
            "pairing-dev"
        );
        let mut file = self.read_file()?;
        let id = endpoint_id.to_lowercase();
        if let Some(existing) = file
            .devices
            .iter_mut()
            .find(|d| d.endpoint_id.to_lowercase() == id)
        {
            existing.last_seen_at = last_seen_at;
            self.write_file(&file)?;
        }
        Ok(())
    }

    fn read_file(&self) -> anyhow::Result<PairedDeviceList> {
        if !self.path.exists() {
            return Ok(PairedDeviceList::default());
        }
        let raw = std::fs::read_to_string(&self.path)
            .with_context(|| format!("read {}", self.path.display()))?;
        Ok(serde_json::from_str(&raw).unwrap_or_default())
    }

    fn write_file(&self, list: &PairedDeviceList) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(list)?)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

impl DeviceIdentity {
    fn write_meta(path: &Path, meta: &DeviceMetaFile) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(meta)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

pub fn load_or_create_identity(data_dir: &Path) -> anyhow::Result<DeviceIdentity> {
    std::fs::create_dir_all(data_dir)?;
    let meta_path = data_dir.join("device.json");

    let secret = load_secret_key()?;
    let endpoint_id = HEXLOWER.encode(secret.public().as_bytes());

    let mut meta = if meta_path.exists() {
        let raw = std::fs::read_to_string(&meta_path)?;
        let mut meta: DeviceMetaFile =
            serde_json::from_str(&raw).context("invalid device.json")?;
        if meta.endpoint_id.to_lowercase() != endpoint_id {
            tracing::warn!("device.json endpoint_id mismatch; updating to keychain identity");
            tracing::info!(
                target: "pairing-dev",
                step = "identity.endpoint_mismatch",
                old = %meta.endpoint_id,
                new = %endpoint_id,
                "pairing-dev"
            );
            meta.endpoint_id = endpoint_id;
        }
        meta.migrate();
        // Refresh auto-detected hostname only when the user has not renamed.
        if !meta.name_is_custom {
            let detected = default_display_name();
            if meta.display_name.trim().is_empty()
                || meta.display_name.eq_ignore_ascii_case("altsendme")
            {
                meta.display_name = detected;
            }
        }
        if meta.os.trim().is_empty() {
            meta.os = detect_os();
        }
        if meta.device_type.trim().is_empty() {
            meta.device_type = default_device_type();
        }
        meta
    } else {
        DeviceMetaFile::new(endpoint_id, default_display_name(), default_device_type())
    };

    meta.migrate();
    DeviceIdentity::write_meta(&meta_path, &meta)?;

    Ok(DeviceIdentity {
        secret_key: secret,
        meta: RwLock::new(meta),
        meta_path,
    })
}

fn load_secret_key() -> anyhow::Result<SecretKey> {
    if let Ok(hex) = std::env::var("IROH_SECRET") {
        return parse_secret_hex(&hex);
    }
    if let Some(hex) = secret_store::load_secret_hex()? {
        return parse_secret_hex(&hex);
    }
    let secret = SecretKey::generate();
    let hex = HEXLOWER.encode(&secret.to_bytes());
    secret_store::save_secret_hex(&hex)?;
    Ok(secret)
}

fn parse_secret_hex(hex: &str) -> anyhow::Result<SecretKey> {
    let bytes = HEXLOWER
        .decode(hex.trim().as_bytes())
        .context("invalid IROH_SECRET hex")?;
    anyhow::ensure!(bytes.len() == 32, "secret key must be 32 bytes");
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(SecretKey::from_bytes(&arr))
}

/// Serializable device info for the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub endpoint_id: String,
    pub display_name: String,
    pub device_type: String,
    pub os: String,
}

impl From<&DeviceIdentity> for DeviceInfo {
    fn from(id: &DeviceIdentity) -> Self {
        Self {
            endpoint_id: id.endpoint_id(),
            display_name: id.display_name(),
            device_type: id.device_type(),
            os: id.os(),
        }
    }
}
