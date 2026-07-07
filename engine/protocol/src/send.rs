use crate::time_compat::{sleep, timeout, Duration, Instant};
use crate::types::{apply_options, AddrInfoOptions, AppHandle, FileMetadata};
use iroh::protocol::{AcceptError, ProtocolHandler};
use iroh::{endpoint::RelayMode, Endpoint};
use iroh_blobs::{
    api::TempTag,
    provider::events::ProviderMessage,
    ticket::BlobTicket,
    BlobFormat, BlobsProtocol,
};
use n0_future::{task::AbortOnDropHandle, StreamExt};
use std::io::ErrorKind;
use std::path::PathBuf;
use tokio::sync::mpsc;

// To avoid encoding thumbnail into ticket causing excessively long tickets, we use a custom metadata protocol to
// send metadata seprately from the file data. After the receive end sticks the ticket, a seprate connection will
// be made to fetch the metadata.
pub const METADATA_ALPN: &[u8] = b"sendme/metadata/1";

#[derive(Debug, Clone)]
struct MetadataProtocol {
    metadata: Option<FileMetadata>,
}

impl ProtocolHandler for MetadataProtocol {
    /// # Description
    /// Handles incoming connections on the metadata protocol.
    /// It reads a metadata request marker (1 byte) from client, responds with a length-prefixed JSON metadata payload, and waits for the client to close the connection before finishing.
    async fn accept(&self, connection: iroh::endpoint::Connection) -> Result<(), AcceptError> {
        let (mut send_stream, mut recv_stream) =
            match timeout(Duration::from_secs(30), connection.accept_bi()).await {
                Ok(Ok(streams)) => streams,
                Ok(Err(err)) => return Err(err.into()),
                Err(_) => {
                    tracing::debug!("metadata accept_bi timeout (benign)");
                    return Ok(());
                }
            };

        tracing::info!("metadata protocol bi stream accepted");

        let mut req = [0u8; 1];
        timeout(Duration::from_secs(10), recv_stream.read_exact(&mut req))
            .await
            .map_err(|_| {
                AcceptError::from_err(std::io::Error::new(
                    ErrorKind::TimedOut,
                    "metadata request read timeout",
                ))
            })?
            .map_err(AcceptError::from_err)?;

        // Validate request marker (1 means metadata request)
        if req[0] != 1 {
            return Err(AcceptError::from_err(std::io::Error::new(
                ErrorKind::InvalidData,
                format!("invalid metadata request marker: {}", req[0]),
            )));
        }

        tracing::debug!("metadata request marker received");

        let payload = self.metadata.clone().ok_or_else(|| {
            AcceptError::from_err(std::io::Error::new(
                ErrorKind::NotFound,
                "metadata unavailable",
            ))
        })?;

        let meta_bytes = serde_json::to_vec(&payload).map_err(AcceptError::from_err)?;
        const MAX_METADATA_BYTES: usize = 8 * 1024 * 1024;
        if meta_bytes.len() > MAX_METADATA_BYTES {
            return Err(AcceptError::from_err(std::io::Error::new(
                ErrorKind::InvalidData,
                format!("metadata payload too large: {} bytes", meta_bytes.len()),
            )));
        }
        let len_prefix = (meta_bytes.len() as u32).to_be_bytes();

        // Send 4 bytes of length prefix followed by the JSON metadata
        timeout(Duration::from_secs(10), send_stream.write_all(&len_prefix))
            .await
            .map_err(|_| {
                AcceptError::from_err(std::io::Error::new(
                    ErrorKind::TimedOut,
                    "metadata length write timeout",
                ))
            })?
            .map_err(AcceptError::from_err)?;
        timeout(Duration::from_secs(20), send_stream.write_all(&meta_bytes))
            .await
            .map_err(|_| {
                AcceptError::from_err(std::io::Error::new(
                    ErrorKind::TimedOut,
                    "metadata body write timeout",
                ))
            })?
            .map_err(AcceptError::from_err)?;

        send_stream.finish().map_err(AcceptError::from_err)?;

        // Wait for the client to close its receive stream (which means it got the data).
        // This prevents tearing down the QUIC connection before the data buffers are flushed.
        // We give it 30s which is more than the client's read timeout.
        let mut eof_buf = [0u8; 1];
        let _ = timeout(Duration::from_secs(30), recv_stream.read(&mut eof_buf)).await;

        tracing::info!(bytes = meta_bytes.len(), "metadata sent");

        Ok(())
    }
}

