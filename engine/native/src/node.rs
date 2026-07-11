use std::collections::HashSet;
use std::path::Path;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use iroh::endpoint::{
    presets, AfterHandshakeOutcome, Connection, Endpoint, EndpointHooks, RelayMode, Side,
};
use iroh::protocol::{AcceptError, ProtocolHandler, Router};
use iroh::{address_lookup::pkarr::PkarrPublisher, EndpointAddr, EndpointId, TransportAddr};
use protocol::{
    apply_options, export_connection_keying_material, read_message, sign_challenge,
    verify_challenge, write_message, AddrInfoOptions, AppHandle, ControlMessage, PairedDevice,
    RememberVote, CONTROL_ALPN,
};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

use crate::device_identity::{load_or_create_identity, DeviceIdentity, DeviceInfo, PairedDeviceStore};

#[derive(Debug)]
struct AccessState {
    allowed: HashSet<EndpointId>,
    pairing_host_open: bool,
}

#[derive(Debug)]
struct PairedOnlyHook {
    access: Arc<RwLock<AccessState>>,
}

impl EndpointHooks for PairedOnlyHook {
    async fn after_handshake(&self, conn: &Connection) -> AfterHandshakeOutcome {
        if conn.side() != Side::Server {
            return AfterHandshakeOutcome::accept();
        }
        if conn.alpn() != CONTROL_ALPN {
            return AfterHandshakeOutcome::accept();
        }
        let remote = conn.remote_id();
        let access = self.access.read().await;
        let allowed = access.allowed.contains(&remote);
        if access.pairing_host_open || allowed {
            info!(
                remote = %remote,
                pairing_host_open = access.pairing_host_open,
                allowed,
                allowed_count = access.allowed.len(),
                "paired-invite: control handshake accepted"
            );
            return AfterHandshakeOutcome::accept();
        }
        warn!(
            remote = %remote,
            allowed_count = access.allowed.len(),
            "paired-invite: control handshake rejected (peer not allowed)"
        );
        AfterHandshakeOutcome::Reject {
            error_code: 403u32.into(),
            reason: b"unauthorized control peer".to_vec(),
        }
    }
}

#[derive(Clone)]
struct ControlCtx {
    identity: Arc<DeviceIdentity>,
    paired_store: Arc<PairedDeviceStore>,
    access: Arc<RwLock<AccessState>>,
    app_handle: AppHandle,
}

#[derive(Clone)]
struct ControlProtocol {
    ctx: ControlCtx,
}

impl std::fmt::Debug for ControlProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ControlProtocol").finish_non_exhaustive()
    }
}

