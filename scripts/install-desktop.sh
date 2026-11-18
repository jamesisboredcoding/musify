#!/usr/bin/env bash
# Installs Musify's icon and launcher entry for the current user, so the desktop
# (KDE, GNOME, …) shows the Musify icon instead of a generic one.
# Usage: scripts/install-desktop.sh [path-to-musify-binary]
set -euo pipefail
cd "$(dirname "$0")/.."
bin="${1:-musify}"
share="${XDG_DATA_HOME:-$HOME/.local/share}"
install -Dm644 assets/icons/musify.svg "$share/icons/hicolor/scalable/apps/musify.svg"
install -Dm644 assets/icons/musify-256.png "$share/icons/hicolor/256x256/apps/musify.png"
sed "s|^Exec=musify|Exec=$bin|" assets/musify.desktop > "$share/applications/musify.desktop"
chmod 644 "$share/applications/musify.desktop"
command -v update-desktop-database >/dev/null && update-desktop-database "$share/applications" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q "$share/icons/hicolor" 2>/dev/null || true
command -v kbuildsycoca6 >/dev/null && kbuildsycoca6 >/dev/null 2>&1 || true
echo "Installed Musify launcher ($bin)."