fn emit_event(app_handle: &AppHandle, event_name: &str) {
    if let Some(handle) = app_handle {
        if let Err(e) = handle.emit_event(event_name) {
            tracing::warn!("Failed to emit event {}: {}", event_name, e);
        }
    }
}

fn emit_progress_event(
    app_handle: &AppHandle,
    bytes_transferred: u64,
    total_size: u64,
    speed_bps: f64,
) {
    if let Some(handle) = app_handle {
        let event_name = "transfer-progress";

        // Match receive-progress encoding: speed as fixed-point int (×1000).
        let speed_int = (speed_bps * 1000.0) as i64;
        let payload = format!("{}:{}:{}", bytes_transferred, total_size, speed_int);
        if let Err(e) = handle.emit_event_with_payload(event_name, &payload) {
            tracing::warn!("Failed to emit progress event: {}", e);
        }
    }
}

fn speed_bps_from_elapsed(transferred: u64, elapsed_secs: f64) -> f64 {
    const MIN_ELAPSED_SECS: f64 = 0.1;
    if elapsed_secs < MIN_ELAPSED_SECS {
        return 0.0;
    }
    transferred as f64 / elapsed_secs
}

fn emit_active_connection_count(app_handle: &AppHandle, count: usize) {
    if let Some(handle) = app_handle {
        let event_name = "active-connection-count";
        let payload = count.to_string();

        if let Err(e) = handle.emit_event_with_payload(event_name, &payload) {
            tracing::warn!("Failed to emit active connection count event: {}", e);
        }
    }
}

/// Shared send orchestration after blobs are imported into the store.
pub struct ShareSessionOutcome<S> {
    pub ticket: String,
    pub hash: String,
    pub size: u64,
    pub entry_type: String,
    pub router: iroh::protocol::Router,
    pub temp_tag: TempTag,
    pub store: S,
    pub progress_handle: AbortOnDropHandle<anyhow::Result<()>>,
    pub cleanup_dir: Option<PathBuf>,
}

pub async fn run_share_session<S>(
    endpoint: Endpoint,
    store: S,
    blobs: BlobsProtocol,
    temp_tag: TempTag,
    size: u64,
    metadata: Option<FileMetadata>,
    ticket_type: AddrInfoOptions,
    app_handle: &AppHandle,
    entry_type: String,
    relay_mode: RelayMode,
    cleanup_dir: Option<PathBuf>,
    progress_rx: mpsc::Receiver<ProviderMessage>,
) -> anyhow::Result<ShareSessionOutcome<S>>
where
    S: Send + Sync + 'static,
{
    let progress_handle = n0_future::task::spawn(show_provide_progress_with_logging(
        progress_rx,
        app_handle.clone(),
        size,
        entry_type.clone(),
    ));

    let router = iroh::protocol::Router::builder(endpoint)
        .accept(iroh_blobs::ALPN, blobs)
        .accept(METADATA_ALPN, MetadataProtocol { metadata })
        .spawn();

    let ep = router.endpoint();
    timeout(Duration::from_secs(30), async move {
        if !matches!(relay_mode, RelayMode::Disabled) {
            let _ = ep.online().await;
        }
    })
    .await?;

    let hash = temp_tag.hash();

    let mut addr = router.endpoint().addr();
    apply_options(&mut addr, ticket_type);

    let ticket = BlobTicket::new(addr, hash, BlobFormat::HashSeq);

    Ok(ShareSessionOutcome {
        ticket: ticket.to_string(),
        hash: hash.to_hex().to_string(),
        size,
        entry_type,
        router,
        temp_tag,
        store,
        progress_handle: AbortOnDropHandle::new(progress_handle),
        cleanup_dir,
    })
}

/// Range specs used by receivers before the main payload download (hash-seq + child sizes).
fn is_sizes_probe_request(ranges: &iroh_blobs::protocol::ChunkRangesSeq) -> bool {
    use iroh_blobs::protocol::{ChunkRanges, ChunkRangesExt, ChunkRangesSeq};

    ranges == &ChunkRangesSeq::verified_child_sizes()
        || ranges
            == &ChunkRangesSeq::from_ranges_infinite([ChunkRanges::all(), ChunkRanges::last_chunk()])
}

/// Only treat a request as the final payload transfer once nearly all bytes were sent.
fn transfer_payload_complete(bytes_sent: u64, total_size: u64) -> bool {
    if total_size == 0 {
        return true;
    }
    // Size probes only fetch hash-seq headers and last chunks — far below payload size.
    bytes_sent.saturating_mul(100) >= total_size.saturating_mul(95)
}

