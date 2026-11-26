#!/usr/bin/env bash
# Builds dist/Facet-<version>-x86_64.AppImage.
# Usage: scripts/build-appimage.sh [--no-build]   (--no-build packages the existing release binary)
# Needs appimagetool: set APPIMAGETOOL, put it on PATH, or it's downloaded once to target/tools.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
VERSION="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
ARCH=x86_64
APPDIR="$ROOT/target/AppDir"
OUT="$ROOT/dist/Facet-$VERSION-$ARCH.AppImage"

[[ "${1:-}" == "--no-build" ]] || cargo build --release --locked

TOOL="${APPIMAGETOOL:-$(command -v appimagetool || true)}"
if [[ -z "$TOOL" ]]; then
    TOOL="$ROOT/target/tools/appimagetool-$ARCH.AppImage"
    if [[ ! -x "$TOOL" ]]; then
        mkdir -p "$(dirname "$TOOL")"
        curl -fL -o "$TOOL" "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$ARCH.AppImage"
        chmod +x "$TOOL"
    fi
fi

rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/facet" "$APPDIR/usr/share/applications" \
         "$APPDIR/usr/share/icons/hicolor/256x256/apps"
install -m755 target/release/facet "$APPDIR/usr/bin/facet"
cp -r assets "$APPDIR/usr/share/facet/assets"
cp packaging/facet.desktop "$APPDIR/facet.desktop"
cp packaging/facet.desktop "$APPDIR/usr/share/applications/facet.desktop"
cp packaging/facet.png "$APPDIR/facet.png"
cp packaging/facet.png "$APPDIR/usr/share/icons/hicolor/256x256/apps/facet.png"
ln -s facet.png "$APPDIR/.DirIcon"
cat > "$APPDIR/AppRun" <<'RUN'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
export FACET_ASSETS="$HERE/usr/share/facet/assets"
exec "$HERE/usr/bin/facet" "$@"
RUN
chmod +x "$APPDIR/AppRun"

mkdir -p dist
# APPIMAGE_EXTRACT_AND_RUN lets appimagetool run without FUSE (e.g. in CI).
ARCH=$ARCH APPIMAGE_EXTRACT_AND_RUN=1 "$TOOL" --no-appstream "$APPDIR" "$OUT"
echo "Built $OUT"
