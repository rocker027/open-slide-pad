#!/bin/zsh
set -euo pipefail
cd "${0:A:h:h}"
cargo build --release --locked
APP_PATH="$PWD/dist/Open Slide Pad.app"
mkdir -p "$APP_PATH/Contents/MacOS" "$APP_PATH/Contents/Resources"
cp target/release/sliderust "$APP_PATH/Contents/MacOS/sliderust"
cp resources/Info.plist "$APP_PATH/Contents/Info.plist"
# 版本以 Cargo.toml 為唯一來源；須在簽章前寫入。
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
[[ -n "$VERSION" ]] || { print -u2 "無法從 Cargo.toml 讀出版本"; exit 1; }
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $VERSION" "$APP_PATH/Contents/Info.plist"
if [[ -f resources/AppIcon.icns ]]; then
  cp resources/AppIcon.icns "$APP_PATH/Contents/Resources/AppIcon.icns"
fi
codesign --force --sign - "$APP_PATH"
codesign --verify --deep --strict "$APP_PATH"
print "已建立：$APP_PATH"
