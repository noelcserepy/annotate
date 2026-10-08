#!/bin/sh
# Build Annotate.app.
#   scripts/bundle.sh            for this Mac
#   scripts/bundle.sh install    and move it to /Applications and launch it, so only one copy carries the bundle ID
#   scripts/bundle.sh universal  for arm64 and x86_64, as the release ships it
# Signs with the identity named in ANNOTATE_SIGN_IDENTITY, else ad-hoc.
set -e
cd "$(dirname "$0")/.."
if [ "$1" = "universal" ]; then
    for target in aarch64-apple-darwin x86_64-apple-darwin; do
        cargo build --release --locked --target "$target"
    done
    lipo -create -output target/annotate target/aarch64-apple-darwin/release/annotate target/x86_64-apple-darwin/release/annotate
    BIN=target/annotate
    BUILD=target/aarch64-apple-darwin/release/build
else
    cargo build --release
    BIN=target/release/annotate
    BUILD=target/release/build
fi
APP=target/Annotate.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/annotate"
# build.rs renders the icon into its OUT_DIR; take the newest.
cp "$(ls -t "$BUILD"/annotate-*/out/AppIcon.icns | head -1)" "$APP/Contents/Resources/AppIcon.icns"
cp Info.plist "$APP/Contents/Info.plist"
codesign --force --sign "${ANNOTATE_SIGN_IDENTITY:--}" "$APP"
if [ "$1" = "install" ]; then
    pkill -x annotate || true
    rm -rf /Applications/Annotate.app
    mv "$APP" /Applications/Annotate.app
    open /Applications/Annotate.app
fi
