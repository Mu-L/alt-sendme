use engine::{
    sign_challenge, verify_challenge, ControlMessage, PairedDevice, PairingStatus, PairingTicket,
};
use iroh::SecretKey;
use protocol::identity::{normalize_display_name, DeviceMetaFile};
use protocol::{read_message, write_message};

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
fn pairing_info_without_os_field_still_parses() {
    // Older peers don't send `os`.
    let json = r#"{
        "type": "pairing-info",
        "endpoint_id": "aa",
        "display_name": "Old Peer",
        "device_type": "laptop",
        "signature": "deadbeef"
    }"#;
    let decoded: ControlMessage = serde_json::from_str(json).expect("deserialize");
    match decoded {
        ControlMessage::PairingInfo { os, display_name, .. } => {
            assert_eq!(os, "");
            assert_eq!(display_name, "Old Peer");
        }
        other => panic!("expected PairingInfo, got {other:?}"),
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
    let round_tripped: PairedDevice = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(round_tripped, device);
}

#[test]
fn unpaired_remotely_devices_are_not_active_or_connectable() {
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
    // Must drop out of presence loops and the allowlist.
    assert!(!device.pairing_status.is_connectable());
    let json = serde_json::to_string(&device).expect("serialize");
    assert!(json.contains("unpaired-remotely"));
    let round_tripped: PairedDevice = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(round_tripped, device);
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
fn pairing_ticket_encode_rejects_invalid_endpoint_id() {
    let ticket = PairingTicket {
        v: 1,
        kind: PairingTicket::KIND.to_string(),
        endpoint_id: "not-an-endpoint-id".to_string(),
        relay_url: None,
    };
    assert!(ticket.encode().is_err());
}

#[test]
fn pairing_ticket_accepts_bare_endpoint_id() {
    let endpoint_id = "b".repeat(64);
    let decoded = PairingTicket::decode(&endpoint_id).expect("bare id decode");
    assert_eq!(decoded.endpoint_id, endpoint_id);
    assert!(decoded.relay_url.is_none());
}

#[test]
fn pairing_ticket_decode_trims_whitespace() {
    let endpoint_id = "d".repeat(64);
    let padded = format!("  {endpoint_id}\n");
    let decoded = PairingTicket::decode(&padded).expect("padded decode");
    assert_eq!(decoded.endpoint_id, endpoint_id);
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
fn pairing_ticket_decode_rejects_invalid_input() {
    // Wrong kind.
    let wrong_kind = format!("{{\"kind\":\"share\",\"endpoint_id\":\"{}\"}}", "a".repeat(64));
    assert!(PairingTicket::decode(&wrong_kind).is_err());
    // Bare id too short / not hex.
    assert!(PairingTicket::decode(&"a".repeat(63)).is_err());
    assert!(PairingTicket::decode(&"g".repeat(64)).is_err());
    // Arbitrary garbage.
    assert!(PairingTicket::decode("").is_err());
    assert!(PairingTicket::decode("hello world").is_err());
}

#[test]
fn pairing_ticket_decode_rejects_json_with_invalid_endpoint_id() {
    // Bad ids should fail here, not deep inside the join flow.
    let garbage_id = "{\"kind\":\"pair\",\"endpoint_id\":\"garbage\"}";
    assert!(PairingTicket::decode(garbage_id).is_err());
}

#[test]
fn pairing_auth_sign_and_verify() {
    let secret = SecretKey::generate();
    let endpoint_id = secret.public();
    // Exported keying material is always 32 bytes.
    let keying = [0xA5u8; 32];
    let signature = sign_challenge(&secret, &keying);
    assert!(verify_challenge(&endpoint_id, &keying, &signature));
    assert!(!verify_challenge(&endpoint_id, b"wrong", &signature));
}

#[test]
fn pairing_auth_rejects_forged_or_malformed_signatures() {
    let secret = SecretKey::generate();
    let endpoint_id = secret.public();
    let keying = [0xA5u8; 32];
    let signature = sign_challenge(&secret, &keying);

    // A different key must not be able to impersonate the endpoint.
    let attacker = SecretKey::generate();
    let forged = sign_challenge(&attacker, &keying);
    assert!(!verify_challenge(&endpoint_id, &keying, &forged));

    // Malformed encodings are rejected, not panics.
    assert!(!verify_challenge(&endpoint_id, &keying, "not hex"));
    assert!(!verify_challenge(&endpoint_id, &keying, &"ab".repeat(32)));
    assert!(!verify_challenge(&endpoint_id, &keying, ""));
    // Signatures are exchanged as lowercase hex.
    assert!(!verify_challenge(&endpoint_id, &keying, &signature.to_uppercase()));
}

#[tokio::test]
async fn control_message_framing_roundtrip() {
    let (mut client, mut server) = tokio::io::duplex(64 * 1024);
    let sent = ControlMessage::Invite {
        blob_ticket: "ticket".to_string(),
        file_count: 3,
        total_size: 42,
        sender_name: "Laptop".to_string(),
    };
    write_message(&mut client, &sent).await.expect("write");
    let received = read_message(&mut server).await.expect("read");
    match received {
        ControlMessage::Invite {
            blob_ticket,
            file_count,
            total_size,
            sender_name,
        } => {
            assert_eq!(blob_ticket, "ticket");
            assert_eq!(file_count, 3);
            assert_eq!(total_size, 42);
            assert_eq!(sender_name, "Laptop");
        }
        other => panic!("expected Invite, got {other:?}"),
    }
}

#[tokio::test]
async fn control_message_framing_rejects_oversized_and_bad_lengths() {
    use tokio::io::AsyncWriteExt;

    // Over the 1 MiB cap.
    let (mut client, _server) = tokio::io::duplex(64 * 1024);
    let oversized = ControlMessage::Invite {
        blob_ticket: "x".repeat(1024 * 1024 + 1),
        file_count: 1,
        total_size: 1,
        sender_name: "s".to_string(),
    };
    assert!(write_message(&mut client, &oversized).await.is_err());

    // Zero-length frame.
    let (mut client, mut server) = tokio::io::duplex(64 * 1024);
    client.write_all(&0u32.to_be_bytes()).await.unwrap();
    assert!(read_message(&mut server).await.is_err());

    // Length prefix over the cap.
    let (mut client, mut server) = tokio::io::duplex(64 * 1024);
    client
        .write_all(&(2 * 1024 * 1024u32).to_be_bytes())
        .await
        .unwrap();
    assert!(read_message(&mut server).await.is_err());
}

#[test]
fn normalize_display_name_trims_and_validates() {
    assert_eq!(normalize_display_name("  My Mac  ").unwrap(), "My Mac");
    assert!(normalize_display_name("").is_err());
    assert!(normalize_display_name("   ").is_err());

    let max = DeviceMetaFile::MAX_DISPLAY_NAME_CHARS;
    assert!(normalize_display_name(&"x".repeat(max)).is_ok());
    assert!(normalize_display_name(&"x".repeat(max + 1)).is_err());
    // The limit counts characters, not bytes.
    assert!(normalize_display_name(&"ü".repeat(max)).is_ok());
    assert!(normalize_display_name(&"ü".repeat(max + 1)).is_err());
}

#[test]
fn device_meta_file_migrate_fills_missing_fields() {
    let mut meta = DeviceMetaFile {
        version: 1,
        endpoint_id: "aa".to_string(),
        display_name: "Old".to_string(),
        device_type: String::new(),
        os: String::new(),
        created_at: 1,
        name_is_custom: false,
    };
    meta.migrate();
    assert_eq!(meta.version, DeviceMetaFile::VERSION);
    assert!(!meta.os.is_empty());
    assert!(!meta.device_type.is_empty());
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
