use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use iroh::endpoint::{
    presets, AfterHandshakeOutcome, Connection, Endpoint, EndpointHooks, RelayMode, Side,
};
use iroh::protocol::{AcceptError, ProtocolHandler, Router};
use iroh::{address_lookup::pkarr::PkarrPublisher, EndpointAddr, EndpointId, TransportAddr};
use protocol::{
    apply_options, export_connection_keying_material, read_message, sign_challenge,
    verify_challenge, write_message, AddrInfoOptions, AppHandle, ControlMessage, InviteResponse,
    PairedDevice, PairingStatus, RememberVote, CONTROL_ALPN, PRESENCE_CONNECT_TIMEOUT_SECS,
};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tracing::debug;

use crate::device_identity::{
    load_or_create_identity, DeviceIdentity, DeviceInfo, PairedDeviceInfo, PairedDeviceStore,
};
use crate::paired_connections::{invite_wait_timeout, PairedConnectionManager};
use crate::pairing_dev_log::{
    direction_from_side, elapsed_ms, format_connect_addr, log_pairing_error,
    log_pairing_flow_error, peer_role_from_side,
};
use crate::pairing_util::{build_control_connect_addr, control_message_kind, set_presence};
use crate::runtime::NodeRuntime;
use crate::{pairing_dev, pairing_dev_warn, pairing_flow, pairing_flow_warn};

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
            pairing_dev!("hook.skip", side = ?conn.side(), reason = "not_server");
            return AfterHandshakeOutcome::accept();
        }
        if conn.alpn() != CONTROL_ALPN {
            pairing_dev!(
                "hook.skip",
                alpn = ?String::from_utf8_lossy(conn.alpn()),
                reason = "not_control_alpn"
            );
            return AfterHandshakeOutcome::accept();
        }
        let remote = conn.remote_id();
        let access = self.access.read().await;
        let allowed = access.allowed.contains(&remote);
        if access.pairing_host_open || allowed {
            pairing_flow!(
                "control",
                direction_from_side(conn.side()),
                "hook.accept",
                remote = %remote,
                conn_side = ?conn.side(),
                peer_role = peer_role_from_side(conn.side()),
                pairing_host_open = access.pairing_host_open,
                peer_in_allowlist = allowed,
                allowlist_size = access.allowed.len()
            );
            return AfterHandshakeOutcome::accept();
        }
        pairing_flow_warn!(
            "control",
            direction_from_side(conn.side()),
            "hook.reject",
            remote = %remote,
            conn_side = ?conn.side(),
            pairing_host_open = access.pairing_host_open,
            allowlist_size = access.allowed.len(),
            allowlist = ?access.allowed.iter().map(|id| id.to_string()).collect::<Vec<_>>(),
            hint = "peer may be using a new endpoint_id after identity rotation; re-pair required"
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
    pairing_host_persistent: Arc<AtomicBool>,
    app_handle: AppHandle,
    home_relay_url: Option<String>,
    presence: Arc<std::sync::RwLock<HashMap<String, bool>>>,
    paired_connections: Arc<PairedConnectionManager>,
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
        let session_start = Instant::now();
        let remote = conn.remote_id();
        let local = self.ctx.identity.endpoint_id();
        let conn_side = conn.side();
        let pairing_host_open = self.ctx.access.read().await.pairing_host_open;
        let allowed = self.is_allowed(&remote).await;
        let in_store = self.is_in_paired_store(&remote).await;

        if allowed {
            pairing_flow!(
                "control",
                direction_from_side(conn_side),
                "session.route.paired",
                remote = %remote,
                local = %local,
                conn_side = ?conn_side,
                peer_role = peer_role_from_side(conn_side)
            );
            return self.handle_paired_peer_connection(conn).await;
        }

        if !pairing_host_open {
            pairing_flow_warn!(
                "control",
                direction_from_side(conn_side),
                "session.reject_unauthorized",
                remote = %remote,
                local = %local,
                conn_side = ?conn_side,
                peer_in_paired_store = in_store,
                hint = "not_in_allowlist_and_pairing_host_closed"
            );
            return Ok(());
        }

        pairing_flow!(
            "pairing",
            direction_from_side(conn_side),
            "session.start",
            remote = %remote,
            local = %local,
            conn_side = ?conn_side,
            peer_role = peer_role_from_side(conn_side),
            home_relay = ?self.ctx.home_relay_url,
            pairing_host_open,
            peer_in_paired_store = in_store
        );

        pairing_dev!("control.session.export_keying", remote = %remote);
        let keying = export_connection_keying_material(&conn).context("export keying material")?;

        pairing_dev!("control.session.accept_bi", remote = %remote);
        let bi_start = Instant::now();
        let (mut send, mut recv) = match conn.accept_bi().await {
            Ok(streams) => {
                pairing_dev!(
                    "control.session.bi_ready",
                    remote = %remote,
                    accept_bi_ms = elapsed_ms(bi_start)
                );
                streams
            }
            Err(err) => {
                log_pairing_error("control.session.accept_bi_failed", &err);
                return Err(err).context("accept bi stream for control session");
            }
        };

        let our_info = ControlMessage::PairingInfo {
            endpoint_id: self.ctx.identity.endpoint_id(),
            display_name: self.ctx.identity.display_name(),
            device_type: self.ctx.identity.device_type(),
            os: self.ctx.identity.os(),
            signature: sign_challenge(&self.ctx.identity.secret_key, &keying),
        };
        write_message(&mut send, &our_info)
            .await
            .context("write local PairingInfo")?;
        pairing_dev!(
            "control.session.sent_pairing_info",
            remote = %remote,
            local = %local,
            display_name = %self.ctx.identity.display_name(),
            os = %self.ctx.identity.os()
        );

        let mut remote_info: Option<ControlMessage> = None;
        let mut remote_vote: Option<RememberVote> = None;
        let mut pairing_completed = false;
        let mut invite_received = false;
        let session_id = uuid::Uuid::new_v4().to_string();

        loop {
            let msg = match read_message(&mut recv).await {
                Ok(m) => m,
                Err(err) => {
                    pairing_dev!(
                        "control.session.read_end",
                        remote = %remote,
                        error = %err,
                        has_remote_pairing_info = remote_info.is_some(),
                        has_remote_remember_vote = remote_vote.is_some()
                    );
                    break;
                }
            };
            match &msg {
                ControlMessage::PairingInfo { .. } => {
                    pairing_dev!("control.session.msg", remote = %remote, kind = "PairingInfo");
                }
                ControlMessage::RememberVote { vote, .. } => {
                    pairing_dev!("control.session.msg", remote = %remote, kind = "RememberVote", ?vote);
                }
                ControlMessage::Invite { file_count, total_size, sender_name, blob_ticket, .. } => {
                    pairing_dev!(
                        "control.session.msg",
                        remote = %remote,
                        kind = "Invite",
                        file_count,
                        total_size,
                        sender_name = %sender_name,
                        ticket_len = blob_ticket.len()
                    );
                }
                ControlMessage::InviteResponse { .. } => {
                    pairing_dev!("control.session.msg", remote = %remote, kind = "InviteResponse");
                }
                ControlMessage::Recognition { .. } => {
                    pairing_dev!("control.session.msg", remote = %remote, kind = "Recognition");
                }
                ControlMessage::Forget { .. } => {
                    pairing_dev!("control.session.msg", remote = %remote, kind = "Forget");
                }
            }
            match msg {
                ControlMessage::PairingInfo {
                    endpoint_id,
                    display_name,
                    device_type,
                    os,
                    signature,
                } => {
                    let Ok(peer_id) = EndpointId::from_str(&endpoint_id) else {
                        pairing_dev_warn!(
                            "control.session.pairing_info_invalid_id",
                            remote = %remote,
                            peer_id = %endpoint_id
                        );
                        continue;
                    };
                    if !verify_challenge(&peer_id, &keying, &signature) {
                        pairing_dev_warn!(
                            "control.session.pairing_info_bad_sig",
                            remote = %remote,
                            peer_id = %endpoint_id
                        );
                        continue;
                    }
                    pairing_dev!(
                        "control.session.pairing_info_ok",
                        remote = %remote,
                        peer_id = %endpoint_id,
                        display_name = %display_name,
                        device_type = %device_type,
                        os = %os
                    );
                    remote_info = Some(ControlMessage::PairingInfo {
                        endpoint_id,
                        display_name,
                        device_type,
                        os,
                        signature,
                    });
                }
                ControlMessage::RememberVote { vote, .. } => {
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
                    pairing_dev!(
                        "invite.received",
                        remote = %remote,
                        peer_in_allowlist = allowed,
                        peer_in_paired_store = in_store,
                        file_count,
                        total_size,
                        sender_name = %sender_name,
                        ticket_len = blob_ticket.len()
                    );
                    if !allowed {
                        pairing_dev_warn!(
                            "invite.dropped_not_allowed",
                            remote = %remote,
                            peer_in_paired_store = in_store
                        );
                        continue;
                    }
                    invite_received = true;
                    let payload = serde_json::json!({
                        "blob_ticket": blob_ticket,
                        "file_count": file_count,
                        "total_size": total_size,
                        "sender_name": sender_name,
                        "remote_endpoint_id": remote.to_string(),
                    });
                    if let Some(handle) = &self.ctx.app_handle {
                        pairing_dev!(
                            "invite.emit_ui",
                            remote = %remote,
                            event = "paired-invite-received",
                            payload_len = payload.to_string().len()
                        );
                        match handle.emit_event_with_payload(
                            "paired-invite-received",
                            &payload.to_string(),
                        ) {
                            Ok(()) => {
                                pairing_dev!(
                                    "invite.emit_ui_ok",
                                    remote = %remote,
                                    event = "paired-invite-received"
                                );
                            }
                            Err(err) => {
                                pairing_dev_warn!(
                                    "invite.emit_ui_failed",
                                    remote = %remote,
                                    error = %err
                                );
                            }
                        }
                    } else {
                        pairing_dev_warn!(
                            "invite.emit_ui_skipped",
                            remote = %remote,
                            reason = "no_app_handle"
                        );
                    }
                }
                ControlMessage::InviteResponse { response, .. } => {
                    let response_str = match response {
                        InviteResponse::Accepted => "accepted",
                        InviteResponse::Declined => "declined",
                    };
                    debug!(?response, "invite response from {remote}");
                    crate::pairing_util::emit_paired_invite_response(
                        &self.ctx.app_handle,
                        &self.ctx.paired_store,
                        &remote.to_string(),
                        response_str,
                    );
                }
                ControlMessage::Recognition { signature } => {
                    if verify_challenge(&remote, &keying, &signature) {
                        pairing_dev!("control.session.recognition_ok", remote = %remote);
                        let now = protocol::identity::unix_now_ms();
                        let _ = self.ctx.paired_store.touch(&remote.to_string(), now);
                        set_presence(
                            &self.ctx.presence,
                            &self.ctx.app_handle,
                            &self.ctx.paired_store,
                            &remote.to_string(),
                            true,
                            "recognition_pairing_host",
                        );
                    } else {
                        pairing_dev_warn!("control.session.recognition_bad_sig", remote = %remote);
                    }
                }
                ControlMessage::Forget { signature } => {
                    if verify_challenge(&remote, &keying, &signature) {
                        pairing_dev!("control.session.forget_ok", remote = %remote);
                        if let Ok(Some(device)) = self
                            .ctx
                            .paired_store
                            .mark_unpaired_remotely(&remote.to_string())
                        {
                            {
                                let mut access = self.ctx.access.write().await;
                                access.allowed.remove(&remote);
                            }
                            set_presence(
                                &self.ctx.presence,
                                &self.ctx.app_handle,
                                &self.ctx.paired_store,
                                &remote.to_string(),
                                false,
                                "forget_pairing_host",
                            );
                            let payload = serde_json::json!({
                                "endpoint_id": device.endpoint_id,
                                "display_name": device.display_name,
                                "reason": "remote",
                            });
                            if let Some(handle) = &self.ctx.app_handle {
                                let _ = handle.emit_event_with_payload(
                                    "device-unpaired",
                                    &payload.to_string(),
                                );
                            }
                        }
                    } else {
                        pairing_dev_warn!("control.session.forget_bad_sig", remote = %remote);
                    }
                }
            }

            if remote_info.is_some() && remote_vote == Some(RememberVote::Remember) {
                pairing_dev!(
                    "pair.complete.handshake",
                    remote = %remote,
                    role = "host"
                );
                if let Some(ControlMessage::PairingInfo {
                    endpoint_id,
                    display_name,
                    device_type,
                    os,
                    ..
                }) = &remote_info
                {
                    let now = protocol::identity::unix_now_ms();
                    let device = PairedDevice {
                        endpoint_id: endpoint_id.clone(),
                        display_name: display_name.clone(),
                        device_type: device_type.clone(),
                        os: os.clone(),
                        paired_at: now,
                        last_seen_at: now,
                        relay_url: self.ctx.home_relay_url.clone(),
                        pairing_status: PairingStatus::Active,
                    };
                    let _ = self.ctx.paired_store.remember(device);
                    self.allow_peer(remote).await;
                    self.ctx.paired_connections.refresh().await;
                    pairing_dev!(
                        "pair.complete.stored",
                        remote = %remote,
                        peer_id = %endpoint_id,
                        display_name = %display_name,
                        role = "host",
                        relay_url = ?self.ctx.home_relay_url
                    );
                    if let Some(handle) = &self.ctx.app_handle {
                        pairing_dev!("pair.emit_ui", event = "device-paired", role = "host");
                        let _ = handle.emit_event("device-paired");
                    }
                }
                pairing_completed = true;
                break;
            }
        }

        if remote_info.is_some() {
            pairing_dev!(
                "control.session.send_remember_vote",
                remote = %remote,
                vote = ?RememberVote::Remember
            );
            let vote = ControlMessage::RememberVote {
                session_id,
                vote: RememberVote::Remember,
            };
            if let Err(err) = write_message(&mut send, &vote).await {
                log_pairing_error("control.session.send_remember_vote_failed", &err);
            } else {
                pairing_dev!("control.session.send_remember_vote_ok", remote = %remote);
            }
        }

        if pairing_completed {
            let persistent = self
                .ctx
                .pairing_host_persistent
                .load(Ordering::SeqCst);
            if persistent {
                pairing_dev!(
                    "host.stay_open",
                    local_endpoint = %local,
                    reason = "persistent_pairing"
                );
            } else {
                self.ctx.access.write().await.pairing_host_open = false;
                pairing_dev!("host.close", local_endpoint = %local, reason = "pairing_complete");
            }
            // Hold the session until the joiner reads our messages and disconnects.
            drop(send);
            drop(recv);
            pairing_dev!("control.session.wait_joiner", remote = %remote, timeout_secs = 30);
            match tokio::time::timeout(Duration::from_secs(30), conn.closed()).await {
                Ok(closed) => pairing_dev!(
                    "control.session.joiner_closed",
                    remote = %remote,
                    close_reason = ?closed
                ),
                Err(_) => {
                    pairing_dev_warn!("control.session.joiner_wait_timeout", remote = %remote)
                }
            }
        } else if invite_received {
            // Keep the session open until the sender finishes reading our side.
            drop(send);
            drop(recv);
            pairing_dev!("control.session.wait_invite_sender", remote = %remote, timeout_secs = 15);
            match tokio::time::timeout(Duration::from_secs(15), conn.closed()).await {
                Ok(closed) => pairing_dev!(
                    "control.session.invite_sender_closed",
                    remote = %remote,
                    close_reason = ?closed
                ),
                Err(_) => {
                    pairing_dev_warn!(
                        "control.session.invite_sender_wait_timeout",
                        remote = %remote
                    )
                }
            }
        }

        pairing_dev!(
            "control.session.finish",
            remote = %remote,
            session_ms = elapsed_ms(session_start),
            pairing_completed,
            invite_received,
            had_remote_pairing_info = remote_info.is_some(),
            had_remote_remember_vote = remote_vote.is_some(),
            peer_was_allowlisted = false
        );
        Ok(())
    }

    async fn handle_paired_peer_connection(&self, conn: Connection) -> anyhow::Result<()> {
        let session_start = Instant::now();
        let remote = conn.remote_id();
        let local = self.ctx.identity.endpoint_id();
        let endpoint_id = remote.to_string();
        let conn_side = conn.side();
        pairing_flow!(
            "control",
            direction_from_side(conn_side),
            "session.paired.start",
            remote = %remote,
            local = %local,
            conn_side = ?conn_side,
            peer_role = peer_role_from_side(conn_side),
            home_relay = ?self.ctx.home_relay_url,
            role = "receiver"
        );

        self.ctx
            .paired_connections
            .register_inbound(&endpoint_id, conn.clone())
            .await;

        pairing_flow!(
            "control",
            direction_from_side(conn_side),
            "session.paired.export_keying",
            remote = %remote,
            role = "receiver"
        );
        let keying = match export_connection_keying_material(&conn) {
            Ok(keying) => keying,
            Err(err) => {
                log_pairing_flow_error(
                    "control",
                    direction_from_side(conn_side),
                    "session.paired.keying_failed",
                    &err,
                );
                self.ctx
                    .paired_connections
                    .unregister_inbound(&endpoint_id)
                    .await;
                return Err(err).context("export keying material for paired session");
            }
        };

        let mut bi_index: u32 = 0;
        loop {
            bi_index += 1;
            pairing_flow!(
                "control",
                direction_from_side(conn_side),
                "session.paired.accept_bi.wait",
                remote = %remote,
                bi_index,
                role = "receiver"
            );
            let (_send, mut recv) = match conn.accept_bi().await {
                Ok(streams) => {
                    pairing_flow!(
                        "control",
                        direction_from_side(conn_side),
                        "session.paired.accept_bi.ok",
                        remote = %remote,
                        bi_index,
                        role = "receiver"
                    );
                    streams
                }
                Err(err) => {
                    pairing_flow!(
                        "control",
                        direction_from_side(conn_side),
                        "session.paired.accept_bi.end",
                        remote = %remote,
                        bi_index,
                        error = %err,
                        role = "receiver"
                    );
                    break;
                }
            };

            pairing_flow!(
                "control",
                direction_from_side(conn_side),
                "session.paired.read.wait",
                remote = %remote,
                bi_index,
                role = "receiver"
            );
            let msg = match read_message(&mut recv).await {
                Ok(m) => m,
                Err(err) => {
                    pairing_flow!(
                        "control",
                        direction_from_side(conn_side),
                        "session.paired.read.failed",
                        remote = %remote,
                        bi_index,
                        error = %err,
                        role = "receiver"
                    );
                    continue;
                }
            };

            pairing_flow!(
                "control",
                direction_from_side(conn_side),
                "session.paired.msg.received",
                remote = %remote,
                bi_index,
                kind = control_message_kind(&msg),
                role = "receiver"
            );
            let unpaired = self
                .handle_paired_control_message(&remote, &keying, msg, conn_side, bi_index)
                .await;
            if unpaired {
                // Close so the sender's delivery wait resolves promptly.
                conn.close(0u32.into(), b"unpaired");
                break;
            }
        }

        self.ctx
            .paired_connections
            .unregister_inbound(&endpoint_id)
            .await;
        pairing_flow!(
            "control",
            direction_from_side(conn_side),
            "session.paired.finish",
            remote = %remote,
            bi_streams_handled = bi_index,
            session_ms = elapsed_ms(session_start),
            role = "receiver"
        );
        Ok(())
    }

    /// Returns true when the peer unpaired us and the session should close.
    async fn handle_paired_control_message(
        &self,
        remote: &EndpointId,
        keying: &[u8],
        msg: ControlMessage,
        conn_side: Side,
        bi_index: u32,
    ) -> bool {
        match msg {
            ControlMessage::Invite {
                blob_ticket,
                file_count,
                total_size,
                sender_name,
            } => {
                pairing_flow!(
                    "invite",
                    direction_from_side(conn_side),
                    "invite.received",
                    remote = %remote,
                    bi_index,
                    file_count,
                    total_size,
                    sender_name = %sender_name,
                    ticket_len = blob_ticket.len(),
                    role = "receiver"
                );
                crate::pairing_util::emit_paired_invite_received(
                    &self.ctx.app_handle,
                    &remote.to_string(),
                    &blob_ticket,
                    file_count,
                    total_size,
                    &sender_name,
                );
            }
            ControlMessage::InviteResponse { response, .. } => {
                let response_str = match response {
                    InviteResponse::Accepted => "accepted",
                    InviteResponse::Declined => "declined",
                };
                pairing_flow!(
                    "invite",
                    direction_from_side(conn_side),
                    "invite.response.received",
                    remote = %remote,
                    bi_index,
                    response = %response_str,
                    role = "sender"
                );
                crate::pairing_util::emit_paired_invite_response(
                    &self.ctx.app_handle,
                    &self.ctx.paired_store,
                    &remote.to_string(),
                    response_str,
                );
            }
            ControlMessage::Recognition { signature } => {
                pairing_flow!(
                    "connections",
                    direction_from_side(conn_side),
                    "recognition.received",
                    remote = %remote,
                    bi_index,
                    role = "receiver"
                );
                if verify_challenge(remote, keying, &signature) {
                    pairing_flow!(
                        "connections",
                        direction_from_side(conn_side),
                        "recognition.verified",
                        remote = %remote,
                        role = "receiver"
                    );
                    let now = protocol::identity::unix_now_ms();
                    let _ = self
                        .ctx
                        .paired_store
                        .touch(&remote.to_string(), now);
                    set_presence(
                        &self.ctx.presence,
                        &self.ctx.app_handle,
                        &self.ctx.paired_store,
                        &remote.to_string(),
                        true,
                        "recognition_inbound",
                    );
                } else {
                    pairing_flow_warn!(
                        "connections",
                        direction_from_side(conn_side),
                        "recognition.bad_signature",
                        remote = %remote,
                        role = "receiver"
                    );
                }
            }
            ControlMessage::Forget { signature } => {
                pairing_flow!(
                    "forget",
                    direction_from_side(conn_side),
                    "forget.received",
                    remote = %remote,
                    bi_index,
                    role = "receiver"
                );
                if verify_challenge(remote, keying, &signature) {
                    pairing_flow!(
                        "forget",
                        direction_from_side(conn_side),
                        "forget.verified",
                        remote = %remote,
                        role = "receiver"
                    );
                    let marked = self
                        .ctx
                        .paired_store
                        .mark_unpaired_remotely(&remote.to_string());
                    if let Ok(Some(device)) = marked {
                        {
                            let mut access = self.ctx.access.write().await;
                            access.allowed.remove(remote);
                        }
                        set_presence(
                            &self.ctx.presence,
                            &self.ctx.app_handle,
                            &self.ctx.paired_store,
                            &remote.to_string(),
                            false,
                            "forget_inbound",
                        );
                        self.ctx.paired_connections.forget(&remote.to_string()).await;
                        let payload = serde_json::json!({
                            "endpoint_id": device.endpoint_id,
                            "display_name": device.display_name,
                            "reason": "remote",
                        });
                        if let Some(handle) = &self.ctx.app_handle {
                            pairing_flow!(
                                "forget",
                                "inbound",
                                "forget.emit_ui",
                                remote = %remote,
                                event = "device-unpaired"
                            );
                            let _ = handle.emit_event_with_payload(
                                "device-unpaired",
                                &payload.to_string(),
                            );
                        }
                    }
                    return true;
                } else {
                    pairing_flow_warn!(
                        "forget",
                        direction_from_side(conn_side),
                        "forget.bad_signature",
                        remote = %remote,
                        role = "receiver"
                    );
                }
            }
            other => {
                pairing_flow_warn!(
                    "control",
                    direction_from_side(conn_side),
                    "session.paired.unexpected_msg",
                    remote = %remote,
                    bi_index,
                    kind = control_message_kind(&other),
                    role = "receiver"
                );
            }
        }
        false
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
        let allowlist_size = {
            let mut access = self.ctx.access.write().await;
            access.allowed.insert(remote);
            access.allowed.len()
        };
        pairing_dev!(
            "allowlist.add",
            remote = %remote,
            allowlist_size
        );
    }
}

