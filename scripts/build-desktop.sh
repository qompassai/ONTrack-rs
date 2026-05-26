#!/usr/bin/env bash
# scripts/build-desktop.sh — Build the OnTrack desktop binary in release mode.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

FEATURES=${ONTRACK_FEATURES:-}
ARGS=()
if [ -n "$FEATURES" ]; then
    ARGS+=(--features "$FEATURES")
fi

cargo build -p ontrack-desktop --release "${ARGS[@]}"
echo
echo "→ Binary: $ROOT/target/release/ontrack"
