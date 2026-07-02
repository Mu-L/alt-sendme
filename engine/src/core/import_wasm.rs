//! Browser import: in-memory bytes → iroh-blobs store.

use anyhow::Context;
use iroh_blobs::api::blobs::AddProgressItem;
use iroh_blobs::{
    api::{Store, TempTag},
    format::collection::Collection,
};
use n0_future::StreamExt;

/// Import a single file from raw bytes (wrapped as a one-entry collection).
pub async fn import_single_file_bytes(
    file_name: String,
    bytes: Vec<u8>,
    db: &Store,
) -> anyhow::Result<(TempTag, u64)> {
    let mut stream = db.blobs().add_bytes(bytes).stream().await;
    let mut item_size = 0u64;
    let file_tag = loop {
        let item = stream
            .next()
            .await
            .context("import stream ended without a tag")?;
        match item {
            AddProgressItem::Size(size) => item_size = size,
            AddProgressItem::Done(tt) => break tt,
            AddProgressItem::Error(cause) => {
                anyhow::bail!("error importing bytes: {cause}")
            }
            _ => {}
        }
    };

    let collection = Collection::from_iter([(file_name, file_tag.hash())]);
    let temp_tag = collection.clone().store(db).await?;
    Ok((temp_tag, item_size))
}