impl ProtocolHandler for ControlProtocol {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let this = self.clone();
        let remote = connection.remote_id();
        let side = connection.side();
        pairing_flow!(
            "control",
            direction_from_side(side),
            "session.incoming",
            remote = %remote,
            conn_side = ?side,
            peer_role = peer_role_from_side(side),
            local = %this.ctx.identity.endpoint_id()
        );
        tokio::spawn(async move {
            if let Err(err) = this.handle_connection(connection).await {
                log_pairing_flow_error("control", "inbound", "session.error", &err);
            }
        });
        pairing_flow!(
            "control",
            direction_from_side(side),
            "session.spawned",
            remote = %remote
        );
        Ok(())
    }
}

pub struct NodeService {
    runtime: Arc<Mutex<NodeRuntime>>,
    identity: Arc<DeviceIdentity>,
    paired_store: Arc<PairedDeviceStore>,
    access: Arc<RwLock<AccessState>>,
    pairing_host_open: Arc<AtomicBool>,
    pairing_host_persistent: Arc<AtomicBool>,
    pairing_expire_task: Mutex<Option<JoinHandle<()>>>,
    paired_connections: Arc<PairedConnectionManager>,
    connections_supervisor: Mutex<Option<JoinHandle<()>>>,
    presence: Arc<std::sync::RwLock<HashMap<String, bool>>>,
    app_handle: AppHandle,
    relay_mode: Mutex<RelayMode>,
}

