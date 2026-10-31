#!/usr/bin/env sh
# Build and run Commander Lab straight from this git checkout (Linux, NixOS).
#
#   ./run-linux.sh             browser edition: works everywhere, incl. NixOS
#   ./run-linux.sh --desktop   desktop window (needs webkit2gtk; see README)
#   ./run-linux.sh --update    git pull, rebuild, then run (combine with --desktop)
#   ./run-linux.sh --build     build only
#
# Anything else is passed to the app, e.g. ./run-linux.sh --port 8080
# On NixOS without cargo/node on your PATH it re-runs itself in shell.nix.
# Your decks live in ~/.local/share/commander-lab, shared with installed copies.
set -e
cd "$(dirname "$0")"

DESKTOP=0
UPDATE=0
BUILD_ONLY=0
PASS=""
for arg in "$@"; do
    case "$arg" in
        --desktop) DESKTOP=1 ;;
        --update) UPDATE=1 ;;
        --build) BUILD_ONLY=1 ;;
        *) PASS="$PASS \"$arg\"" ;;
    esac
done

have() { command -v "$1" >/dev/null 2>&1; }

# NixOS: get the toolchain from shell.nix when it isn't installed.
if { ! have cargo || ! have npm; } && [ -z "$IN_NIX_SHELL" ]; then
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
    echo "Building the desktop app..."
    cargo build --release -p commander-lab
    BIN=target/release/commander-lab
else
    echo "Building the browser edition..."
    cargo build --release -p commander-lab-web
    BIN=target/release/commander-lab-web
fi

[ "$BUILD_ONLY" = 1 ] && { echo "Built $BIN"; exit 0; }

eval exec "$BIN" $PASS
