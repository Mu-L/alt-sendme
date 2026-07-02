//! Platform-specific blob store creation.

#[cfg(not(target_arch = "wasm32"))]
use crate::core::types::AutoCleanupDir;

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use anyhow::Context;
    use data_encoding::HEXLOWER;
    use iroh_blobs::store::fs::FsStore;
    use rand::RngExt;
    use std::path::{Path, PathBuf};

    /// Create a unique temp directory for an outbound share session.
    pub fn new_send_blobs_dir() -> PathBuf {
        let suffix = rand::rng().random::<[u8; 16]>();
        std::env::temp_dir().join(format!(".sendme-send-{}", HEXLOWER.encode(&suffix)))
    }

    /// Load (or create) a filesystem blob store for sending.
    pub async fn create_send_store(dir: &Path) -> anyhow::Result<FsStore> {
        tokio::fs::create_dir_all(dir)
            .await
            .with_context(|| format!("failed to create send store dir {}", dir.display()))?;
        FsStore::load(dir)
            .await
            .with_context(|| format!("failed to load send store at {}", dir.display()))
    }

    /// Load (or create) a filesystem blob store for receiving.
    pub async fn create_recv_store(hash_hex: &str) -> anyhow::Result<(FsStore, PathBuf)> {
        let dir_name = format!(".sendme-recv-{}", hash_hex);
        let path = std::env::temp_dir().join(dir_name);
        let store = FsStore::load(&path)
            .await
            .with_context(|| format!("failed to load recv store at {}", path.display()))?;
        Ok((store, path))
    }

    /// Wrap a recv store path with automatic cleanup on drop.
    pub fn recv_cleanup_guard(path: PathBuf) -> AutoCleanupDir {
        AutoCleanupDir::new(path)
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

#[cfg(target_arch = "wasm32")]
mod wasm {
    use iroh_blobs::store::mem::MemStore;

    pub fn create_send_mem_store() -> MemStore {
        MemStore::default()
    }

    pub fn create_recv_mem_store() -> MemStore {
        MemStore::default()
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;