impl NodeService {
    pub async fn start(
        data_dir: &Path,
        relay_mode: RelayMode,
        app_handle: AppHandle,
    ) -> anyhow::Result<Self> {
        pairing_dev!("node.init.start", data_dir = %data_dir.display());
        let identity = Arc::new(load_or_create_identity(data_dir)?);
        let paired_store = Arc::new(PairedDeviceStore::new(data_dir));
        let allowed = load_allowed_from_store(&paired_store)?;
        let paired_list = paired_store.list().unwrap_or_default();
        pairing_dev!(
            "node.init.identity",
            local_endpoint = %identity.endpoint_id(),
            display_name = %identity.display_name(),
            device_type = %identity.device_type(),
            os = %identity.os(),
            allowlist_size = allowed.len(),
            stored_devices = paired_list.len()
        );
        for device in &paired_list {
            pairing_dev!(
                "node.init.stored_device",
                endpoint_id = %device.endpoint_id,
                display_name = %device.display_name
            );
        }
        if identity.identity_rotated {
            let stale_count = paired_store
                .mark_stale_after_local_identity_rotation()
                .unwrap_or(0);
            pairing_dev_warn!(
                "node.init.identity_rotated",
                previous_endpoint = ?identity.previous_endpoint_id,
                current_endpoint = %identity.endpoint_id(),
                paired_device_count = paired_list.len(),
                stale_device_count = stale_count,
                hint = "peers paired with the previous endpoint_id cannot reach this device until re-paired"
            );
            if stale_count > 0 {
                if let Some(handle) = &app_handle {
                    let payload = serde_json::json!({
                        "previous_endpoint_id": identity.previous_endpoint_id,
                        "current_endpoint_id": identity.endpoint_id(),
                        "stale_device_count": stale_count,
                    });
                    pairing_dev!("node.emit_ui", event = "identity-rotated");
                    let _ = handle.emit_event_with_payload(
                        "identity-rotated",
                        &payload.to_string(),
                    );
                }
            }
        }
        let allowlist_ids: Vec<String> = allowed.iter().map(|id| id.to_string()).collect();
        pairing_dev!(
            "node.init.allowlist",
            local_endpoint = %identity.endpoint_id(),
            allowlist = ?allowlist_ids
        );

        let access = Arc::new(RwLock::new(AccessState {
            allowed: allowed.clone(),
            pairing_host_open: false,
        }));
        let pairing_host_open = Arc::new(AtomicBool::new(false));
        let pairing_host_persistent = Arc::new(AtomicBool::new(false));
        let presence = Arc::new(std::sync::RwLock::new(HashMap::new()));

        let paired_connections = Arc::new(PairedConnectionManager::new(
            identity.clone(),
            paired_store.clone(),
            presence.clone(),
            app_handle.clone(),
        ));

        let runtime = build_runtime(
            identity.clone(),
            paired_store.clone(),
            access.clone(),
            pairing_host_persistent.clone(),
            app_handle.clone(),
            presence.clone(),
            paired_connections.clone(),
            relay_mode.clone(),
        )
        .await?;
        let runtime = Arc::new(Mutex::new(runtime));
        paired_connections.attach_runtime(runtime.clone());
        let connections_supervisor = paired_connections.start();

        pairing_dev!(
            "node.init.ready",
            local_endpoint = %identity.endpoint_id(),
            allowlist_size = allowed.len()
        );

        Ok(Self {
            runtime,
            identity,
            paired_store,
            access,
            pairing_host_open,
            pairing_host_persistent,
            pairing_expire_task: Mutex::new(None),
            paired_connections,
            connections_supervisor: Mutex::new(Some(connections_supervisor)),
            presence,
            app_handle,
            relay_mode: Mutex::new(relay_mode),
        })
    }