impl ControlProtocol {
    async fn handle_connection(&self, conn: Connection) -> anyhow::Result<()> {
        let remote = conn.remote_id();
        let local = self.ctx.identity.endpoint_id();
        let allowed = self.is_allowed(&remote).await;
        let in_store = self.is_in_paired_store(&remote).await;
        info!(
            remote = %remote,
            local = %local,
            allowed,
            in_paired_store = in_store,
            "paired-invite: control session started"
        );

        let keying = export_connection_keying_material(&conn).context("export keying material")?;
        let (mut send, mut recv) = conn
            .accept_bi()
            .await
            .context("accept bi stream for control session")?;
        info!(remote = %remote, "paired-invite: bi stream accepted");

        let our_info = ControlMessage::PairingInfo {
            endpoint_id: self.ctx.identity.endpoint_id(),
            display_name: self.ctx.identity.display_name().to_string(),
            device_type: self.ctx.identity.meta.device_type.clone(),
            signature: sign_challenge(&self.ctx.identity.secret_key, &keying),
        };
        write_message(&mut send, &our_info)
            .await
            .context("write local PairingInfo")?;
        info!(remote = %remote, "paired-invite: sent local PairingInfo");

        let mut remote_info: Option<ControlMessage> = None;
        let mut remote_vote: Option<RememberVote> = None;
        let session_id = uuid::Uuid::new_v4().to_string();

        loop {
            let msg = match read_message(&mut recv).await {
                Ok(m) => m,
                Err(err) => {
                    info!(
                        remote = %remote,
                        error = %err,
                        had_remote_info = remote_info.is_some(),
                        "paired-invite: control read ended"
                    );
                    break;
                }
            };
            match msg {
                ControlMessage::PairingInfo {
                    endpoint_id,
                    display_name,
                    device_type,
                    signature,
                } => {
                    let Ok(peer_id) = EndpointId::from_str(&endpoint_id) else {
                        continue;
                    };
                    if !verify_challenge(&peer_id, &keying, &signature) {
                        warn!(
                            remote = %remote,
                            peer_id = %endpoint_id,
                            "paired-invite: PairingInfo signature invalid"
                        );
                        continue;
                    }
                    info!(
                        remote = %remote,
                        peer_id = %endpoint_id,
                        display_name = %display_name,
                        "paired-invite: received remote PairingInfo"
                    );
                    remote_info = Some(ControlMessage::PairingInfo {
                        endpoint_id,
                        display_name,
                        device_type,
                        signature,
                    });
                }
                ControlMessage::RememberVote { vote, .. } => {
                    info!(remote = %remote, ?vote, "paired-invite: received RememberVote");
                    remote_vote = Some(vote);
                }
                ControlMessage::Invite {
                    blob_ticket,
                    file_count,
                    total_size,
                    sender_name,
                } => {
                    let allowed = self.is_allowed(&remote).await;
                    let in_store = self.is_in_paired_store(&remote).await;
                    info!(
                        remote = %remote,
                        allowed,
                        in_paired_store = in_store,
                        file_count,
                        total_size,
                        sender_name = %sender_name,
                        ticket_len = blob_ticket.len(),
                        "paired-invite: received Invite"
                    );
                    if !allowed {
                        warn!(
                            remote = %remote,
                            in_paired_store = in_store,
                            "paired-invite: Invite dropped (peer not in access allowlist)"
                        );
                        continue;
                    }
                    let payload = serde_json::json!({
                        "blob_ticket": blob_ticket,
                        "file_count": file_count,
                        "total_size": total_size,
                        "sender_name": sender_name,
                        "remote_endpoint_id": remote.to_string(),
                    });
                    if let Some(handle) = &self.ctx.app_handle {
                        match handle.emit_event_with_payload(
                            "paired-invite-received",
                            &payload.to_string(),
                        ) {
                            Ok(()) => info!(
                                remote = %remote,
                                "paired-invite: emitted paired-invite-received event"
                            ),
                            Err(err) => warn!(
                                remote = %remote,
                                error = %err,
                                "paired-invite: failed to emit paired-invite-received event"
                            ),
                        }
                    } else {
                        warn!(
                            remote = %remote,
                            "paired-invite: no app handle; Invite not forwarded to UI"
                        );
                    }
                }
                ControlMessage::InviteResponse { response, .. } => {
                    debug!(?response, "invite response from {remote}");
                }
                ControlMessage::Recognition { signature } => {
                    if verify_challenge(&remote, &keying, &signature) {
                        let _ = self.ctx.paired_store.touch(
                            &remote.to_string(),
                            protocol::identity::unix_now_ms(),
                        );
                    }
                }
            }

            if remote_info.is_some() && remote_vote == Some(RememberVote::Remember) {
                if let Some(ControlMessage::PairingInfo {
                    endpoint_id,
                    display_name,
                    device_type,
                    ..
                }) = &remote_info
                {
                    let now = protocol::identity::unix_now_ms();
                    let device = PairedDevice {
                        endpoint_id: endpoint_id.clone(),
                        display_name: display_name.clone(),
                        device_type: device_type.clone(),
                        paired_at: now,
                        last_seen_at: now,
                    };
                    let _ = self.ctx.paired_store.remember(device);
                    self.allow_peer(remote).await;
                    if let Some(handle) = &self.ctx.app_handle {
                        let _ = handle.emit_event("device-paired");
                    }
                }
                break;
            }
        }

        if remote_info.is_some() {
            let vote = ControlMessage::RememberVote {
                session_id,
                vote: RememberVote::Remember,
            };
            if let Err(err) = write_message(&mut send, &vote).await {
                info!(
                    remote = %remote,
                    error = %err,
                    "paired-invite: failed to send RememberVote (peer may have closed)"
                );
            }
        }

        info!(remote = %remote, "paired-invite: control session finished");
        Ok(())
    }

