//! WASM bridge: wasm-bindgen exports over the shared `engine` crate.

use engine::{
    fetch_metadata, set_wasm_secret_key, start_share_bytes, AddrInfoOptions, AppHandle,
    EventEmitter, FileMetadata, ReceiveOptions, RelayModeOption, SendOptions, WasmReceiveResult,
    WasmShareSession,
};
use std::str::FromStr;
use std::sync::{Arc, Mutex, OnceLock};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

struct JsEventEmitter {
    callback: js_sys::Function,
}

impl EventEmitter for JsEventEmitter {
    fn emit_event(&self, event_name: &str) -> Result<(), String> {
        self.callback
            .call2(
                &JsValue::NULL,
                &JsValue::from_str(event_name),
                &JsValue::NULL,
            )
            .map_err(|e| format!("event callback failed: {e:?}"))?;
        Ok(())
    }

    fn emit_event_with_payload(&self, event_name: &str, payload: &str) -> Result<(), String> {
        self.callback
            .call2(
                &JsValue::NULL,
                &JsValue::from_str(event_name),
                &JsValue::from_str(payload),
            )
            .map_err(|e| format!("event callback failed: {e:?}"))?;
        Ok(())
    }
}

static EVENT_CALLBACK: OnceLock<Mutex<Option<js_sys::Function>>> = OnceLock::new();
static SHARE_SESSION: OnceLock<Mutex<Option<WasmShareSession>>> = OnceLock::new();

fn event_slot() -> &'static Mutex<Option<js_sys::Function>> {
    EVENT_CALLBACK.get_or_init(|| Mutex::new(None))
}

fn share_slot() -> &'static Mutex<Option<WasmShareSession>> {
    SHARE_SESSION.get_or_init(|| Mutex::new(None))
}

fn app_handle() -> AppHandle {
    let guard = event_slot().lock().expect("event callback mutex poisoned");
    guard
        .as_ref()
        .map(|callback| Arc::new(JsEventEmitter { callback: callback.clone() }) as Arc<dyn EventEmitter>)
}

fn js_err(err: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&err.to_string())
}

/// Register `(eventName, payload?) => void` for transfer progress events.
#[wasm_bindgen]
pub fn set_event_callback(callback: js_sys::Function) {
    *event_slot().lock().expect("event callback mutex poisoned") = Some(callback);
}

/// Persist node identity across page reloads (hex-encoded iroh secret key).
#[wasm_bindgen]
pub fn set_secret_key(secret_hex: &str) -> Result<(), JsValue> {
    let key = iroh::SecretKey::from_str(secret_hex).map_err(js_err)?;
    set_wasm_secret_key(key);
    Ok(())
}

#[wasm_bindgen]
pub struct WasmSendResult {
    ticket: String,
    hash: String,
    size: u64,
}

#[wasm_bindgen]
impl WasmSendResult {
    #[wasm_bindgen(getter)]
    pub fn ticket(&self) -> String {
        self.ticket.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn hash(&self) -> String {
        self.hash.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn size(&self) -> u64 {
        self.size
    }
}

#[wasm_bindgen]
pub struct WasmReceiveFileResult {
    file_name: String,
    bytes: Vec<u8>,
}

#[wasm_bindgen]
impl WasmReceiveFileResult {
    #[wasm_bindgen(getter)]
    pub fn file_name(&self) -> String {
        self.file_name.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn bytes(&self) -> Vec<u8> {
        self.bytes.clone()
    }
}

/// Share a single in-memory file (relay-only ticket).
#[wasm_bindgen]
pub async fn send_file(
    file_name: String,
    bytes: Vec<u8>,
    metadata_json: Option<String>,
) -> Result<WasmSendResult, JsValue> {
    let metadata = match metadata_json {
        Some(json) => Some(serde_json::from_str::<FileMetadata>(&json).map_err(js_err)?),
        None => None,
    };

    let options = SendOptions {
        relay_mode: RelayModeOption::Default,
        ticket_type: AddrInfoOptions::Relay,
        magic_ipv4_addr: None,
        magic_ipv6_addr: None,
    };

    let session = start_share_bytes(file_name, bytes, options, &app_handle(), metadata)
        .await
        .map_err(js_err)?;

    let result = WasmSendResult {
        ticket: session.ticket.clone(),
        hash: session.hash.clone(),
        size: session.size,
    };

    *share_slot().lock().expect("share session mutex poisoned") = Some(session);
    Ok(result)
}

/// Stop the active browser share session, if any.
#[wasm_bindgen]
pub fn stop_sharing() {
    let _ = share_slot().lock().expect("share session mutex poisoned").take();
}

/// Fetch sender metadata JSON for a ticket (no file download).
#[wasm_bindgen]
pub async fn fetch_ticket_metadata(ticket: String) -> Result<String, JsValue> {
    let options = ReceiveOptions {
        output_dir: None,
        relay_mode: RelayModeOption::Default,
        magic_ipv4_addr: None,
        magic_ipv6_addr: None,
    };

    let metadata = fetch_metadata(ticket, options).await.map_err(js_err)?;
    serde_json::to_string(&metadata).map_err(js_err)
}

/// Download a single-file ticket into memory.
#[wasm_bindgen]
pub async fn receive_file(ticket: String) -> Result<WasmReceiveFileResult, JsValue> {
    use engine::download_bytes;

    let options = ReceiveOptions {
        output_dir: None,
        relay_mode: RelayModeOption::Default,
        magic_ipv4_addr: None,
        magic_ipv6_addr: None,
    };

    let WasmReceiveResult { file_name, bytes } =
        download_bytes(ticket, options, app_handle()).await.map_err(js_err)?;

    Ok(WasmReceiveFileResult { file_name, bytes })
}

/// Bind a relay-only iroh endpoint and return its node id (smoke test).
#[wasm_bindgen]
pub async fn smoke_test_endpoint() -> Result<String, JsValue> {
    use iroh::endpoint::presets;
    use iroh::Endpoint;

    let endpoint = Endpoint::builder(presets::N0)
        .bind()
        .await
        .map_err(|e| JsValue::from_str(&format!("endpoint bind failed: {e}")))?;

    let id = endpoint.id().to_string();
    endpoint.close().await;
    Ok(id)
}