    pub async fn shutdown(&self) -> anyhow::Result<()> {
        pairing_dev!("node.shutdown.start", local_endpoint = %self.identity.endpoint_id());
        self.stop_pairing_host().await;
        if let Some(handle) = self.connections_supervisor.lock().await.take() {
            handle.abort();
            pairing_dev!("connections.supervisor_aborted", local_endpoint = %self.identity.endpoint_id());
        }
        self.paired_connections.shutdown().await;
        let runtime = self.runtime.lock().await;
        runtime.router.shutdown().await?;
        runtime.endpoint.close().await;
        pairing_dev!("node.shutdown.done", local_endpoint = %self.identity.endpoint_id());
        Ok(())
    }

    pub async fn reconfigure_relay(&self, relay_mode: RelayMode) -> anyhow::Result<()> {
        {
            let current = self.relay_mode.lock().await;
            if format!("{current:?}") == format!("{relay_mode:?}") {
                pairing_dev!(
                    "node.relay.skip",
                    local_endpoint = %self.identity.endpoint_id(),
                    reason = "unchanged"
                );
                return Ok(());
            }
            pairing_dev!(
                "node.relay.reconfigure",
                local_endpoint = %self.identity.endpoint_id(),
                from = ?*current,
                to = ?relay_mode
            );
        }

        self.stop_pairing_host().await;

        let mut runtime = self.runtime.lock().await;
        runtime.router.shutdown().await?;
        runtime.endpoint.close().await;

        let new_runtime = build_runtime(
            self.identity.clone(),
            self.paired_store.clone(),
            self.access.clone(),
            self.pairing_host_persistent.clone(),
            self.app_handle.clone(),
            self.presence.clone(),
            self.paired_connections.clone(),
            relay_mode.clone(),
        )
        .await?;

        *runtime = new_runtime;
        self.paired_connections.refresh().await;
        *self.relay_mode.lock().await = relay_mode;
        pairing_dev!("node.relay.reconfigure_done", local_endpoint = %self.identity.endpoint_id());
        Ok(())
    }

