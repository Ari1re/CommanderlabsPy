#!/usr/bin/env sh
# Commander Lab — single-file Linux installer + launcher.
#
#   sh CommanderLab-linux-x86_64.sh            # install (first run) and start
#   sh CommanderLab-linux-x86_64.sh --extract  # only install/update
#
# The app is unpacked to ~/.local/share/commander-lab/app and re-unpacked only
# when this file contains a newer build. Your decks are kept in
# ~/.local/share/commander-lab and are never touched by updates.
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
    chmod +x "$APP/commander-lab" "$APP/commander-lab.sh"
fi

if [ "$1" = "--extract" ]; then
    echo "Installed. Run: $APP/commander-lab.sh"
    exit 0
fi

exec "$APP/commander-lab.sh" "$@"
exit 0
__CL_ARCHIVE_BELOW__
