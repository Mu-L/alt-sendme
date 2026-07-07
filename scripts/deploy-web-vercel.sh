#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROJECT_JSON="$ROOT/.vercel/project.json"
OUTPUT="$ROOT/.vercel/output"

cd "$ROOT"
pnpm run build:web:vercel

if [[ -f "$PROJECT_JSON" ]]; then
	export VERCEL_ORG_ID
	export VERCEL_PROJECT_ID
	VERCEL_ORG_ID="$(node -e "console.log(require('$PROJECT_JSON').orgId)")"
	VERCEL_PROJECT_ID="$(node -e "console.log(require('$PROJECT_JSON').projectId)")"
elif [[ -z "${VERCEL_ORG_ID:-}" || -z "${VERCEL_PROJECT_ID:-}" ]]; then
	echo "ERROR: Set VERCEL_ORG_ID and VERCEL_PROJECT_ID, or run 'npx vercel link' from the repo root."
	exit 1
fi

rm -rf "$OUTPUT"
mkdir -p "$OUTPUT/static"
cp -R "$ROOT/frontend/dist/." "$OUTPUT/static/"
rm -f "$OUTPUT/static/vercel.json"

cat >"$OUTPUT/config.json" <<'EOF'
{
	"version": 3,
	"routes": [
		{ "handle": "filesystem" },
		{ "src": "/(.*)", "dest": "/index.html" }
	]
}
EOF

vercel_args=(deploy --prebuilt --prod --yes)
if [[ -n "${VERCEL_TOKEN:-}" ]]; then
	vercel_args+=(--token="$VERCEL_TOKEN")
fi

vercel "${vercel_args[@]}"
