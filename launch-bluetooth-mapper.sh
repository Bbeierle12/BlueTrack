#!/usr/bin/env bash
set -euo pipefail

APP_DIR="/home/bbeierle12/bluetooth-mapper"
RELEASE_BIN="$APP_DIR/target/release/bluetooth-mapper"
DEBUG_BIN="$APP_DIR/target/debug/bluetooth-mapper"

cd "$APP_DIR"

if [[ -x "$RELEASE_BIN" ]]; then
    exec "$RELEASE_BIN" "$@"
fi

if [[ -x "$DEBUG_BIN" ]]; then
    exec "$DEBUG_BIN" "$@"
fi

exec cargo run --release -- "$@"
