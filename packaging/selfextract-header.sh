#!/usr/bin/env sh
# Commander Lab — single-file Linux installer and launcher.
#
#   sh CommanderLab-linux-x86_64.sh            install (first run / update) and start
#   sh CommanderLab-linux-x86_64.sh --web      start the browser edition instead
#   sh CommanderLab-linux-x86_64.sh --extract  only install / update
#
# Installs to ~/.local/share/commander-lab/app. Your decks live in
# ~/.local/share/commander-lab and are never touched by updates.
#
# The desktop window is an AppImage. On NixOS (or anywhere it can't start) the
# browser edition — one static binary with no dependencies — is used instead.
set -e

VERSION="__CL_VERSION__"
BASE="${COMMANDER_LAB_HOME:-${XDG_DATA_HOME:-$HOME/.local/share}/commander-lab}"
APP="$BASE/app"

if [ ! -f "$APP/VERSION" ] || [ "$(cat "$APP/VERSION")" != "$VERSION" ]; then
    echo "Installing Commander Lab $VERSION to $APP ..."
    rm -rf "$APP"
    mkdir -p "$APP"
    LINE=$(awk '/^__CL_ARCHIVE_BELOW__$/ { print NR + 1; exit 0 }' "$0")
    tail -n +"$LINE" "$0" | tar -xz -C "$APP"
    echo "$VERSION" > "$APP/VERSION"
    chmod +x "$APP"/commander-lab.sh "$APP"/CommanderLab.AppImage "$APP"/commander-lab-web 2>/dev/null || true
fi

if [ "$1" = "--extract" ]; then
    echo "Installed. Start it with: $APP/commander-lab.sh"
    exit 0
fi

exec "$APP/commander-lab.sh" "$@"
exit 0
__CL_ARCHIVE_BELOW__
