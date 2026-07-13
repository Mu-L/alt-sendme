use serde::{Deserialize, Serialize};

pub const CONTROL_ALPN: &[u8] = b"altsendme/control/1";

pub const AUTH_LABEL: &[u8] = b"altsendme-device-auth-v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ControlMessage {
    PairingInfo {
        endpoint_id: String,
        display_name: String,
        device_type: String,
        /// OS family exchanged at pair time (`macos`, `linux`, …). Optional for
        /// backward compatibility with older peers.
        #[serde(default)]
        os: String,
        signature: String,
    },
    RememberVote {
        session_id: String,
        vote: RememberVote,
    },
    Invite {
        blob_ticket: String,
        file_count: u32,
        total_size: u64,
        sender_name: String,
    },
    InviteResponse {
        session_id: String,
        response: InviteResponse,
    },
    Recognition {
        signature: String,
    },
    Forget {
        signature: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RememberVote {
    Remember,
    No,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum InviteResponse {
    Accepted,
    Declined,
}

/// Pairing join payload encoded in QR / paste code.
///
/// Encoded as a bare 64-char endpoint id when no relay hint is needed; JSON
/// with `relay_url` when the host uses a custom relay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingTicket {
    #[serde(default = "default_v", skip_serializing_if = "is_v1")]
    pub v: u32,
    pub kind: String,
    pub endpoint_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relay_url: Option<String>,
}

const fn default_v() -> u32 {
    1
}

const fn is_v1(v: &u32) -> bool {
    *v == 1
}

fn is_endpoint_id_hex(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Relay URL to embed in a pairing ticket. Public/default relays are omitted
/// because joiners discover them via Pkarr/DNS.
pub fn pairing_ticket_relay_hint(relay_url: Option<String>) -> Option<String> {
    relay_url.filter(|url| !crate::relay::is_public_relay_url(url))
}

impl PairingTicket {
    pub const KIND: &'static str = "pair";

    pub fn encode(&self) -> anyhow::Result<String> {
        let relay_hint = pairing_ticket_relay_hint(self.relay_url.clone());
        if relay_hint.is_none() {
            anyhow::ensure!(
                is_endpoint_id_hex(&self.endpoint_id),
                "invalid endpoint id"
            );
            return Ok(self.endpoint_id.clone());
        }
        let ticket = Self {
            v: self.v,
            kind: self.kind.clone(),
            endpoint_id: self.endpoint_id.clone(),
            relay_url: relay_hint,
        };
        Ok(serde_json::to_string(&ticket)?)
    }

    pub fn decode(s: &str) -> anyhow::Result<Self> {
        let trimmed = s.trim();
        if let Ok(ticket) = serde_json::from_str::<Self>(trimmed) {
            anyhow::ensure!(ticket.kind == Self::KIND, "not a pairing ticket");
            anyhow::ensure!(
                is_endpoint_id_hex(&ticket.endpoint_id),
                "invalid endpoint id in pairing ticket"
            );
            return Ok(ticket);
        }
        // Allow bare endpoint id hex for manual entry.
        if is_endpoint_id_hex(trimmed) {
            return Ok(Self {
                v: 1,
                kind: Self::KIND.to_string(),
                endpoint_id: trimmed.to_string(),
                relay_url: None,
            });
        }
        anyhow::bail!("invalid pairing ticket")
    }
}

pub async fn write_message(
    send: &mut (impl tokio::io::AsyncWrite + Unpin),
    message: &ControlMessage,
) -> anyhow::Result<()> {
    use tokio::io::AsyncWriteExt;
    let body = serde_json::to_vec(message)?;
    const MAX: usize = 1024 * 1024;
    anyhow::ensure!(body.len() <= MAX, "control message too large");
    let len = (body.len() as u32).to_be_bytes();
    send.write_all(&len).await?;
    send.write_all(&body).await?;
    send.flush().await?;
    Ok(())
}

pub async fn read_message(
    recv: &mut (impl tokio::io::AsyncRead + Unpin),
) -> anyhow::Result<ControlMessage> {
    use tokio::io::AsyncReadExt;
    let mut len_buf = [0u8; 4];
    recv.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    anyhow::ensure!(len > 0 && len <= 1024 * 1024, "invalid control message length");
    let mut body = vec![0u8; len];
    recv.read_exact(&mut body).await?;
    Ok(serde_json::from_slice(&body)?)
}