async fn show_provide_progress_with_logging(
    mut recv: mpsc::Receiver<iroh_blobs::provider::events::ProviderMessage>,
    app_handle: AppHandle,
    total_collection_size: u64,
    entry_type: String,
) -> anyhow::Result<()> {
    use n0_future::FuturesUnordered;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::sync::Mutex;

    let mut tasks = FuturesUnordered::new();

    #[derive(Clone)]
    struct TransferState {
        start_time: Instant,
        total_size: u64,
        accounted_payload_bytes: u64,
        current_blob_size: u64,
        current_blob_end_offset: u64,
        ignore_current_blob: bool,
    }

    let transfer_states: Arc<Mutex<std::collections::HashMap<(u64, u64), TransferState>>> =
        Arc::new(Mutex::new(std::collections::HashMap::new()));

    let active_requests = Arc::new(AtomicUsize::new(0));
    let completed_requests = Arc::new(AtomicUsize::new(0));
    let has_emitted_started = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let has_emitted_completed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let last_request_time: Arc<tokio::sync::Mutex<Option<Instant>>> =
        Arc::new(tokio::sync::Mutex::new(None));

    loop {
        tokio::select! {
            biased;
            item = recv.recv() => {
                let Some(item) = item else {
                    break;
                };

                match item {
                    iroh_blobs::provider::events::ProviderMessage::ClientConnectedNotify(_msg) => {
                    }
                    iroh_blobs::provider::events::ProviderMessage::ConnectionClosed(_msg) => {
                    }
                    iroh_blobs::provider::events::ProviderMessage::GetRequestReceivedNotify(msg) => {
                        let is_sizes_probe_request = is_sizes_probe_request(&msg.request.ranges);

                        let connection_id = msg.connection_id;
                        let request_id = msg.request_id;

                        if !is_sizes_probe_request {
                            active_requests.fetch_add(1, Ordering::SeqCst);

                            let mut last_time = last_request_time.lock().await;
                            *last_time = Some(Instant::now());
                        }

                        let app_handle_task = app_handle.clone();
                        let transfer_states_task = transfer_states.clone();
                        let active_requests_task = active_requests.clone();
                        let completed_requests_task = completed_requests.clone();
                        let has_emitted_started_task = has_emitted_started.clone();
                        let has_emitted_completed_task = has_emitted_completed.clone();
                        let last_request_time_task = last_request_time.clone();
                        let entry_type_task = entry_type.clone();
                        let total_collection_size_task = total_collection_size;

                        let mut rx = msg.rx;
                        tasks.push(async move {
                            if is_sizes_probe_request {
                                while let Ok(Some(_)) = rx.recv().await {}
                                return;
                            }

                            let mut transfer_started = false;
                            let mut request_completed = false;

                            while let Ok(Some(update)) = rx.recv().await {
                                match update {
                                    iroh_blobs::provider::events::RequestUpdate::Started(m) => {
                                        if !transfer_started {
                                            let active_count = {
                                                let mut states = transfer_states_task.lock().await;
                                                states.insert(
                                                    (connection_id, request_id),
                                                    TransferState {
                                                        start_time: Instant::now(),
                                                        total_size: total_collection_size,
                                                        accounted_payload_bytes: 0,
                                                        current_blob_size: 0,
                                                        current_blob_end_offset: 0,
                                                        ignore_current_blob: false,
                                                    }
                                                );
                                                states.len()
                                            };

                                            emit_active_connection_count(&app_handle_task, active_count);

                                            if !has_emitted_started_task.swap(true, Ordering::SeqCst) {
                                                emit_event(&app_handle_task, "transfer-started");
                                            }

                                            transfer_started = true;
                                        }

                                        let is_metadata_blob =
                                            (entry_type_task == "directory" || entry_type_task == "collection")
                                                && m.index == 0;

                                        {
                                            let mut states = transfer_states_task.lock().await;
                                            if let Some(state) = states.get_mut(&(connection_id, request_id)) {
                                                let was_ignoring_current_blob = state.ignore_current_blob;

                                                if !state.ignore_current_blob {
                                                    state.accounted_payload_bytes = state
                                                        .accounted_payload_bytes
                                                        .saturating_add(state.current_blob_size)
                                                        .min(state.total_size);
                                                }

                                                if was_ignoring_current_blob && !is_metadata_blob {
                                                    state.start_time = Instant::now();
                                                    state.accounted_payload_bytes = 0;
                                                }

                                                state.current_blob_size = m.size;
                                                state.current_blob_end_offset = 0;
                                                state.ignore_current_blob = is_metadata_blob;
                                            }
                                        }
                                    }
                                    iroh_blobs::provider::events::RequestUpdate::Progress(m) => {
                                        if !transfer_started {
                                            let active_count = {
                                                let mut states = transfer_states_task.lock().await;
                                                states.insert(
                                                    (connection_id, request_id),
                                                    TransferState {
                                                        start_time: Instant::now(),
                                                        total_size: total_collection_size,
                                                        accounted_payload_bytes: 0,
                                                        current_blob_size: 0,
                                                        current_blob_end_offset: 0,
                                                        ignore_current_blob: true,
                                                    }
                                                );
                                                states.len()
                                            };

                                            emit_active_connection_count(&app_handle_task, active_count);

                                            if !has_emitted_started_task.swap(true, Ordering::SeqCst) {
                                                emit_event(&app_handle_task, "transfer-started");
                                            }
                                            transfer_started = true;
                                        }

                                        if let Some((transferred, total_size, elapsed)) = {
                                            let mut states = transfer_states_task.lock().await;
                                            states.get_mut(&(connection_id, request_id)).map(|state| {
                                                if !state.ignore_current_blob {
                                                    state.current_blob_end_offset = m
                                                        .end_offset
                                                        .min(state.current_blob_size)
                                                        .max(state.current_blob_end_offset);
                                                }

                                                let transferred = if state.ignore_current_blob {
                                                    state.accounted_payload_bytes
                                                } else {
                                                    state
                                                        .accounted_payload_bytes
                                                        .saturating_add(state.current_blob_end_offset)
                                                        .min(state.total_size)
                                                };

                                                (
                                                    transferred,
                                                    state.total_size,
                                                    state.start_time.elapsed().as_secs_f64(),
                                                )
                                            })
                                        } {
                                            let speed_bps = speed_bps_from_elapsed(
                                                transferred.min(total_size),
                                                elapsed,
                                            );
                                            emit_progress_event(
                                                &app_handle_task,
                                                transferred.min(total_size),
                                                total_size,
                                                speed_bps,
                                            );
                                        }
                                    }
                                    iroh_blobs::provider::events::RequestUpdate::Completed(_m) => {
                                        if transfer_started && !request_completed {
                                            let bytes_sent = {
                                                let mut states = transfer_states_task.lock().await;
                                                if let Some(state) =
                                                    states.get_mut(&(connection_id, request_id))
                                                {
                                                    if !state.ignore_current_blob {
                                                        state.accounted_payload_bytes = state
                                                            .accounted_payload_bytes
                                                            .saturating_add(state.current_blob_size)
                                                            .min(state.total_size);
                                                    }
                                                    Some(state.accounted_payload_bytes)
                                                } else {
                                                    None
                                                }
                                            };

                                            let active_count = {
                                                let mut states = transfer_states_task.lock().await;
                                                states.remove(&(connection_id, request_id));
                                                states.len()
                                            };

                                            emit_active_connection_count(&app_handle_task, active_count);

                                            request_completed = true;

                                            if !transfer_payload_complete(
                                                bytes_sent.unwrap_or(0),
                                                total_collection_size_task,
                                            ) {
                                                active_requests_task.fetch_sub(1, Ordering::SeqCst);
                                                continue;
                                            }

                                            let completed = completed_requests_task.fetch_add(1, Ordering::SeqCst) + 1;
                                            let active = active_requests_task.load(Ordering::SeqCst);

                                            // Size-probe requests are ignored above. A completed payload
                                            // request with all bytes sent indicates the end of the transfer.
                                            let min_required = 1;

                                            if completed >= active
                                                && completed >= min_required {
                                                let active_before_wait = active;

                                                sleep(Duration::from_millis(500)).await;

                                                let completed_after = completed_requests_task.load(Ordering::SeqCst);
                                                let active_after = active_requests_task.load(Ordering::SeqCst);

                                                let new_requests_arrived = active_after > active_before_wait;

                                                let has_active_transfers = {
                                                    let states = transfer_states_task.lock().await;
                                                    !states.is_empty()
                                                };

                                                let last_request_recent = {
                                                    let last_time = last_request_time_task.lock().await;
                                                    if let Some(time) = *last_time {
                                                        time.elapsed() < Duration::from_millis(500)
                                                    } else {
                                                        false
                                                    }
                                                };

                                                if completed_after >= active_after
                                                    && completed_after >= min_required
                                                    && !new_requests_arrived
                                                    && !has_active_transfers
                                                    && !last_request_recent {
                                                    if !has_emitted_completed_task
                                                        .swap(true, Ordering::SeqCst)
                                                    {
                                                        emit_event(&app_handle_task, "transfer-completed");
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    iroh_blobs::provider::events::RequestUpdate::Aborted(_m) => {
                                        tracing::warn!("Request aborted: conn {} req {}",
                                            connection_id, request_id);
                                        if transfer_started && !request_completed {
                                            let active_count = {
                                                let mut states = transfer_states_task.lock().await;
                                                states.remove(&(connection_id, request_id));
                                                states.len()
                                            };

                                            emit_active_connection_count(&app_handle_task, active_count);

                                            request_completed = true;

                                            let completed = completed_requests_task.fetch_add(1, Ordering::SeqCst) + 1;
                                            let active = active_requests_task.load(Ordering::SeqCst);

                                            if completed >= active {
                                                emit_event(&app_handle_task, "transfer-failed");
                                            }
                                        }
                                    }
                                }
                            }

                            if transfer_started && !request_completed {
                                let bytes_sent = {
                                    let states = transfer_states_task.lock().await;
                                    states
                                        .get(&(connection_id, request_id))
                                        .map(|state| state.accounted_payload_bytes)
                                };

                                if !transfer_payload_complete(
                                    bytes_sent.unwrap_or(0),
                                    total_collection_size_task,
                                ) {
                                    active_requests_task.fetch_sub(1, Ordering::SeqCst);
                                    return;
                                }

                                let completed = completed_requests_task.fetch_add(1, Ordering::SeqCst) + 1;
                                let active = active_requests_task.load(Ordering::SeqCst);

                                let min_required = 1;

                                if completed >= active
                                    && completed >= min_required {
                                    let active_before_wait = active;

                                    sleep(Duration::from_millis(500)).await;

                                    let completed_after = completed_requests_task.load(Ordering::SeqCst);
                                    let active_after = active_requests_task.load(Ordering::SeqCst);

                                    let new_requests_arrived = active_after > active_before_wait;

                                    let has_active_transfers = {
                                        let states = transfer_states_task.lock().await;
                                        !states.is_empty()
                                    };

                                    let last_request_recent = {
                                        let last_time = last_request_time_task.lock().await;
                                        if let Some(time) = *last_time {
                                            time.elapsed() < Duration::from_millis(500)
                                        } else {
                                            false
                                        }
                                    };

                                    if completed_after >= active_after
                                        && completed_after >= min_required
                                        && !new_requests_arrived
                                        && !has_active_transfers
                                        && !last_request_recent {
                                        if !has_emitted_completed_task
                                            .swap(true, Ordering::SeqCst)
                                        {
                                            emit_event(&app_handle_task, "transfer-completed");
                                        }
                                    }
                                }
                            }
                        });
                    }
                    _ => {
                    }
                }
            }
            Some(_) = tasks.next(), if !tasks.is_empty() => {
            }
        }
    }

    while tasks.next().await.is_some() {}

    let completed = completed_requests.load(Ordering::SeqCst);
    let active = active_requests.load(Ordering::SeqCst);

    let min_required = 1;

    if completed >= active && completed >= min_required && completed > 0 {
        if !has_emitted_completed.swap(true, Ordering::SeqCst) {
            emit_event(&app_handle, "transfer-completed");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{is_sizes_probe_request, transfer_payload_complete};
    use iroh_blobs::protocol::{ChunkRanges, ChunkRangesExt, ChunkRangesSeq};

    #[test]
    fn sizes_probe_detection() {
        assert!(is_sizes_probe_request(&ChunkRangesSeq::verified_child_sizes()));
        assert!(is_sizes_probe_request(&ChunkRangesSeq::from_ranges_infinite([
            ChunkRanges::all(),
            ChunkRanges::last_chunk(),
        ])));
        assert!(!is_sizes_probe_request(&ChunkRangesSeq::all()));
    }

    #[test]
    fn payload_complete_threshold() {
        assert!(transfer_payload_complete(1000, 1000));
        assert!(transfer_payload_complete(950, 1000));
        assert!(!transfer_payload_complete(100, 1000));
        assert!(transfer_payload_complete(0, 0));
    }
}
