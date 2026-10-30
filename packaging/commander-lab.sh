#!/usr/bin/env sh
# Commander Lab launcher for Linux.
#
# Decks, the card cache and exports live in a per-user data folder so they
# survive updates. Override with COMMANDER_LAB_HOME=/some/path.
set -e

APP_DIR="$(cd "$(dirname "$0")" && pwd)"
DATA_DIR="${COMMANDER_LAB_HOME:-${XDG_DATA_HOME:-$HOME/.local/share}/commander-lab}"

mkdir -p "$DATA_DIR"
cd "$DATA_DIR"

exec "$APP_DIR/commander-lab" "$@"