    pub fn device_info(&self) -> DeviceInfo {
        DeviceInfo::from(self.identity.as_ref())
    }

    pub fn set_device_display_name(&self, display_name: &str) -> anyhow::Result<DeviceInfo> {
        let info = self.identity.set_display_name(display_name)?;
        pairing_dev!(
            "identity.rename",
            endpoint_id = %info.endpoint_id,
            display_name = %info.display_name
        );
        Ok(info)
    }

    pub fn rename_paired(
        &self,
        endpoint_id: &str,
        display_name: &str,
    ) -> anyhow::Result<PairedDevice> {
        let device = self.paired_store.rename(endpoint_id, display_name)?;
        pairing_dev!(
            "store.rename",
            endpoint_id = %device.endpoint_id,
            display_name = %device.display_name
        );
        Ok(device)
    }

    pub fn list_paired(&self) -> anyhow::Result<Vec<PairedDeviceInfo>> {
        let devices = self.paired_store.list()?;
        let presence = self.presence.read().expect("presence lock");
        let infos: Vec<PairedDeviceInfo> = devices
            .into_iter()
            .map(|device| {
                let online = presence
                    .get(&device.endpoint_id.to_lowercase())
                    .or_else(|| presence.get(&device.endpoint_id))
                    .copied()
                    .unwrap_or(false);
                PairedDeviceInfo::from_device(device, online)
            })
            .collect();
        pairing_dev!(
            "store.list",
            count = infos.len(),
            endpoint_ids = ?infos.iter().map(|d| d.endpoint_id.as_str()).collect::<Vec<_>>()
        );
        Ok(infos)
    }

    pub async fn forget_paired(&self, endpoint_id: &str) -> anyhow::Result<()> {
        pairing_dev!("store.forget.start", endpoint_id = %endpoint_id);
        let stored_relay = self
            .paired_store
            .get(endpoint_id)?
            .and_then(|d| d.relay_url);
        if let Ok(id) = EndpointId::from_str(endpoint_id) {
            self.access.write().await.allowed.remove(&id);
            let allowlist_size = self.access.read().await.allowed.len();
            pairing_dev!(
                "allowlist.remove",
                endpoint_id = %endpoint_id,
                allowlist_size
            );
        }
        self.paired_store.forget(endpoint_id)?;
        self.paired_connections.forget(endpoint_id).await;
        set_presence(
            &self.presence,
            &self.app_handle,
            &self.paired_store,
            endpoint_id,
            false,
            "forget_local",
        );
        if let Some(handle) = &self.app_handle {
            let payload = serde_json::json!({
                "endpoint_id": endpoint_id,
                "reason": "local",
            });
            let _ = handle.emit_event_with_payload("device-unpaired", &payload.to_string());
        }
        pairing_dev!("store.forget.done", endpoint_id = %endpoint_id);

        let runtime = self.runtime.clone();
        let identity = self.identity.clone();
        let endpoint_id = endpoint_id.to_string();
        tokio::spawn(async move {
            if let Err(err) = send_forget_to_peer(
                &runtime,
                &identity,
                &endpoint_id,
                stored_relay.as_deref(),
            )
            .await
            {
                log_pairing_error("store.forget.notify_failed", &err);
            }
        });
        Ok(())
    }

    pub async fn pairing_ticket(&self) -> anyhow::Result<String> {
        pairing_dev!("host.ticket.build", local_endpoint = %self.identity.endpoint_id());
        let runtime = self.runtime.lock().await;
        let mut addr = runtime.endpoint.addr();
        apply_options(&mut addr, AddrInfoOptions::Relay);
        let relay_url = addr.relay_urls().next().map(|u| u.to_string());
        let ticket = protocol::PairingTicket {
            v: 1,
            kind: protocol::PairingTicket::KIND.to_string(),
            endpoint_id: self.identity.endpoint_id(),
            relay_url: relay_url.clone(),
        };
        let encoded = ticket.encode()?;
        pairing_dev!(
            "host.ticket.ready",
            local_endpoint = %self.identity.endpoint_id(),
            relay_url = ?relay_url,
            ticket_len = encoded.len()
        );
        Ok(encoded)
    }

