use engine::{sign_challenge, verify_challenge, ControlMessage, PairingTicket, PairedDevice, PairingStatus};
use iroh::SecretKey;

#[test]
fn forget_control_message_roundtrip() {
    let msg = ControlMessage::Forget {
        signature: "abc123".to_string(),
    };
    let json = serde_json::to_string(&msg).expect("serialize");
    assert!(json.contains("\"type\":\"forget\""));
    let decoded: ControlMessage = serde_json::from_str(&json).expect("deserialize");
    match decoded {
        ControlMessage::Forget { signature } => assert_eq!(signature, "abc123"),
        other => panic!("expected Forget, got {other:?}"),
    }
}

#[test]
fn paired_device_defaults_to_active_pairing_status() {
    let json = r#"{
        "endpoint_id": "aa",
        "display_name": "Test",
        "device_type": "laptop",
        "os": "macos",
        "paired_at": 1,
        "last_seen_at": 2
    }"#;
    let device: PairedDevice = serde_json::from_str(json).expect("deserialize");
    assert_eq!(device.pairing_status, PairingStatus::Active);
    assert!(device.pairing_status.is_active());
}

#[test]
fn stale_local_identity_devices_are_connectable_but_not_active() {
    let device = PairedDevice {
        endpoint_id: "cc".to_string(),
        display_name: "Stale".to_string(),
        device_type: "desktop".to_string(),
        os: "linux".to_string(),
        paired_at: 1,
        last_seen_at: 2,
        relay_url: None,
        pairing_status: PairingStatus::StaleLocalIdentity,
    };
    assert!(!device.pairing_status.is_active());
    assert!(device.pairing_status.is_connectable());
    let json = serde_json::to_string(&device).expect("serialize");
    assert!(json.contains("stale-local-identity"));
}

#[test]
fn unpaired_remotely_devices_are_not_active() {
    let device = PairedDevice {
        endpoint_id: "bb".to_string(),
        display_name: "Remote".to_string(),
        device_type: "desktop".to_string(),
        os: "linux".to_string(),
        paired_at: 1,
        last_seen_at: 2,
        relay_url: None,
        pairing_status: PairingStatus::UnpairedRemotely,
    };
    assert!(!device.pairing_status.is_active());
    let json = serde_json::to_string(&device).expect("serialize");
    assert!(json.contains("unpaired-remotely"));
}

#[test]
fn pairing_ticket_roundtrip_json_with_relay() {
    let ticket = PairingTicket {
        v: 1,
        kind: PairingTicket::KIND.to_string(),
        endpoint_id: "a".repeat(64),
        relay_url: Some("https://relay.example.com".to_string()),
    };
    let encoded = ticket.encode().expect("encode");
    let decoded = PairingTicket::decode(&encoded).expect("decode");
    assert_eq!(decoded.endpoint_id, ticket.endpoint_id);
    assert_eq!(decoded.relay_url, ticket.relay_url);
    assert!(encoded.contains("relay_url"));
    assert!(!encoded.contains("\"v\":"));
}

#[test]
fn pairing_ticket_encodes_bare_endpoint_id_without_relay() {
    let endpoint_id = "a".repeat(64);
    let ticket = PairingTicket {
        v: 1,
        kind: PairingTicket::KIND.to_string(),
        endpoint_id: endpoint_id.clone(),
        relay_url: None,
    };
    let encoded = ticket.encode().expect("encode");
    assert_eq!(encoded, endpoint_id);
    assert_eq!(encoded.len(), 64);
    let decoded = PairingTicket::decode(&encoded).expect("decode");
    assert_eq!(decoded.endpoint_id, endpoint_id);
    assert!(decoded.relay_url.is_none());
    assert_eq!(decoded.v, 1);
}

#[test]
fn pairing_ticket_encodes_bare_endpoint_id_for_public_relay() {
    let endpoint_id = "a".repeat(64);
    let ticket = PairingTicket {
        v: 1,
        kind: PairingTicket::KIND.to_string(),
        endpoint_id: endpoint_id.clone(),
        relay_url: Some("https://aps1-1.relay.n0.iroh.link./".to_string()),
    };
    let encoded = ticket.encode().expect("encode");
    assert_eq!(encoded, endpoint_id);
}

#[test]
fn pairing_ticket_accepts_bare_endpoint_id() {
    let endpoint_id = "b".repeat(64);
    let decoded = PairingTicket::decode(&endpoint_id).expect("bare id decode");
    assert_eq!(decoded.endpoint_id, endpoint_id);
    assert!(decoded.relay_url.is_none());
}

#[test]
fn pairing_ticket_accepts_legacy_json_with_v_and_null_relay() {
    let endpoint_id = "c".repeat(64);
    let legacy = format!(
        "{{\"v\":1,\"kind\":\"pair\",\"endpoint_id\":\"{}\",\"relay_url\":null}}",
        endpoint_id
    );
    let decoded = PairingTicket::decode(&legacy).expect("legacy decode");
    assert_eq!(decoded.endpoint_id, endpoint_id);
    assert!(decoded.relay_url.is_none());
}

#[test]
fn pairing_auth_sign_and_verify() {
    let secret = SecretKey::generate();
    let endpoint_id = secret.public();
    let keying = b"test-keying-material-32-bytes!!";
    let signature = sign_challenge(&secret, keying);
    assert!(verify_challenge(&endpoint_id, keying, &signature));
    assert!(!verify_challenge(&endpoint_id, b"wrong", &signature));
}

#[test]
fn pairing_host_is_persistent_when_ttl_is_none() {
    use engine::pairing_host_is_persistent;
    assert!(pairing_host_is_persistent(None));
    assert!(!pairing_host_is_persistent(Some(120)));
}
#[test]
fn relay_and_addresses_preserves_relay_and_ip_addrs() {
    use engine::{apply_options, AddrInfoOptions};
    use iroh::{EndpointAddr, EndpointId, RelayUrl, TransportAddr};
    use std::str::FromStr;

    let secret = SecretKey::generate();
    let endpoint_id = EndpointId::from(secret.public());
    let relay_url = RelayUrl::from_str("https://relay.example.com").unwrap();
    let mut addr = EndpointAddr::from(endpoint_id);
    addr.addrs.insert(TransportAddr::Relay(relay_url.clone()));
    addr.addrs.insert(TransportAddr::Ip("127.0.0.1:8080".parse().unwrap()));

    let before = addr.addrs.len();
    apply_options(&mut addr, AddrInfoOptions::RelayAndAddresses);
    assert_eq!(addr.addrs.len(), before);
    assert!(addr
        .addrs
        .iter()
        .any(|a| matches!(a, TransportAddr::Relay(_))));
    assert!(addr.addrs.iter().any(|a| matches!(a, TransportAddr::Ip(_))));
}
