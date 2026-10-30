#!/usr/bin/env sh
# Commander Lab launcher (Linux). Picks the desktop window or the browser
# edition. Both use the same decks in ~/.local/share/commander-lab.
APP_DIR="$(cd "$(dirname "$0")" && pwd)"
export COMMANDER_LAB_HOME="${COMMANDER_LAB_HOME:-${XDG_DATA_HOME:-$HOME/.local/share}/commander-lab}"

web() {
    echo "Starting Commander Lab in your browser (Ctrl+C to quit)..."
    exec "$APP_DIR/commander-lab-web" "$@"
}

if [ "$1" = "--web" ]; then
    shift
    web "$@"
fi

if [ -f /etc/NIXOS ]; then
    # AppImages need an FHS system; use appimage-run when it's installed.
    if command -v appimage-run >/dev/null 2>&1; then
        exec appimage-run "$APP_DIR/CommanderLab.AppImage" "$@"
    fi
    web "$@"
fi

# Extract-and-run avoids needing FUSE.
APPIMAGE_EXTRACT_AND_RUN=1 "$APP_DIR/CommanderLab.AppImage" "$@"
status=$?
if [ $status -ne 0 ] && [ $status -ne 130 ]; then
    echo "The desktop window couldn't start (exit $status); opening the browser edition instead."
    web "$@"
fi
