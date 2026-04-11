#!/usr/bin/env bash
set -euo pipefail

APP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RELEASE_BIN="$APP_DIR/target/release/bluetrack"
DEBUG_BIN="$APP_DIR/target/debug/bluetrack"

cd "$APP_DIR"

if [[ -x "$RELEASE_BIN" ]]; then
    exec "$RELEASE_BIN" "$@"
fi

if [[ -x "$DEBUG_BIN" ]]; then
    exec "$DEBUG_BIN" "$@"
fi

exec cargo run --release -- "$@"
