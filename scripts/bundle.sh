#!/bin/zsh
set -euo pipefail
cd "${0:A:h:h}"
cargo build --release --locked
APP_PATH="$PWD/dist/Open Slide Pad.app"
mkdir -p "$APP_PATH/Contents/MacOS" "$APP_PATH/Contents/Resources"
cp target/release/sliderust "$APP_PATH/Contents/MacOS/sliderust"
cp resources/Info.plist "$APP_PATH/Contents/Info.plist"
if [[ -f resources/AppIcon.icns ]]; then
  cp resources/AppIcon.icns "$APP_PATH/Contents/Resources/AppIcon.icns"
fi
codesign --force --sign - "$APP_PATH"
codesign --verify --deep --strict "$APP_PATH"
print "已建立：$APP_PATH"
