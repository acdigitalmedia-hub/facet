#!/usr/bin/env bash
# Builds Facet from this folder and adds it to your desktop's app launcher (no root needed).
# For playing a release, the AppImage is simpler; this is for running the development copy.
# Usage: scripts/install-desktop.sh            install / update
#        scripts/install-desktop.sh --remove   remove the launcher entry and icon
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
ICON="$DATA/icons/hicolor/256x256/apps/facet.png"
ENTRY="$DATA/applications/facet.desktop"
# Launcher from before the 2D version was retired.
OLD_ENTRY="$DATA/applications/facet3d.desktop"

if [[ "${1:-}" == "--remove" ]]; then
    rm -f "$ICON" "$ENTRY" "$OLD_ENTRY"
    update-desktop-database "$DATA/applications" 2>/dev/null || true
    echo "Removed Facet launcher."
    exit 0
fi

cd "$ROOT"
cargo build --release
mkdir -p "$(dirname "$ICON")" "$(dirname "$ENTRY")"
cp packaging/facet.png "$ICON"
rm -f "$OLD_ENTRY"

# Path= runs it from the project, so the editor can also change the built-in maps.
cat > "$ENTRY" <<EOF
[Desktop Entry]
Type=Application
Name=Facet
GenericName=Tower Defense
Comment=A 3D gem maze tower defense
Exec="$ROOT/target/release/facet"
Path=$ROOT
Icon=facet
Terminal=false
Categories=Game;StrategyGame;
StartupWMClass=facet
EOF

update-desktop-database "$DATA/applications" 2>/dev/null || true
gtk-update-icon-cache -q "$DATA/icons/hicolor" 2>/dev/null || true
echo "Installed: look for 'Facet' in your app launcher."
