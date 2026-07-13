use std::collections::HashMap;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use iroh::endpoint::Connection;
use iroh::EndpointId;
use protocol::{
    export_connection_keying_material, sign_challenge, write_message, ControlMessage,
    PairedDevice, CONTROL_ALPN, PAIRED_INVITE_WAIT_SECS, PAIRED_RECONNECT_MAX_SECS,
    PAIRED_RECONNECT_MIN_SECS, PRESENCE_CONNECT_TIMEOUT_SECS,
};
use tokio::sync::{Mutex, Notify, OnceCell, RwLock};
use tokio::task::JoinHandle;

use crate::device_identity::{DeviceIdentity, PairedDeviceStore};
use crate::pairing_dev_log::{
    direction_from_side, elapsed_ms, log_pairing_flow_error, peer_role_from_side,
};
use crate::pairing_util::{build_control_connect_addr, set_presence};
use crate::{pairing_flow, pairing_flow_warn};
use protocol::AppHandle;

struct SessionWaiter {
    notify: Arc<Notify>,
}

/// Maintains persistent outbound control connections to paired devices.
/// Presence is derived from live connection state instead of polling probes.
pub struct PairedConnectionManager {
    runtime: Arc<OnceCell<Arc<Mutex<crate::runtime::NodeRuntime>>>>,
    identity: Arc<DeviceIdentity>,
    paired_store: Arc<PairedDeviceStore>,
    presence: Arc<std::sync::RwLock<HashMap<String, bool>>>,
    app_handle: AppHandle,
    sessions: Arc<RwLock<HashMap<String, Connection>>>,
    waiters: Arc<Mutex<HashMap<String, SessionWaiter>>>,
    tasks: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
    shutdown: Arc<AtomicBool>,
}

