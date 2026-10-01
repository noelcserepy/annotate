#!/bin/sh
# Build Annotate.app. `scripts/bundle.sh install` moves it to /Applications and launches it,
# so only one copy carries the bundle ID.
set -e
cd "$(dirname "$0")/.."
cargo build --release
APP=target/Annotate.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/annotate "$APP/Contents/MacOS/annotate"
# build.rs renders the icon into its OUT_DIR; take the newest.
cp "$(ls -t target/release/build/annotate-*/out/AppIcon.icns | head -1)" "$APP/Contents/Resources/AppIcon.icns"
cp Info.plist "$APP/Contents/Info.plist"
KC=~/Library/Keychains/annotate.keychain-db
if security find-identity -p codesigning "$KC" 2>/dev/null | grep -q "Annotate Dev"; then
    security unlock-keychain -p annotate "$KC"
    codesign --force --keychain "$KC" --sign "Annotate Dev" "$APP"
else
    echo "no 'Annotate Dev' identity (run scripts/make-cert.sh); ad-hoc signing"
    codesign --force --sign - "$APP"
fi
if [ "$1" = "install" ]; then
    pkill -x annotate || true
    rm -rf /Applications/Annotate.app
    mv "$APP" /Applications/Annotate.app
    open /Applications/Annotate.app
fi
