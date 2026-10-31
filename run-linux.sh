#!/usr/bin/env sh
# Build and run Commander Lab straight from this git checkout (Linux, NixOS).
#
#   ./run-linux.sh             desktop app (needs WebKitGTK; see README)
#   ./run-linux.sh --web       browser edition instead (no system libraries needed)
#   ./run-linux.sh --update    git pull, rebuild, then run (combine with --web)
#   ./run-linux.sh --build     build only
#
# Anything else is passed to the app, e.g. ./run-linux.sh --port 8080
# On NixOS it builds and runs inside shell.nix, which has the toolchain and the
# desktop app's libraries.
# Your decks live in ~/.local/share/commander-lab, shared with installed copies.
set -e
cd "$(dirname "$0")"

DESKTOP=1
UPDATE=0
BUILD_ONLY=0
PASS=""
for arg in "$@"; do
    case "$arg" in
        --desktop) DESKTOP=1 ;;
        --web) DESKTOP=0 ;;
        --update) UPDATE=1 ;;
        --build) BUILD_ONLY=1 ;;
        *) PASS="$PASS \"$arg\"" ;;
    esac
done

have() { command -v "$1" >/dev/null 2>&1; }

# NixOS: the desktop app's libraries (and cargo/node, if missing) come from
# shell.nix. Elsewhere, only enter it when the toolchain is missing.
if [ -z "$IN_NIX_SHELL" ] && { { [ -f /etc/NIXOS ] && [ "$DESKTOP" = 1 ]; } || ! have cargo || ! have npm; }; then
    if have nix-shell; then
        echo "Entering nix-shell for cargo and node (first time takes a while)..."
        exec nix-shell shell.nix --run "sh ./run-linux.sh $*"
    fi
    echo "Commander Lab needs Rust (https://rustup.rs) and Node.js 20+ (https://nodejs.org)." >&2
    exit 1
fi

if [ "$UPDATE" = 1 ]; then
    echo "Updating from git..."
    git pull --ff-only
fi

# Frontend: install packages when the lockfile changed, rebuild when sources did.
if [ ! -d frontend/node_modules ] || [ frontend/package-lock.json -nt frontend/node_modules ]; then
    echo "Installing frontend packages..."
    npm --prefix frontend ci
    touch frontend/node_modules
fi
if [ ! -f frontend/dist/index.html ] || [ -n "$(find frontend/src frontend/index.html frontend/package.json -newer frontend/dist/index.html 2>/dev/null | head -n 1)" ]; then
    echo "Building the interface..."
    npm --prefix frontend run build
fi

# Builds made here have no release number, so the app doesn't offer GitHub
# release updates; run ./run-linux.sh --update instead.
if [ "$DESKTOP" = 1 ]; then
    if [ ! -f /etc/NIXOS ] && have pkg-config && ! pkg-config --exists webkit2gtk-4.1 2>/dev/null; then
        echo "The desktop app needs the WebKitGTK 4.1 development libraries:" >&2
        echo "  Debian/Ubuntu: sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev" >&2
        echo "  Fedora:        sudo dnf install webkit2gtk4.1-devel gtk3-devel librsvg2-devel" >&2
        echo "  Arch:          sudo pacman -S webkit2gtk-4.1 gtk3 librsvg" >&2
        echo "Or run the browser edition: ./run-linux.sh --web" >&2
        exit 1
    fi
    echo "Building the desktop app..."
    # custom-protocol builds the interface into the app (without it, Tauri
    # makes a development build that looks for a dev server).
    cargo build --release -p commander-lab --features tauri/custom-protocol
    BIN=target/release/commander-lab
    # Blank windows on many GPUs otherwise (see packaging/commander-lab.sh).
    export WEBKIT_DISABLE_DMABUF_RENDERER="${WEBKIT_DISABLE_DMABUF_RENDERER:-1}"
else
    echo "Building the browser edition..."
    cargo build --release -p commander-lab-web
    BIN=target/release/commander-lab-web
fi

[ "$BUILD_ONLY" = 1 ] && { echo "Built $BIN"; exit 0; }

eval exec "$BIN" $PASS