    pub async fn start_pairing_host(&self, ttl_secs: Option<u64>) -> anyhow::Result<String> {
        pairing_dev!("host.open.start", local_endpoint = %self.identity.endpoint_id());
        self.stop_pairing_host().await;

        let persistent = protocol::pairing::pairing_host_is_persistent(ttl_secs);
        self.pairing_host_persistent
            .store(persistent, Ordering::SeqCst);
        self.pairing_host_open.store(true, Ordering::SeqCst);
        self.access.write().await.pairing_host_open = true;
        pairing_dev!(
            "host.open.active",
            local_endpoint = %self.identity.endpoint_id(),
            ttl_secs = ?ttl_secs,
            persistent
        );

        if let Some(ttl) = ttl_secs {
            let access = self.access.clone();
            let flag = self.pairing_host_open.clone();
            let persistent_flag = self.pairing_host_persistent.clone();
            let app_handle = self.app_handle.clone();
            let handle = tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(ttl)).await;
                flag.store(false, Ordering::SeqCst);
                persistent_flag.store(false, Ordering::SeqCst);
                access.write().await.pairing_host_open = false;
                pairing_dev!("host.expired", reason = "ttl_elapsed");
                if let Some(handle) = &app_handle {
                    pairing_dev!("host.emit_ui", event = "pairing-host-expired");
                    let _ = handle.emit_event("pairing-host-expired");
                }
            });
            *self.pairing_expire_task.lock().await = Some(handle);
        }

        let ticket = self.pairing_ticket().await?;
        pairing_dev!(
            "host.open.done",
            local_endpoint = %self.identity.endpoint_id(),
            ticket_len = ticket.len()
        );
        Ok(ticket)
    }

    pub async fn stop_pairing_host(&self) {
        if let Some(handle) = self.pairing_expire_task.lock().await.take() {
            handle.abort();
            pairing_dev!("host.timer_aborted", local_endpoint = %self.identity.endpoint_id());
        }
        self.pairing_host_persistent.store(false, Ordering::SeqCst);
        let was_open = self.pairing_host_open.swap(false, Ordering::SeqCst);
        self.access.write().await.pairing_host_open = false;
        if was_open {
            pairing_dev!("host.close", local_endpoint = %self.identity.endpoint_id());
        }
    }

    pub async fn join_pairing(&self, ticket_str: &str) -> anyhow::Result<()> {
        let join_start = Instant::now();
        pairing_dev!(
            "join.start",
            local_endpoint = %self.identity.endpoint_id(),
            ticket_len = ticket_str.len()
        );
        let ticket = protocol::PairingTicket::decode(ticket_str)?;
        let remote = EndpointId::from_str(&ticket.endpoint_id)?;
        pairing_dev!(
            "join.ticket_decoded",
            local_endpoint = %self.identity.endpoint_id(),
            host_endpoint = %remote,
            relay_url = ?ticket.relay_url
        );
        let host_relay_url = ticket.relay_url.clone();
        let mut addr = EndpointAddr::from(remote);
        if let Some(relay) = host_relay_url.as_deref() {
            if let Ok(url) = relay.parse() {
                addr.addrs.insert(TransportAddr::Relay(url));
                pairing_dev!("join.relay_hint", host_endpoint = %remote, relay = %relay);
            } else {
                pairing_dev_warn!(
                    "join.relay_hint_invalid",
                    host_endpoint = %remote,
                    relay = %relay
                );
            }
        }
        pairing_dev!(
            "join.connect_addr",
            host_endpoint = %remote,
            addr = %format_connect_addr(&addr)
        );

        pairing_dev!("join.connect", host_endpoint = %remote);
        let connect_start = Instant::now();
        let runtime = self.runtime.lock().await;
        let conn = match runtime.endpoint.connect(addr, CONTROL_ALPN).await {
            Ok(conn) => conn,
            Err(err) => {
                log_pairing_error("join.connect_failed", &err);
                return Err(err).context("pairing connect failed");
            }
        };
        drop(runtime);
        pairing_dev!(
            "join.connected",
            host_endpoint = %remote,
            remote_conn = %conn.remote_id(),
            connect_ms = elapsed_ms(connect_start)
        );

        pairing_dev!("join.export_keying", host_endpoint = %remote);
        let keying = export_connection_keying_material(&conn)?;

        pairing_dev!("join.open_bi", host_endpoint = %remote);
        let bi_start = Instant::now();
        let (mut send, mut recv) = conn.open_bi().await.context("open bi stream for join")?;
        pairing_dev!(
            "join.bi_ready",
            host_endpoint = %remote,
            open_bi_ms = elapsed_ms(bi_start)
        );

        // Send first so the host can accept_bi and begin its side of the handshake.
        let info = ControlMessage::PairingInfo {
            endpoint_id: self.identity.endpoint_id(),
            display_name: self.identity.display_name(),
            device_type: self.identity.device_type(),
            os: self.identity.os(),
            signature: sign_challenge(&self.identity.secret_key, &keying),
        };
        write_message(&mut send, &info)
            .await
            .context("write local PairingInfo")?;
        pairing_dev!(
            "join.sent_pairing_info",
            host_endpoint = %remote,
            local_endpoint = %self.identity.endpoint_id()
        );

        let vote = ControlMessage::RememberVote {
            session_id: uuid::Uuid::new_v4().to_string(),
            vote: RememberVote::Remember,
        };
        write_message(&mut send, &vote)
            .await
            .context("write RememberVote")?;
        pairing_dev!(
            "join.sent_remember_vote",
            host_endpoint = %remote,
            vote = ?RememberVote::Remember
        );

        pairing_dev!("join.read_host_pairing_info", host_endpoint = %remote);
        let read_start = Instant::now();
        let host_info = match read_message(&mut recv).await {
            Ok(msg) => msg,
            Err(err) => {
                log_pairing_error("join.read_host_pairing_info_failed", &err);
                return Err(err).context("read host PairingInfo");
            }
        };
        pairing_dev!(
            "join.read_host_pairing_info_ok",
            host_endpoint = %remote,
            read_ms = elapsed_ms(read_start)
        );
        let ControlMessage::PairingInfo {
            endpoint_id,
            display_name,
            device_type,
            os,
            signature,
        } = host_info
        else {
            pairing_dev_warn!(
                "join.unexpected_host_message",
                host_endpoint = %remote,
                kind = ?host_info
            );
            anyhow::bail!("expected host PairingInfo");
        };
        let peer_id = EndpointId::from_str(&endpoint_id).context("invalid host endpoint id")?;
        if !verify_challenge(&peer_id, &keying, &signature) {
            anyhow::bail!("host PairingInfo signature invalid");
        }
        pairing_dev!(
            "join.host_pairing_info_ok",
            host_endpoint = %endpoint_id,
            display_name = %display_name,
            device_type = %device_type,
            os = %os
        );
        let now = protocol::identity::unix_now_ms();
        self.paired_store.remember(PairedDevice {
            endpoint_id: endpoint_id.clone(),
            display_name: display_name.clone(),
            device_type,
            os,
            paired_at: now,
            last_seen_at: now,
            relay_url: host_relay_url.clone(),
            pairing_status: PairingStatus::Active,
        })?;
        self.access.write().await.allowed.insert(peer_id);
        self.paired_connections.refresh().await;
        pairing_dev!(
            "pair.complete.stored",
            role = "joiner",
            host_endpoint = %endpoint_id,
            display_name = %display_name,
            relay_url = ?host_relay_url
        );
        if let Some(handle) = &self.app_handle {
            pairing_dev!("pair.emit_ui", event = "device-paired", role = "joiner");
            let _ = handle.emit_event("device-paired");
        }
        pairing_dev!(
            "join.done",
            host_endpoint = %endpoint_id,
            success = true,
            total_ms = elapsed_ms(join_start)
        );
        Ok(())
    }

    pub async fn invite_paired_device(
        &self,
        remote_endpoint_id: &str,
        blob_ticket: &str,
        file_count: u32,
        total_size: u64,
    ) -> anyhow::Result<bool> {
        let invite_start = Instant::now();
        let remote = EndpointId::from_str(remote_endpoint_id)?;
        let local = self.identity.endpoint_id();
        let access = self.access.read().await;
        let in_allowlist = access.allowed.contains(&remote);
        let allowlist: Vec<String> = access.allowed.iter().map(|id| id.to_string()).collect();
        drop(access);

        pairing_flow!(
            "invite",
            "outbound",
            "invite.start",
            local_endpoint = %local,
            remote_endpoint = %remote,
            peer_in_allowlist = in_allowlist,
            allowlist_size = allowlist.len(),
            allowlist = ?allowlist,
            file_count,
            total_size,
            ticket_len = blob_ticket.len(),
            role = "sender"
        );
        if !in_allowlist {
            pairing_flow_warn!(
                "invite",
                "outbound",
                "invite.abort_not_allowed",
                remote_endpoint = %remote,
                allowlist = ?allowlist,
                role = "sender"
            );
            anyhow::bail!("unknown paired device");
        }

        let stored_relay = self
            .paired_store
            .get(remote_endpoint_id)?
            .and_then(|d| d.relay_url);
        let paired_meta = self.paired_store.get(remote_endpoint_id)?;
        pairing_flow!(
            "invite",
            "outbound",
            "invite.paired_device",
            remote_endpoint = %remote,
            display_name = paired_meta.as_ref().map(|d| d.display_name.as_str()).unwrap_or("?"),
            stored_relay = ?stored_relay,
            last_seen_at = paired_meta.as_ref().map(|d| d.last_seen_at),
            role = "sender"
        );

        pairing_flow!(
            "invite",
            "outbound",
            "invite.wait_session.start",
            remote_endpoint = %remote,
            timeout_secs = invite_wait_timeout().as_secs(),
            role = "sender"
        );
        let conn = match self
            .paired_connections
            .wait_for_connection(remote_endpoint_id, invite_wait_timeout())
            .await
        {
            Some(conn) => {
                pairing_flow!(
                    "invite",
                    "outbound",
                    "invite.wait_session.ok",
                    remote_endpoint = %remote,
                    remote_conn = %conn.remote_id(),
                    source = "persistent_session",
                    role = "sender"
                );
                conn
            }
            None => {
                pairing_flow!(
                    "invite",
                    "outbound",
                    "invite.wait_session.miss",
                    remote_endpoint = %remote,
                    source = "fallback_connect",
                    timeout_secs = PRESENCE_CONNECT_TIMEOUT_SECS,
                    role = "sender"
                );
                let connect_start = Instant::now();
                let runtime = self.runtime.lock().await;
                let addr = build_control_connect_addr(
                    &runtime.endpoint,
                    remote,
                    stored_relay.as_deref(),
                    "invite",
                );
                pairing_flow!(
                    "invite",
                    "outbound",
                    "invite.connect.start",
                    remote_endpoint = %remote,
                    addr = %format_connect_addr(&addr),
                    timeout_secs = PRESENCE_CONNECT_TIMEOUT_SECS,
                    role = "sender"
                );
                let connect = tokio::time::timeout(
                    Duration::from_secs(PRESENCE_CONNECT_TIMEOUT_SECS),
                    runtime.endpoint.connect(addr, CONTROL_ALPN),
                )
                .await;
                drop(runtime);
                match connect {
                    Ok(Ok(conn)) => {
                        pairing_flow!(
                            "invite",
                            "outbound",
                            "invite.connect.ok",
                            remote_endpoint = %remote,
                            remote_conn = %conn.remote_id(),
                            source = "fallback",
                            connect_ms = elapsed_ms(connect_start),
                            role = "sender"
                        );
                        let now = protocol::identity::unix_now_ms();
                        let _ = self.paired_store.touch(remote_endpoint_id, now);
                        set_presence(
                            &self.presence,
                            &self.app_handle,
                            &self.paired_store,
                            remote_endpoint_id,
                            true,
                            "invite_fallback_connect",
                        );
                        conn
                    }
                    Ok(Err(err)) => {
                        log_pairing_flow_error("invite", "outbound", "invite.connect.failed", &err);
                        pairing_flow!(
                            "invite",
                            "outbound",
                            "invite.done",
                            remote_endpoint = %remote,
                            delivered = false,
                            reason = "connect_failed",
                            total_ms = elapsed_ms(invite_start),
                            hint = "target endpoint may be stale after identity rotation; re-pair required",
                            role = "sender"
                        );
                        return Ok(false);
                    }
                    Err(_) => {
                        pairing_flow_warn!(
                            "invite",
                            "outbound",
                            "invite.connect.timeout",
                            remote_endpoint = %remote,
                            timeout_secs = PRESENCE_CONNECT_TIMEOUT_SECS,
                            elapsed_ms = elapsed_ms(connect_start),
                            hint = "target endpoint may be offline or stale after identity rotation; verify paired device endpoint_id matches peer's current identity",
                            role = "sender"
                        );
                        pairing_flow!(
                            "invite",
                            "outbound",
                            "invite.done",
                            remote_endpoint = %remote,
                            delivered = false,
                            reason = "connect_timeout",
                            total_ms = elapsed_ms(invite_start),
                            role = "sender"
                        );
                        return Ok(false);
                    }
                }
            }
        };

        pairing_flow!(
            "invite",
            "outbound",
            "invite.open_bi.start",
            remote_endpoint = %remote,
            role = "sender"
        );
        let bi_start = Instant::now();
        let (mut send, _recv) = match conn.open_bi().await {
            Ok(streams) => {
                pairing_flow!(
                    "invite",
                    "outbound",
                    "invite.open_bi.ok",
                    remote_endpoint = %remote,
                    open_bi_ms = elapsed_ms(bi_start),
                    role = "sender"
                );
                streams
            }
            Err(err) => {
                log_pairing_flow_error("invite", "outbound", "invite.open_bi.failed", &err);
                pairing_flow!(
                    "invite",
                    "outbound",
                    "invite.done",
                    remote_endpoint = %remote,
                    delivered = false,
                    reason = "open_bi_failed",
                    total_ms = elapsed_ms(invite_start),
                    role = "sender"
                );
                return Err(err).context("open bi stream for invite");
            }
        };

        let invite = ControlMessage::Invite {
            blob_ticket: blob_ticket.to_string(),
            file_count,
            total_size,
            sender_name: self.identity.display_name(),
        };
        let write_start = Instant::now();
        if let Err(err) = write_message(&mut send, &invite).await {
            log_pairing_flow_error("invite", "outbound", "invite.write.failed", &err);
            return Err(err).context("write Invite message");
        }
        pairing_flow!(
            "invite",
            "outbound",
            "invite.write.ok",
            remote_endpoint = %remote,
            file_count,
            total_size,
            sender_name = %self.identity.display_name(),
            write_ms = elapsed_ms(write_start),
            role = "sender"
        );
        // Hold the connection in the background so the receiver can read the
        // invite, without blocking the caller (the UI needs a fast result).
        drop(send);
        tokio::spawn(async move {
            pairing_flow!(
                "invite",
                "outbound",
                "invite.wait_receiver.start",
                remote_endpoint = %remote,
                timeout_secs = 15,
                role = "sender"
            );
            let wait_start = Instant::now();
            match tokio::time::timeout(Duration::from_secs(15), conn.closed()).await {
                Ok(closed) => pairing_flow!(
                    "invite",
                    "outbound",
                    "invite.wait_receiver.closed",
                    remote_endpoint = %remote,
                    close_reason = ?closed,
                    wait_ms = elapsed_ms(wait_start),
                    role = "sender"
                ),
                // Expected when the receiver keeps the session open while it
                // downloads; the invite itself was already delivered.
                Err(_) => pairing_flow!(
                    "invite",
                    "outbound",
                    "invite.wait_receiver.timeout",
                    remote_endpoint = %remote,
                    wait_ms = elapsed_ms(wait_start),
                    role = "sender"
                ),
            }
        });
        pairing_flow!(
            "invite",
            "outbound",
            "invite.done",
            remote_endpoint = %remote,
            delivered = true,
            total_ms = elapsed_ms(invite_start),
            role = "sender"
        );
        Ok(true)
    }

    pub async fn respond_paired_invite(
        &self,
        remote_endpoint_id: &str,
        accepted: bool,
    ) -> anyhow::Result<()> {
        let remote = EndpointId::from_str(remote_endpoint_id)?;
        let response = if accepted {
            InviteResponse::Accepted
        } else {
            InviteResponse::Declined
        };
        let response_str = if accepted { "accepted" } else { "declined" };

        pairing_flow!(
            "invite",
            "outbound",
            "invite.response.start",
            remote_endpoint = %remote,
            response = %response_str,
            role = "receiver"
        );

        let access = self.access.read().await;
        let in_allowlist = access.allowed.contains(&remote);
        drop(access);
        if !in_allowlist {
            pairing_flow_warn!(
                "invite",
                "outbound",
                "invite.response.abort_not_allowed",
                remote_endpoint = %remote,
                role = "receiver"
            );
            anyhow::bail!("unknown paired device");
        }

        let stored_relay = self
            .paired_store
            .get(remote_endpoint_id)?
            .and_then(|d| d.relay_url);

        let conn = match self
            .paired_connections
            .wait_for_connection(remote_endpoint_id, invite_wait_timeout())
            .await
        {
            Some(conn) => conn,
            None => {
                pairing_flow!(
                    "invite",
                    "outbound",
                    "invite.response.wait_session.miss",
                    remote_endpoint = %remote,
                    source = "fallback_connect",
                    role = "receiver"
                );
                let runtime = self.runtime.lock().await;
                let addr = build_control_connect_addr(
                    &runtime.endpoint,
                    remote,
                    stored_relay.as_deref(),
                    "invite",
                );
                let connect = tokio::time::timeout(
                    Duration::from_secs(PRESENCE_CONNECT_TIMEOUT_SECS),
                    runtime.endpoint.connect(addr, CONTROL_ALPN),
                )
                .await;
                match connect {
                    Ok(Ok(conn)) => conn,
                    Ok(Err(err)) => {
                        return Err(err).context("invite response connect failed");
                    }
                    Err(_) => anyhow::bail!("invite response connect timeout"),
                }
            }
        };

        let (mut send, _recv) = conn
            .open_bi()
            .await
            .context("open bi stream for invite response")?;
        let message = ControlMessage::InviteResponse {
            session_id: String::new(),
            response,
        };
        write_message(&mut send, &message)
            .await
            .context("write InviteResponse message")?;
        let _ = send.finish();

        pairing_flow!(
            "invite",
            "outbound",
            "invite.response.sent",
            remote_endpoint = %remote,
            response = %response_str,
            role = "receiver"
        );
        Ok(())
    }
}

