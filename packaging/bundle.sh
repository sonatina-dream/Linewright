#!/bin/bash
# Build Linewright.app (release build, app icon, Info.plist, ad-hoc code signature).
# Usage: packaging/bundle.sh [--install]   (--install copies the app to /Applications)
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release

APP="target/Linewright.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/linewright "$APP/Contents/MacOS/linewright"
cp packaging/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
cp packaging/Info.plist "$APP/Contents/Info.plist"
cp LICENSE-MIT LICENSE-APACHE THIRD_PARTY_LICENSES.md "$APP/Contents/Resources/"
codesign --force --deep --sign - "$APP" >/dev/null 2>&1 || echo "warning: ad-hoc codesign failed (the app still runs locally)"

echo "Built $APP"
if [[ "${1:-}" == "--install" ]]; then
    rm -rf "/Applications/Linewright.app"
    cp -R "$APP" /Applications/
    /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "/Applications/Linewright.app" >/dev/null 2>&1 || true
    echo "Installed to /Applications/Linewright.app"
fi