    async fn is_allowed(&self, remote: &EndpointId) -> bool {
        self.ctx.access.read().await.allowed.contains(remote)
    }

    async fn is_in_paired_store(&self, remote: &EndpointId) -> bool {
        let remote_str = remote.to_string();
        self.ctx
            .paired_store
            .list()
            .ok()
            .is_some_and(|devices| devices.iter().any(|d| d.endpoint_id == remote_str))
    }

    async fn allow_peer(&self, remote: EndpointId) {
        self.ctx.access.write().await.allowed.insert(remote);
    }
}

impl ProtocolHandler for ControlProtocol {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let this = self.clone();
        let remote = connection.remote_id();
        if let Err(err) = this.handle_connection(connection).await {
            warn!(
                remote = %remote,
                error = %err,
                "paired-invite: control connection failed"
            );
        }
        Ok(())
    }
}

struct NodeRuntime {
    endpoint: Endpoint,
    router: Router,
}

pub struct NodeService {
    runtime: Mutex<NodeRuntime>,
    identity: Arc<DeviceIdentity>,
    paired_store: Arc<PairedDeviceStore>,
    access: Arc<RwLock<AccessState>>,
    pairing_host_open: Arc<AtomicBool>,
    pairing_expire_task: Mutex<Option<JoinHandle<()>>>,
    app_handle: AppHandle,
    relay_mode: Mutex<RelayMode>,
}

impl NodeService {
    pub async fn start(
        data_dir: &Path,
        relay_mode: RelayMode,
        app_handle: AppHandle,
    ) -> anyhow::Result<Self> {
        let identity = Arc::new(load_or_create_identity(data_dir)?);
        let paired_store = Arc::new(PairedDeviceStore::new(data_dir));
        let allowed = load_allowed_from_store(&paired_store)?;
        info!(
            local_endpoint = %identity.endpoint_id(),
            allowed_peers = ?allowed.iter().map(|id| id.to_string()).collect::<Vec<_>>(),
            allowed_count = allowed.len(),
            "paired-invite: node started"
        );

        let access = Arc::new(RwLock::new(AccessState {
            allowed,
            pairing_host_open: false,
        }));
        let pairing_host_open = Arc::new(AtomicBool::new(false));

        let runtime = build_runtime(
            identity.clone(),
            paired_store.clone(),
            access.clone(),
            app_handle.clone(),
            relay_mode.clone(),
        )
        .await?;

        Ok(Self {
            runtime: Mutex::new(runtime),
            identity,
            paired_store,
            access,
            pairing_host_open,
            pairing_expire_task: Mutex::new(None),
            app_handle,
            relay_mode: Mutex::new(relay_mode),
        })
    }

    pub async fn shutdown(&self) -> anyhow::Result<()> {
        self.stop_pairing_host().await;
        let mut runtime = self.runtime.lock().await;
        runtime.router.shutdown().await?;
        runtime.endpoint.close().await;
        Ok(())
    }

    pub async fn reconfigure_relay(&self, relay_mode: RelayMode) -> anyhow::Result<()> {
        self.stop_pairing_host().await;

        let mut runtime = self.runtime.lock().await;
        runtime.router.shutdown().await?;
        runtime.endpoint.close().await;

        let new_runtime = build_runtime(
            self.identity.clone(),
            self.paired_store.clone(),
            self.access.clone(),
            self.app_handle.clone(),
            relay_mode.clone(),
        )
        .await?;

        *runtime = new_runtime;
        *self.relay_mode.lock().await = relay_mode;
        Ok(())
    }

