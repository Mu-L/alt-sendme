#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROFILE="debug"
CARGO_RELEASE=""

for arg in "$@"; do
	case "$arg" in
	--release)
		PROFILE="release"
		CARGO_RELEASE="--release"
		;;
	esac
done

if [[ -z "${CC:-}" ]]; then
	if [[ -x /opt/homebrew/opt/llvm/bin/clang ]]; then
		export CC=/opt/homebrew/opt/llvm/bin/clang
	elif [[ -x /usr/local/opt/llvm/bin/clang ]]; then
		export CC=/usr/local/opt/llvm/bin/clang
	fi
fi

cd "$ROOT/wasm-bridge"
export CARGO_TARGET_DIR="$ROOT/wasm-bridge/target"
cargo build --target wasm32-unknown-unknown $CARGO_RELEASE

WASM_PATH="$CARGO_TARGET_DIR/wasm32-unknown-unknown/$PROFILE/wasm_bridge.wasm"
OUT_DIR="$ROOT/frontend/src/wasm/pkg"

if ! command -v wasm-bindgen >/dev/null 2>&1; then
	echo "wasm-bindgen not found; installing wasm-bindgen-cli..."
	cargo install wasm-bindgen-cli --locked
fi

mkdir -p "$OUT_DIR"
wasm-bindgen "$WASM_PATH" \
	--target web \
	--out-dir "$OUT_DIR" \
	--out-name wasm_bridge \
	--typescript

echo "WASM package written to $OUT_DIR"
