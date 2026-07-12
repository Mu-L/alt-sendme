use engine::{sign_challenge, verify_challenge, PairingTicket};
use iroh::SecretKey;

#[test]
fn pairing_ticket_roundtrip_json() {
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
}

#[test]
fn pairing_ticket_accepts_bare_endpoint_id() {
    let endpoint_id = "b".repeat(64);
    let decoded = PairingTicket::decode(&endpoint_id).expect("bare id decode");
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
