#!/usr/bin/env sh
# Commander Lab launcher (Linux). Opens the desktop app. Your decks live in
# ~/.local/share/commander-lab and are shared with the browser edition.
#
#   commander-lab.sh          desktop app
#   commander-lab.sh --web    browser edition instead
APP_DIR="$(cd "$(dirname "$0")" && pwd)"
APPIMAGE_FILE="$APP_DIR/CommanderLab.AppImage"
export COMMANDER_LAB_HOME="${COMMANDER_LAB_HOME:-${XDG_DATA_HOME:-$HOME/.local/share}/commander-lab}"
# WebKitGTK's DMA-BUF renderer shows a blank window on many GPUs (NVIDIA
# especially) and under some Wayland compositors. Software compositing is fine
# for this app.
export WEBKIT_DISABLE_DMABUF_RENDERER="${WEBKIT_DISABLE_DMABUF_RENDERER:-1}"

have() { command -v "$1" >/dev/null 2>&1; }

web() {
    echo "Starting Commander Lab in your browser (Ctrl+C to quit)..."
    exec "$APP_DIR/commander-lab-web" "$@"
}

if [ "$1" = "--web" ]; then
    shift
    web "$@"
fi

if [ -f /etc/NIXOS ]; then
    # AppImages need an FHS system; appimage-run provides one. Use it when
    # it's installed, otherwise let Nix fetch it for this run.
    if have appimage-run; then
        exec appimage-run "$APPIMAGE_FILE"
    fi
    if have nix-shell; then
        echo "Starting the desktop app through appimage-run (the first start downloads it)..."
        nix-shell -p appimage-run --run "appimage-run '$APPIMAGE_FILE'" && exit 0
    fi
    if have nix; then
        nix --extra-experimental-features 'nix-command flakes' run nixpkgs#appimage-run -- "$APPIMAGE_FILE" && exit 0
    fi
    echo "The desktop app needs appimage-run on NixOS and Nix couldn't provide it."
    echo "Add appimage-run to environment.systemPackages for the desktop app."
    web "$@"
fi

# Extract-and-run avoids needing FUSE.
APPIMAGE_EXTRACT_AND_RUN=1 "$APPIMAGE_FILE"
status=$?
if [ $status -ne 0 ] && [ $status -ne 130 ] && [ $status -ne 143 ]; then
    echo "The desktop app couldn't start (exit $status); opening the browser edition instead."
    web "$@"
fi