    pub fn device_info(&self) -> DeviceInfo {
        DeviceInfo::from(self.identity.as_ref())
    }

    pub fn list_paired(&self) -> anyhow::Result<Vec<PairedDevice>> {
        self.paired_store.list()
    }

    pub async fn forget_paired(&self, endpoint_id: &str) -> anyhow::Result<()> {
        if let Ok(id) = EndpointId::from_str(endpoint_id) {
            self.access.write().await.allowed.remove(&id);
        }
        self.paired_store.forget(endpoint_id)
    }

    pub fn pairing_ticket(&self) -> anyhow::Result<String> {
        let runtime = self
            .runtime
            .try_lock()
            .context("node runtime busy")?;
        let mut addr = runtime.endpoint.addr();
        apply_options(&mut addr, AddrInfoOptions::Relay);
        let relay_url = addr.relay_urls().next().map(|u| u.to_string());
        let ticket = protocol::PairingTicket {
            v: 1,
            kind: protocol::PairingTicket::KIND.to_string(),
            endpoint_id: self.identity.endpoint_id(),
            relay_url,
        };
        ticket.encode()
    }

    pub async fn start_pairing_host(&self) -> anyhow::Result<String> {
        self.stop_pairing_host().await;

        self.pairing_host_open.store(true, Ordering::SeqCst);
        self.access.write().await.pairing_host_open = true;

        let access = self.access.clone();
        let flag = self.pairing_host_open.clone();
        let app_handle = self.app_handle.clone();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(
                protocol::pairing::PAIRING_VOTE_TIMEOUT_SECS,
            ))
            .await;
            flag.store(false, Ordering::SeqCst);
            access.write().await.pairing_host_open = false;
            if let Some(handle) = &app_handle {
                let _ = handle.emit_event("pairing-host-expired");
            }
        });
        *self.pairing_expire_task.lock().await = Some(handle);

        self.pairing_ticket()
    }

    pub async fn stop_pairing_host(&self) {
        if let Some(handle) = self.pairing_expire_task.lock().await.take() {
            handle.abort();
        }
        self.pairing_host_open.store(false, Ordering::SeqCst);
        self.access.write().await.pairing_host_open = false;
    }

    pub async fn join_pairing(&self, ticket_str: &str) -> anyhow::Result<()> {
        let ticket = protocol::PairingTicket::decode(ticket_str)?;
        let mut addr = EndpointAddr::from(EndpointId::from_str(&ticket.endpoint_id)?);
        if let Some(relay) = ticket.relay_url {
            if let Ok(url) = relay.parse() {
                addr.addrs.insert(TransportAddr::Relay(url));
            }
        }

        let runtime = self.runtime.lock().await;
        let conn = runtime
            .endpoint
            .connect(addr, CONTROL_ALPN)
            .await
            .context("pairing connect failed")?;
        drop(runtime);

        let keying = export_connection_keying_material(&conn)?;
        let (mut send, mut recv) = conn.open_bi().await?;

        let info = ControlMessage::PairingInfo {
            endpoint_id: self.identity.endpoint_id(),
            display_name: self.identity.display_name().to_string(),
            device_type: self.identity.meta.device_type.clone(),
            signature: sign_challenge(&self.identity.secret_key, &keying),
        };
        write_message(&mut send, &info).await?;
        let vote = ControlMessage::RememberVote {
            session_id: uuid::Uuid::new_v4().to_string(),
            vote: RememberVote::Remember,
        };
        write_message(&mut send, &vote).await?;

        if let Ok(ControlMessage::PairingInfo {
            endpoint_id,
            display_name,
            device_type,
            signature,
        }) = read_message(&mut recv).await
        {
            let peer_id = EndpointId::from_str(&endpoint_id)?;
            if verify_challenge(&peer_id, &keying, &signature) {
                let now = protocol::identity::unix_now_ms();
                self.paired_store.remember(PairedDevice {
                    endpoint_id,
                    display_name,
                    device_type,
                    paired_at: now,
                    last_seen_at: now,
                })?;
                self.access.write().await.allowed.insert(peer_id);
                if let Some(handle) = &self.app_handle {
                    let _ = handle.emit_event("device-paired");
                }
            }
        }
        Ok(())
    }

    pub async fn invite_paired_device(
        &self,
        remote_endpoint_id: &str,
        blob_ticket: &str,
        file_count: u32,
        total_size: u64,
    ) -> anyhow::Result<bool> {
        let remote = EndpointId::from_str(remote_endpoint_id)?;
        let local = self.identity.endpoint_id();
        let in_allowlist = self.access.read().await.allowed.contains(&remote);
        info!(
            local_endpoint = %local,
            remote_endpoint = %remote,
            in_allowlist,
            file_count,
            total_size,
            ticket_len = blob_ticket.len(),
            "paired-invite: sender inviting paired device"
        );
        if !in_allowlist {
            warn!(
                remote_endpoint = %remote,
                "paired-invite: sender abort — remote not in allowlist"
            );
            anyhow::bail!("unknown paired device");
        }
        let addr = EndpointAddr::from(remote);
        let runtime = self.runtime.lock().await;
        let local_node = runtime.endpoint.id().to_string();
        info!(
            local_endpoint = %local,
            local_node = %local_node,
            remote_endpoint = %remote,
            "paired-invite: sender connecting control channel"
        );
        let connect = tokio::time::timeout(
            Duration::from_secs(30),
            runtime.endpoint.connect(addr, CONTROL_ALPN),
        )
        .await;
        drop(runtime);

        let conn = match connect {
            Ok(Ok(conn)) => {
                info!(
                    remote_endpoint = %remote,
                    remote_conn = %conn.remote_id(),
                    "paired-invite: sender connected"
                );
                conn
            }
            Ok(Err(err)) => {
                warn!(
                    remote_endpoint = %remote,
                    error = %err,
                    "paired-invite: sender connect failed"
                );
                return Ok(false);
            }
            Err(_) => {
                warn!(
                    remote_endpoint = %remote,
                    "paired-invite: sender connect timed out (30s)"
                );
                return Ok(false);
            }
        };

        let (mut send, _recv) = conn
            .open_bi()
            .await
            .context("open bi stream for invite")?;
        info!(remote_endpoint = %remote, "paired-invite: sender opened bi stream");

        let invite = ControlMessage::Invite {
            blob_ticket: blob_ticket.to_string(),
            file_count,
            total_size,
            sender_name: self.identity.display_name().to_string(),
        };
        write_message(&mut send, &invite)
            .await
            .context("write Invite message")?;
        info!(
            remote_endpoint = %remote,
            file_count,
            total_size,
            "paired-invite: sender wrote Invite (closing connection)"
        );
        Ok(true)
    }
}

