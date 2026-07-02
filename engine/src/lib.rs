pub mod core;

#[cfg(not(target_arch = "wasm32"))]
pub use core::{
    receive::{download, fetch_metadata},
    send::start_share,
    send::start_share_items,
    types::{
        AddrInfoOptions, AppHandle, EventEmitter, FileMetadata, FilePreviewItem, ReceiveOptions,
        ReceiveResult, RelayModeOption, SendOptions, SendResult,
    },
};

#[cfg(target_arch = "wasm32")]
pub use core::{
    receive::{download_bytes, fetch_metadata},
    send::start_share_bytes,
    types::{
        set_wasm_secret_key, AddrInfoOptions, AppHandle, EventEmitter, FileMetadata,
        FilePreviewItem, ReceiveOptions, RelayModeOption, SendOptions, WasmReceiveResult,
        WasmShareSession,
    },
};
