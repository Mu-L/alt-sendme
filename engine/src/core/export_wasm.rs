//! Browser export: iroh-blobs collection → in-memory bytes.

use anyhow::Context;
use iroh_blobs::{api::Store, format::collection::Collection};

/// Read a single-file collection back into memory.
pub async fn export_single_file_bytes(
    db: &Store,
    collection: Collection,
) -> anyhow::Result<(String, Vec<u8>)> {
    let mut iter = collection.iter();
    let (name, hash) = iter
        .next()
        .context("collection is empty")?;
    anyhow::ensure!(
        iter.next().is_none(),
        "wasm MVP supports single-file collections only"
    );

    let bytes = db.get_bytes(*hash).await?.to_vec();
    Ok((name.to_string(), bytes))
}