fn load_allowed_from_store(paired_store: &PairedDeviceStore) -> anyhow::Result<HashSet<EndpointId>> {
    let mut allowed = HashSet::new();
    for device in paired_store.list()? {
        if let Ok(id) = EndpointId::from_str(&device.endpoint_id) {
            allowed.insert(id);
        }
    }
    Ok(allowed)
}

async fn build_runtime(
    identity: Arc<DeviceIdentity>,
    paired_store: Arc<PairedDeviceStore>,
    access: Arc<RwLock<AccessState>>,
    app_handle: AppHandle,
    relay_mode: RelayMode,
) -> anyhow::Result<NodeRuntime> {
    let hook = PairedOnlyHook {
        access: access.clone(),
    };

    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(identity.secret_key.clone())
        .address_lookup(PkarrPublisher::n0_dns())
        .relay_mode(relay_mode)
        .hooks(hook)
        .alpns(vec![CONTROL_ALPN.to_vec()])
        .bind()
        .await?;

    endpoint.online().await;

    let control = ControlProtocol {
        ctx: ControlCtx {
            identity,
            paired_store,
            access,
            app_handle,
        },
    };

    let router = Router::builder(endpoint.clone())
        .accept(CONTROL_ALPN, control)
        .spawn();

    Ok(NodeRuntime { endpoint, router })
}