impl PairedConnectionManager {
    pub fn new(
        identity: Arc<DeviceIdentity>,
        paired_store: Arc<PairedDeviceStore>,
        presence: Arc<std::sync::RwLock<HashMap<String, bool>>>,
        app_handle: AppHandle,
    ) -> Self {
        pairing_flow!(
            "connections",
            "internal",
            "manager.new",
            local_endpoint = %identity.endpoint_id()
        );
        Self {
            runtime: Arc::new(OnceCell::new()),
            identity,
            paired_store,
            presence,
            app_handle,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            waiters: Arc::new(Mutex::new(HashMap::new())),
            tasks: Arc::new(Mutex::new(HashMap::new())),
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn attach_runtime(&self, runtime: Arc<Mutex<crate::runtime::NodeRuntime>>) {
        pairing_flow!("connections", "internal", "manager.runtime_attached");
        let _ = self.runtime.set(runtime);
    }

    pub fn start(self: &Arc<Self>) -> JoinHandle<()> {
        pairing_flow!("connections", "internal", "supervisor.start");
        let manager = Arc::clone(self);
        tokio::spawn(async move {
            manager.run_supervisor().await;
        })
    }

    pub async fn refresh(&self) {
        pairing_flow!("connections", "internal", "supervisor.refresh.start");
        let devices = match self.paired_store.list() {
            Ok(devices) => devices,
            Err(err) => {
                log_pairing_flow_error("connections", "internal", "supervisor.refresh.list_failed", &err);
                return;
            }
        };
        let total = devices.len();
        let active: Vec<PairedDevice> = devices
            .into_iter()
            .filter(|d| d.pairing_status.is_connectable())
            .collect();
        let active_ids: std::collections::HashSet<String> = active
            .iter()
            .map(|d| d.endpoint_id.to_lowercase())
            .collect();

        pairing_flow!(
            "connections",
            "internal",
            "supervisor.refresh.devices",
            total = total,
            active = active.len(),
            endpoint_ids = ?active.iter().map(|d| d.endpoint_id.as_str()).collect::<Vec<_>>()
        );

        let mut tasks = self.tasks.lock().await;
        let before = tasks.len();
        tasks.retain(|endpoint_id, handle| {
            if active_ids.contains(endpoint_id) {
                true
            } else {
                pairing_flow!(
                    "connections",
                    "outbound",
                    "task.abort_removed_device",
                    remote = %endpoint_id
                );
                handle.abort();
                false
            }
        });
        if tasks.len() != before {
            pairing_flow!(
                "connections",
                "internal",
                "supervisor.refresh.tasks_pruned",
                before,
                after = tasks.len()
            );
        }

        for device in active {
            let key = device.endpoint_id.to_lowercase();
            if tasks.contains_key(&key) {
                pairing_flow!(
                    "connections",
                    "outbound",
                    "task.already_running",
                    remote = %device.endpoint_id,
                    display_name = %device.display_name
                );
                continue;
            }
            pairing_flow!(
                "connections",
                "outbound",
                "task.spawn",
                remote = %device.endpoint_id,
                display_name = %device.display_name,
                stored_relay = ?device.relay_url
            );
            let handle = self.spawn_connect_task(device);
            tasks.insert(key, handle);
        }
        pairing_flow!(
            "connections",
            "internal",
            "supervisor.refresh.done",
            running_tasks = tasks.len()
        );
    }

    pub async fn forget(&self, endpoint_id: &str) {
        let key = endpoint_id.to_lowercase();
        pairing_flow!(
            "connections",
            "internal",
            "manager.forget",
            remote = %endpoint_id
        );
        if let Some(handle) = self.tasks.lock().await.remove(&key) {
            handle.abort();
            pairing_flow!(
                "connections",
                "outbound",
                "task.abort_forget",
                remote = %endpoint_id
            );
        }
        self.remove_session(&key, "forget").await;
    }

    pub async fn shutdown(&self) {
        pairing_flow!("connections", "internal", "manager.shutdown.start");
        self.shutdown.store(true, Ordering::SeqCst);
        let mut tasks = self.tasks.lock().await;
        let count = tasks.len();
        for (_, handle) in tasks.drain() {
            handle.abort();
        }
        self.sessions.write().await.clear();
        pairing_flow!(
            "connections",
            "internal",
            "manager.shutdown.done",
            aborted_tasks = count
        );
    }

    /// Returns a live connection if one exists, otherwise waits until connected or timeout.
    pub async fn wait_for_connection(
        &self,
        endpoint_id: &str,
        timeout: Duration,
    ) -> Option<Connection> {
        let key = endpoint_id.to_lowercase();
        let remote = endpoint_id.to_string();
        if let Some(conn) = self.sessions.read().await.get(&key).cloned() {
            pairing_flow!(
                "invite",
                "outbound",
                "session.wait.hit",
                remote = %remote,
                conn_side = ?conn.side(),
                peer_role = peer_role_from_side(conn.side()),
                source = "persistent_session"
            );
            return Some(conn);
        }

        let notify = {
            let mut waiters = self.waiters.lock().await;
            waiters
                .entry(key.clone())
                .or_insert_with(|| SessionWaiter {
                    notify: Arc::new(Notify::new()),
                })
                .notify
                .clone()
        };

        let active_sessions = self.sessions.read().await.len();
        pairing_flow!(
            "invite",
            "outbound",
            "session.wait.start",
            remote = %remote,
            timeout_secs = timeout.as_secs(),
            active_sessions
        );
        let wait = async {
            loop {
                if let Some(conn) = self.sessions.read().await.get(&key).cloned() {
                    return Some(conn);
                }
                notify.notified().await;
            }
        };
        match tokio::time::timeout(timeout, wait).await {
            Ok(Some(conn)) => {
                pairing_flow!(
                    "invite",
                    "outbound",
                    "session.wait.ready",
                    remote = %remote,
                    conn_side = ?conn.side(),
                    peer_role = peer_role_from_side(conn.side())
                );
                Some(conn)
            }
            Ok(None) => None,
            Err(_) => {
                let active_sessions = self.sessions.read().await.len();
                pairing_flow_warn!(
                    "invite",
                    "outbound",
                    "session.wait.timeout",
                    remote = %remote,
                    timeout_secs = timeout.as_secs(),
                    active_sessions
                );
                None
            }
        }
    }

    pub async fn register_inbound(&self, endpoint_id: &str, conn: Connection) {
        let key = endpoint_id.to_lowercase();
        let side = conn.side();
        pairing_flow!(
            "connections",
            direction_from_side(side),
            "session.inbound.register",
            remote = %endpoint_id,
            conn_side = ?side,
            peer_role = peer_role_from_side(side),
            local_endpoint = %self.identity.endpoint_id()
        );
        self.set_session(&key, conn, "inbound_accept").await;
    }

    pub async fn unregister_inbound(&self, endpoint_id: &str) {
        let key = endpoint_id.to_lowercase();
        pairing_flow!(
            "connections",
            "inbound",
            "session.inbound.unregister",
            remote = %endpoint_id
        );
        self.remove_session(&key, "inbound_closed").await;
    }

    async fn run_supervisor(self: Arc<Self>) {
        pairing_flow!("connections", "internal", "supervisor.run.start");
        self.refresh().await;
        loop {
            if self.shutdown.load(Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(Duration::from_secs(30)).await;
            if self.shutdown.load(Ordering::SeqCst) {
                break;
            }
            pairing_flow!("connections", "internal", "supervisor.tick");
            self.refresh().await;
        }
        pairing_flow!("connections", "internal", "supervisor.run.stop");
    }

    fn spawn_connect_task(&self, device: PairedDevice) -> JoinHandle<()> {
        let manager = Arc::new(PairedConnectionManager {
            runtime: self.runtime.clone(),
            identity: self.identity.clone(),
            paired_store: self.paired_store.clone(),
            presence: self.presence.clone(),
            app_handle: self.app_handle.clone(),
            sessions: self.sessions.clone(),
            waiters: self.waiters.clone(),
            tasks: self.tasks.clone(),
            shutdown: self.shutdown.clone(),
        });
        let endpoint_id = device.endpoint_id.clone();
        let relay_url = device.relay_url.clone();
        tokio::spawn(async move {
            manager
                .connect_loop(&endpoint_id, relay_url.as_deref())
                .await;
        })
    }

    async fn connect_loop(&self, endpoint_id: &str, stored_relay: Option<&str>) {
        let key = endpoint_id.to_lowercase();
        let mut backoff_secs = PAIRED_RECONNECT_MIN_SECS;
        let mut attempt: u32 = 0;

        pairing_flow!(
            "connections",
            "outbound",
            "loop.start",
            remote = %endpoint_id,
            local_endpoint = %self.identity.endpoint_id(),
            stored_relay = ?stored_relay
        );

        while !self.shutdown.load(Ordering::SeqCst) {
            attempt += 1;
            if !self.device_still_active(endpoint_id) {
                pairing_flow!(
                    "connections",
                    "outbound",
                    "loop.stop",
                    remote = %endpoint_id,
                    attempt,
                    reason = "device_removed_or_unpaired"
                );
                break;
            }

            pairing_flow!(
                "connections",
                "outbound",
                "loop.connect_attempt",
                remote = %endpoint_id,
                attempt,
                backoff_secs
            );
            match self.connect_and_recognize(endpoint_id, stored_relay, attempt).await {
                Ok(conn) => {
                    backoff_secs = PAIRED_RECONNECT_MIN_SECS;
                    let side = conn.side();
                    pairing_flow!(
                        "connections",
                        direction_from_side(side),
                        "loop.connected",
                        remote = %endpoint_id,
                        attempt,
                        conn_side = ?side,
                        peer_role = peer_role_from_side(side)
                    );
                    self.set_session(&key, conn.clone(), "outbound_connect").await;
                    pairing_flow!(
                        "connections",
                        direction_from_side(side),
                        "loop.hold",
                        remote = %endpoint_id,
                        attempt
                    );
                    let close_reason = conn.closed().await;
                    pairing_flow!(
                        "connections",
                        direction_from_side(side),
                        "loop.connection_closed",
                        remote = %endpoint_id,
                        attempt,
                        close_reason = ?close_reason
                    );
                    self.remove_session(&key, "outbound_closed").await;
                }
                Err(err) => {
                    log_pairing_flow_error(
                        "connections",
                        "outbound",
                        "loop.connect_failed",
                        &err,
                    );
                    set_presence(
                        &self.presence,
                        &self.app_handle,
                        &self.paired_store,
                        endpoint_id,
                        false,
                        "connect_loop_failed",
                    );
                }
            }

            if self.shutdown.load(Ordering::SeqCst) {
                pairing_flow!(
                    "connections",
                    "outbound",
                    "loop.stop",
                    remote = %endpoint_id,
                    reason = "shutdown"
                );
                break;
            }
            pairing_flow!(
                "connections",
                "outbound",
                "loop.backoff",
                remote = %endpoint_id,
                sleep_secs = backoff_secs,
                next_attempt = attempt + 1
            );
            tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
            backoff_secs = (backoff_secs * 2).min(PAIRED_RECONNECT_MAX_SECS);
        }

        pairing_flow!(
            "connections",
            "outbound",
            "loop.end",
            remote = %endpoint_id,
            total_attempts = attempt
        );
    }

    async fn connect_and_recognize(
        &self,
        endpoint_id: &str,
        stored_relay: Option<&str>,
        attempt: u32,
    ) -> anyhow::Result<Connection> {
        let remote = EndpointId::from_str(endpoint_id)?;
        let connect_start = std::time::Instant::now();
        let runtime = self
            .runtime
            .get()
            .context("paired connection manager runtime not attached")?;
        pairing_flow!(
            "connections",
            "outbound",
            "connect.dial.start",
            remote = %endpoint_id,
            attempt,
            timeout_secs = PRESENCE_CONNECT_TIMEOUT_SECS
        );
        let conn = {
            let runtime = runtime.lock().await;
            let addr = build_control_connect_addr(
                &runtime.endpoint,
                remote,
                stored_relay,
                "connections",
            );
            tokio::time::timeout(
                Duration::from_secs(PRESENCE_CONNECT_TIMEOUT_SECS),
                runtime.endpoint.connect(addr, CONTROL_ALPN),
            )
            .await
            .context("paired connect timeout")?
            .context("paired connect failed")?
        };
        let side = conn.side();
        pairing_flow!(
            "connections",
            direction_from_side(side),
            "connect.dial.ok",
            remote = %endpoint_id,
            attempt,
            conn_side = ?side,
            peer_role = peer_role_from_side(side),
            connect_ms = elapsed_ms(connect_start)
        );

        pairing_flow!(
            "connections",
            direction_from_side(side),
            "connect.recognition.export_keying",
            remote = %endpoint_id,
            attempt
        );
        let keying = export_connection_keying_material(&conn).context("export keying")?;
        pairing_flow!(
            "connections",
            direction_from_side(side),
            "connect.recognition.open_bi",
            remote = %endpoint_id,
            attempt
        );
        let (mut send, _recv) = conn
            .open_bi()
            .await
            .context("open bi for recognition")?;
        let recognition = ControlMessage::Recognition {
            signature: sign_challenge(&self.identity.secret_key, &keying),
        };
        write_message(&mut send, &recognition)
            .await
            .context("write recognition")?;
        pairing_flow!(
            "connections",
            direction_from_side(side),
            "connect.recognition.sent",
            remote = %endpoint_id,
            attempt,
            local_endpoint = %self.identity.endpoint_id()
        );
        Ok(conn)
    }

    fn device_still_active(&self, endpoint_id: &str) -> bool {
        self.paired_store
            .get(endpoint_id)
            .ok()
            .flatten()
            .is_some_and(|d| d.pairing_status.is_active())
    }

    async fn set_session(&self, key: &str, conn: Connection, reason: &str) {
        let side = conn.side();
        {
            let mut sessions = self.sessions.write().await;
            let replaced = sessions.insert(key.to_string(), conn).is_some();
            pairing_flow!(
                "connections",
                direction_from_side(side),
                "session.store",
                remote = %key,
                conn_side = ?side,
                replaced,
                session_count = sessions.len(),
                reason
            );
        }
        set_presence(
            &self.presence,
            &self.app_handle,
            &self.paired_store,
            key,
            true,
            reason,
        );
        let now = protocol::identity::unix_now_ms();
        if let Err(err) = self.paired_store.touch(key, now) {
            pairing_flow_warn!(
                "connections",
                "internal",
                "session.touch_failed",
                remote = %key,
                error = %err
            );
        }
        self.notify_waiters(key).await;
    }

    async fn remove_session(&self, key: &str, reason: &str) {
        let removed = self.sessions.write().await.remove(key).is_some();
        if removed {
            pairing_flow!(
                "connections",
                "internal",
                "session.remove",
                remote = %key,
                reason
            );
            set_presence(
                &self.presence,
                &self.app_handle,
                &self.paired_store,
                key,
                false,
                reason,
            );
        } else {
            pairing_flow!(
                "connections",
                "internal",
                "session.remove_skipped",
                remote = %key,
                reason,
                hint = "no_active_session"
            );
        }
    }

    async fn notify_waiters(&self, key: &str) {
        let mut waiters = self.waiters.lock().await;
        if let Some(waiter) = waiters.remove(key) {
            pairing_flow!(
                "invite",
                "outbound",
                "session.wait.notify",
                remote = %key
            );
            waiter.notify.notify_waiters();
        }
    }
}

pub fn invite_wait_timeout() -> Duration {
    Duration::from_secs(PAIRED_INVITE_WAIT_SECS)
}
