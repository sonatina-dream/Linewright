#!/bin/bash
# Regenerate packaging/AppIcon.icns from packaging/icon/linewright.svg.
set -euo pipefail
cd "$(dirname "$0")/.."
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
cargo run --quiet --example render_icon -- "$TMP/AppIcon.iconset"
iconutil -c icns "$TMP/AppIcon.iconset" -o packaging/AppIcon.icns
echo "wrote packaging/AppIcon.icns"
