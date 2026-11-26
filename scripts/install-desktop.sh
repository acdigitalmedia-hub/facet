#!/usr/bin/env bash
# Builds Facet and adds it to your desktop's app launcher (no root needed).
# Usage: scripts/install-desktop.sh            install / update
#        scripts/install-desktop.sh --remove   remove the launcher entry and icon
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
ICON="$DATA/icons/hicolor/256x256/apps/facet.png"
ENTRY="$DATA/applications/facet.desktop"

if [[ "${1:-}" == "--remove" ]]; then
    rm -f "$ICON" "$ENTRY"
    update-desktop-database "$DATA/applications" 2>/dev/null || true
    echo "Removed Facet launcher."
    exit 0
fi

cd "$ROOT"
cargo build --release --bins
mkdir -p "$(dirname "$ICON")" "$(dirname "$ENTRY")"
"$ROOT/target/release/make-icon" "$ICON" 256

cat > "$ENTRY" <<EOF
[Desktop Entry]
Type=Application
Name=Facet
GenericName=Tower Defense
Comment=A gem maze tower defense
Exec="$ROOT/target/release/facet"
Path=$ROOT
Icon=facet
Terminal=false
Categories=Game;StrategyGame;
EOF

update-desktop-database "$DATA/applications" 2>/dev/null || true
gtk-update-icon-cache -q "$DATA/icons/hicolor" 2>/dev/null || true
echo "Installed: look for 'Facet' in your app launcher."
