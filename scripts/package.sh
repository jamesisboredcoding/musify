#!/usr/bin/env bash
# Builds release packages into dist/: an RPM (needs rpmbuild) and an AppImage
# (downloads appimagetool into ~/.cache/musify-build the first time).
# Usage: scripts/package.sh
set -euo pipefail
cd "$(dirname "$0")/.."

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
arch=$(uname -m)
dist="$PWD/dist"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$dist"

cargo build --release
bin=target/release/musify
strip -o "$work/musify" "$bin"

# --- RPM ---
top="$work/rpm"
mkdir -p "$top/SOURCES"
cp "$work/musify" LICENSE assets/musify.desktop assets/icons/musify.svg assets/icons/musify-256.png "$top/SOURCES/"
rpmbuild -bb packaging/musify.spec --define "_topdir $top" --define "version $version" --quiet
cp "$top"/RPMS/*/*.rpm "$dist/"

# --- AppImage ---
tool="${XDG_CACHE_HOME:-$HOME/.cache}/musify-build/appimagetool-$arch.AppImage"
if [ ! -x "$tool" ]; then
    mkdir -p "$(dirname "$tool")"
    curl -fsSL -o "$tool" "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$arch.AppImage"
    chmod +x "$tool"
fi
app="$work/Musify.AppDir"
install -Dm755 "$work/musify" "$app/usr/bin/musify"
install -Dm644 assets/musify.desktop "$app/usr/share/applications/musify.desktop"
install -Dm644 assets/icons/musify.svg "$app/usr/share/icons/hicolor/scalable/apps/musify.svg"
install -Dm644 assets/icons/musify-256.png "$app/usr/share/icons/hicolor/256x256/apps/musify.png"
cp assets/musify.desktop "$app/musify.desktop"
cp assets/icons/musify-256.png "$app/musify.png"
cat > "$app/AppRun" <<'EOF'
#!/bin/sh
exec "$(dirname "$(readlink -f "$0")")/usr/bin/musify" "$@"
EOF
chmod +x "$app/AppRun"
ARCH="$arch" "$tool" --no-appstream "$app" "$dist/Musify-$version-$arch.AppImage" >/dev/null 2>&1 \
    || ARCH="$arch" APPIMAGE_EXTRACT_AND_RUN=1 "$tool" --no-appstream "$app" "$dist/Musify-$version-$arch.AppImage"

ls -lh "$dist"
