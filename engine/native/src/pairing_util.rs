use std::collections::HashMap;
use std::sync::Arc;

use iroh::endpoint::Endpoint;
use iroh::{EndpointAddr, EndpointId, TransportAddr};
use protocol::{apply_options, AddrInfoOptions, AppHandle, ControlMessage};

use crate::device_identity::PairedDeviceStore;
use crate::{pairing_dev_warn, pairing_flow};

pub fn control_message_kind(msg: &ControlMessage) -> &'static str {
    match msg {
        ControlMessage::PairingInfo { .. } => "PairingInfo",
        ControlMessage::RememberVote { .. } => "RememberVote",
        ControlMessage::Invite { .. } => "Invite",
        ControlMessage::InviteResponse { .. } => "InviteResponse",
        ControlMessage::Recognition { .. } => "Recognition",
        ControlMessage::Forget { .. } => "Forget",
    }
}

pub fn build_control_connect_addr(
    endpoint: &Endpoint,
    remote: EndpointId,
    stored_relay: Option<&str>,
    flow: &str,
) -> EndpointAddr {
    pairing_flow!(
        flow,
        "outbound",
        "connect.addr_build.start",
        remote = %remote,
        stored_relay = ?stored_relay
    );
    let mut addr = EndpointAddr::from(remote);
    if let Some(relay) = stored_relay {
        if let Ok(url) = relay.parse() {
            addr.addrs.insert(TransportAddr::Relay(url));
            pairing_flow!(
                flow,
                "outbound",
                "connect.relay_hint",
                remote = %remote,
                relay = %relay,
                source = "paired_store"
            );
        } else {
            pairing_dev_warn!(
                "connect.relay_hint_invalid",
                flow,
                remote = %remote,
                relay = %relay,
                source = "paired_store"
            );
        }
    } else {
        pairing_flow!(
            flow,
            "outbound",
            "connect.relay_hint_missing",
            remote = %remote,
            source = "paired_store"
        );
    }
    let mut local = endpoint.addr();
    apply_options(&mut local, AddrInfoOptions::Relay);
    if let Some(relay) = local.relay_urls().next() {
        let relay_str = relay.to_string();
        addr.addrs.insert(TransportAddr::Relay(relay.clone()));
        pairing_flow!(
            flow,
            "outbound",
            "connect.relay_hint",
            remote = %remote,
            relay = %relay_str,
            source = "local_home"
        );
    }
    pairing_flow!(
        flow,
        "outbound",
        "connect.addr_built",
        remote = %remote,
        addr = %crate::pairing_dev_log::format_connect_addr(&addr)
    );
    addr
}

/// Emit `paired-invite-received` to the frontend (shared by inbound and outbound readers).
pub fn emit_paired_invite_received(
    app_handle: &AppHandle,
    remote_endpoint_id: &str,
    blob_ticket: &str,
    file_count: u32,
    total_size: u64,
    sender_name: &str,
) {
    let payload = serde_json::json!({
        "blob_ticket": blob_ticket,
        "file_count": file_count,
        "total_size": total_size,
        "sender_name": sender_name,
        "remote_endpoint_id": remote_endpoint_id,
    });
    pairing_flow!(
        "invite",
        "outbound",
        "invite.emit_ui.start",
        remote = %remote_endpoint_id,
        event = "paired-invite-received",
        payload_len = payload.to_string().len(),
        role = "receiver"
    );
    let Some(handle) = app_handle else {
        pairing_dev_warn!(
            "invite.emit_ui_skipped",
            remote = %remote_endpoint_id,
            reason = "no_app_handle"
        );
        return;
    };
    match handle.emit_event_with_payload("paired-invite-received", &payload.to_string()) {
        Ok(()) => pairing_flow!(
            "invite",
            "outbound",
            "invite.emit_ui.ok",
            remote = %remote_endpoint_id,
            role = "receiver"
        ),
        Err(err) => pairing_dev_warn!(
            "invite.emit_ui_failed",
            remote = %remote_endpoint_id,
            error = %err
        ),
    }
}

/// Emit `paired-invite-response` so the sender UI can toast accept/decline.
pub fn emit_paired_invite_response(
    app_handle: &AppHandle,
    paired_store: &PairedDeviceStore,
    remote_endpoint_id: &str,
    response: &str,
) {
    let display_name = paired_store
        .get(remote_endpoint_id)
        .ok()
        .flatten()
        .map(|d| d.display_name);
    let payload = serde_json::json!({
        "endpoint_id": remote_endpoint_id,
        "display_name": display_name,
        "response": response,
    });
    pairing_flow!(
        "invite",
        "inbound",
        "invite.response.emit_ui.start",
        remote = %remote_endpoint_id,
        response = %response,
        event = "paired-invite-response",
        role = "sender"
    );
    let Some(handle) = app_handle else {
        pairing_dev_warn!(
            "invite.response.emit_ui_skipped",
            remote = %remote_endpoint_id,
            reason = "no_app_handle"
        );
        return;
    };
    match handle.emit_event_with_payload("paired-invite-response", &payload.to_string()) {
        Ok(()) => pairing_flow!(
            "invite",
            "inbound",
            "invite.response.emit_ui.ok",
            remote = %remote_endpoint_id,
            response = %response,
            role = "sender"
        ),
        Err(err) => pairing_dev_warn!(
            "invite.response.emit_ui_failed",
            remote = %remote_endpoint_id,
            error = %err
        ),
    }
}

pub fn set_presence(
    presence: &Arc<std::sync::RwLock<HashMap<String, bool>>>,
    app_handle: &AppHandle,
    paired_store: &PairedDeviceStore,
    endpoint_id: &str,
    online: bool,
    reason: &str,
) {
    let (changed, previous) = {
        let mut map = presence.write().expect("presence lock");
        let key = endpoint_id.to_lowercase();
        let prev = map.get(&key).copied();
        let changed = prev != Some(online);
        if changed {
            map.insert(key, online);
        }
        (changed, prev)
    };
    pairing_flow!(
        "presence",
        if online { "inbound" } else { "outbound" },
        "presence.update",
        endpoint_id = %endpoint_id,
        online,
        previous = ?previous,
        changed,
        reason
    );
    if !changed {
        return;
    }
    let display_name = paired_store
        .get(endpoint_id)
        .ok()
        .flatten()
        .map(|d| d.display_name.clone())
        .unwrap_or_else(|| "?".to_string());
    let last_seen_at = paired_store
        .get(endpoint_id)
        .ok()
        .flatten()
        .map(|d| d.last_seen_at)
        .unwrap_or(0);
    let payload = serde_json::json!({
        "endpoint_id": endpoint_id,
        "online": online,
        "last_seen_at": last_seen_at,
    });
    pairing_flow!(
        "presence",
        "outbound",
        "presence.emit_ui",
        endpoint_id = %endpoint_id,
        display_name = %display_name,
        online,
        event = "paired-device-presence",
        payload_len = payload.to_string().len(),
        reason
    );
    if let Some(handle) = app_handle {
        if let Err(err) = handle.emit_event_with_payload("paired-device-presence", &payload.to_string())
        {
            pairing_dev_warn!(
                "presence.emit_ui_failed",
                endpoint_id = %endpoint_id,
                error = %err
            );
        }
    } else {
        pairing_dev_warn!(
            "presence.emit_ui_skipped",
            endpoint_id = %endpoint_id,
            reason = "no_app_handle"
        );
    }
}
