mod common;

use std::path::Path;
use std::time::Duration;

use common::MockEventEmitter;
use engine::{NodeService, PairingStatus};
use iroh::endpoint::RelayMode;
use iroh::SecretKey;

const START_TIMEOUT: Duration = Duration::from_secs(60);
const JOIN_DEADLINE: Duration = Duration::from_secs(90);
const SETTLE_DEADLINE: Duration = Duration::from_secs(30);

/// Pre-seed a distinct `identity.key`. The mock keychain is process-global,
/// so without this the second node would inherit the first node's secret.
fn seed_identity(data_dir: &Path) -> String {
    std::fs::create_dir_all(data_dir).expect("create data dir");
    let secret = SecretKey::generate();
    std::fs::write(data_dir.join("identity.key"), secret.to_bytes()).expect("write identity.key");
    data_encoding::HEXLOWER.encode(secret.public().as_bytes())
}

async fn start_node(data_dir: &Path, emitter: std::sync::Arc<MockEventEmitter>) -> NodeService {
    tokio::time::timeout(
        START_TIMEOUT,
        NodeService::start(data_dir, RelayMode::Default, Some(emitter)),
    )
    .await
    .expect("node start timed out")
    .expect("node start failed")
}

async fn wait_until(what: &str, deadline: Duration, check: impl Fn() -> bool) {
    let end = tokio::time::Instant::now() + deadline;
    while !check() {
        assert!(
            tokio::time::Instant::now() < end,
            "timed out waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

fn paired_status(node: &NodeService, endpoint_id: &str) -> Option<PairingStatus> {
    node.list_paired()
        .expect("list_paired")
        .into_iter()
        .find(|d| d.endpoint_id.eq_ignore_ascii_case(endpoint_id))
        .map(|d| d.pairing_status)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn e2e_pairing_lifecycle() {
    // Cargo only shows these logs when the test fails.
    let _ = tracing_subscriber::fmt()
        .with_env_filter("pairing-dev=info")
        .try_init();
    let host_dir = tempfile::tempdir().expect("host dir");
    let joiner_dir = tempfile::tempdir().expect("joiner dir");
    let host_id = seed_identity(host_dir.path());
    let joiner_id = seed_identity(joiner_dir.path());
    assert_ne!(host_id, joiner_id, "nodes must not share an identity");

    let host_events = MockEventEmitter::new();
    let joiner_events = MockEventEmitter::new();
    let host = start_node(host_dir.path(), host_events.clone()).await;
    let joiner = start_node(joiner_dir.path(), joiner_events.clone()).await;
    assert_eq!(host.device_info().endpoint_id, host_id);

    // --- Pair while the host window is open ---
    let ticket = host
        .start_pairing_host(Some(300))
        .await
        .expect("open pairing window");

    // Address discovery may lag right after the endpoints come online.
    let end = tokio::time::Instant::now() + JOIN_DEADLINE;
    loop {
        match joiner.join_pairing(&ticket).await {
            Ok(()) => break,
            Err(err) => {
                assert!(
                    tokio::time::Instant::now() < end,
                    "join_pairing did not succeed within {JOIN_DEADLINE:?}: {err:#}"
                );
                eprintln!("join attempt failed, retrying: {err:#}");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }

    assert_eq!(
        paired_status(&joiner, &host_id),
        Some(PairingStatus::Active),
        "joiner should store the host as an active paired device"
    );
    assert!(joiner_events.has_event("device-paired"));

    // The host finishes its side of the handshake asynchronously.
    wait_until("host to store the joiner as paired", SETTLE_DEADLINE, || {
        paired_status(&host, &joiner_id) == Some(PairingStatus::Active)
    })
    .await;
    wait_until("host device-paired event", SETTLE_DEADLINE, || {
        host_events.has_event("device-paired")
    })
    .await;

    // --- Closed window: unknown peers must be rejected by the access hook ---
    host.stop_pairing_host().await;

    let stranger_dir = tempfile::tempdir().expect("stranger dir");
    let stranger_id = seed_identity(stranger_dir.path());
    let stranger = start_node(stranger_dir.path(), MockEventEmitter::new()).await;
    let denied = tokio::time::timeout(START_TIMEOUT, stranger.join_pairing(&ticket))
        .await
        .expect("stranger join attempt did not resolve");
    assert!(
        denied.is_err(),
        "join must fail for an unknown peer while the pairing window is closed"
    );
    assert_eq!(
        paired_status(&host, &stranger_id),
        None,
        "host must not store the rejected peer"
    );

    // --- Forget propagates to the peer ---
    joiner.forget_paired(&host_id).await.expect("forget host");
    assert_eq!(
        paired_status(&joiner, &host_id),
        None,
        "forgetting removes the device locally"
    );
    wait_until(
        "host to mark the joiner unpaired-remotely",
        SETTLE_DEADLINE,
        || paired_status(&host, &joiner_id) == Some(PairingStatus::UnpairedRemotely),
    )
    .await;
    wait_until("host device-unpaired event", SETTLE_DEADLINE, || {
        host_events.has_event("device-unpaired")
    })
    .await;

    stranger.shutdown().await.expect("shutdown stranger");
    joiner.shutdown().await.expect("shutdown joiner");
    host.shutdown().await.expect("shutdown host");
}
