#!/usr/bin/env sh
# Commander Lab — single-file Linux installer and launcher.
#
#   sh CommanderLab-linux-x86_64.sh            install (first run / update) and open the desktop app
#   sh CommanderLab-linux-x86_64.sh --web      open the browser edition instead
#   sh CommanderLab-linux-x86_64.sh --extract  only install / update
#
# Installs to ~/.local/share/commander-lab/app and adds Commander Lab to your
# app launcher. Your decks live in ~/.local/share/commander-lab and are never
# touched by updates.
#
# The desktop app is an AppImage; on NixOS it runs through appimage-run, which
# the launcher gets from Nix when it isn't installed. The browser edition (one
# static binary) is only used with --web or when the desktop app can't start.
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

# App launcher entry (GNOME, KDE, rofi, wofi, fuzzel...).
APPS="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
if mkdir -p "$APPS" 2>/dev/null; then
    cat > "$APPS/commander-lab.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Commander Lab
Comment=Build and play Magic: The Gathering Commander decks
Exec="$APP/commander-lab.sh"
Icon=$APP/icon.png
Terminal=false
Categories=Game;CardGame;
StartupWMClass=commander-lab
DESKTOP
fi

if [ "$1" = "--extract" ]; then
    echo "Installed. Start it with: $APP/commander-lab.sh"
    exit 0
fi

exec "$APP/commander-lab.sh" "$@"
exit 0
__CL_ARCHIVE_BELOW__