fn load_allowed_from_store(paired_store: &PairedDeviceStore) -> anyhow::Result<HashSet<EndpointId>> {
    let mut allowed = HashSet::new();
    for device in paired_store.list()? {
        if !device.pairing_status.is_connectable() {
            pairing_dev!(
                "store.allowlist_skip",
                endpoint_id = %device.endpoint_id,
                reason = "unpaired_remotely"
            );
            continue;
        }
        if let Ok(id) = EndpointId::from_str(&device.endpoint_id) {
            allowed.insert(id);
        } else {
            pairing_dev_warn!(
                "store.allowlist_skip",
                endpoint_id = %device.endpoint_id,
                reason = "invalid_endpoint_id"
            );
        }
    }
    pairing_dev!(
        "store.allowlist_loaded",
        count = allowed.len()
    );
    Ok(allowed)
}

async fn send_forget_to_peer(
    runtime: &Arc<Mutex<NodeRuntime>>,
    identity: &DeviceIdentity,
    endpoint_id: &str,
    stored_relay: Option<&str>,
) -> anyhow::Result<()> {
    let remote = EndpointId::from_str(endpoint_id)?;
    pairing_flow!(
        "forget",
        "outbound",
        "forget.notify.start",
        remote_endpoint = %remote,
        stored_relay = ?stored_relay
    );
    let runtime_guard = runtime.lock().await;
    let addr = build_control_connect_addr(
        &runtime_guard.endpoint,
        remote,
        stored_relay,
        "forget",
    );
    pairing_flow!(
        "forget",
        "outbound",
        "forget.connect.start",
        remote_endpoint = %remote,
        addr = %format_connect_addr(&addr),
        timeout_secs = PRESENCE_CONNECT_TIMEOUT_SECS
    );
    let connect_start = Instant::now();
    let connect = tokio::time::timeout(
        Duration::from_secs(PRESENCE_CONNECT_TIMEOUT_SECS),
        runtime_guard.endpoint.connect(addr, CONTROL_ALPN),
    )
    .await;
    drop(runtime_guard);

    let conn = match connect {
        Ok(Ok(conn)) => {
            pairing_flow!(
                "forget",
                "outbound",
                "forget.connect.ok",
                remote_endpoint = %remote,
                connect_ms = elapsed_ms(connect_start)
            );
            conn
        }
        Ok(Err(err)) => {
            log_pairing_flow_error("forget", "outbound", "forget.connect.failed", &err);
            return Err(err).context("forget connect failed");
        }
        Err(_) => {
            pairing_flow_warn!(
                "forget",
                "outbound",
                "forget.connect.timeout",
                remote_endpoint = %remote,
                timeout_secs = PRESENCE_CONNECT_TIMEOUT_SECS,
                elapsed_ms = elapsed_ms(connect_start)
            );
            anyhow::bail!("forget connect timeout");
        }
    };
    pairing_flow!(
        "forget",
        "outbound",
        "forget.open_bi.start",
        remote_endpoint = %remote
    );
    let keying = export_connection_keying_material(&conn)?;
    let (mut send, _recv) = conn.open_bi().await.context("forget open bi")?;
    let forget = ControlMessage::Forget {
        signature: sign_challenge(&identity.secret_key, &keying),
    };
    write_message(&mut send, &forget)
        .await
        .context("forget write message")?;
    let _ = send.finish();
    // The peer closes the connection after reading the message; the timeout
    // is a flush fallback for older peers that keep it open.
    match tokio::time::timeout(Duration::from_secs(5), conn.closed()).await {
        Ok(closed) => pairing_flow!(
            "forget",
            "outbound",
            "forget.notify.peer_closed",
            remote_endpoint = %remote,
            close_reason = ?closed
        ),
        Err(_) => pairing_flow!(
            "forget",
            "outbound",
            "forget.notify.close_wait_timeout",
            remote_endpoint = %remote
        ),
    }
    pairing_flow!(
        "forget",
        "outbound",
        "forget.notify.sent",
        remote_endpoint = %remote
    );
    Ok(())
}

async fn build_runtime(
    identity: Arc<DeviceIdentity>,
    paired_store: Arc<PairedDeviceStore>,
    access: Arc<RwLock<AccessState>>,
    pairing_host_persistent: Arc<AtomicBool>,
    app_handle: AppHandle,
    presence: Arc<std::sync::RwLock<HashMap<String, bool>>>,
    paired_connections: Arc<PairedConnectionManager>,
    relay_mode: RelayMode,
) -> anyhow::Result<NodeRuntime> {
    pairing_dev!(
        "runtime.build.start",
        local_endpoint = %identity.endpoint_id(),
        relay_mode = ?relay_mode
    );
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
    pairing_dev!(
        "runtime.endpoint.online",
        local_endpoint = %identity.endpoint_id(),
        node_id = %endpoint.id()
    );

    let mut local_addr = endpoint.addr();
    apply_options(&mut local_addr, AddrInfoOptions::Relay);
    let home_relay_url = local_addr.relay_urls().next().map(|u| u.to_string());
    pairing_dev!(
        "runtime.home_relay",
        local_endpoint = %identity.endpoint_id(),
        home_relay = ?home_relay_url
    );

    let control = ControlProtocol {
        ctx: ControlCtx {
            identity,
            paired_store,
            access,
            pairing_host_persistent,
            app_handle,
            home_relay_url,
            presence,
            paired_connections,
        },
    };

    let router = Router::builder(endpoint.clone())
        .accept(CONTROL_ALPN, control)
        .spawn();

    pairing_dev!(
        "runtime.build.done",
        alpn = %String::from_utf8_lossy(CONTROL_ALPN)
    );

    Ok(NodeRuntime { endpoint, router })
}
