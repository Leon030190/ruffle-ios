#!/usr/bin/env bash
# Script đóng gói Ruffle thành file .ipa trên macOS

set -euo pipefail

echo "==> Đang kiểm tra toolchain Rust & target iOS..."
rustup target add aarch64-apple-ios || true

echo "==> Đang biên dịch Xcode Project (Release cho iPhone device)..."
xcodebuild clean build \
  -project ruffle-ios.xcodeproj \
  -scheme "ruffle-ios" \
  -configuration Release \
  -destination 'generic/platform=iOS' \
  -derivedDataPath build \
  CODE_SIGNING_ALLOWED=NO \
  CODE_SIGNING_REQUIRED=NO \
  CODE_SIGN_IDENTITY="" \
  AD_HOC_CODE_SIGNING_ALLOWED=YES

echo "==> Đang tìm kiếm bundle .app..."
APP_DIR=$(find build/Build/Products/Release-iphoneos -name "*.app" -type d | head -n 1)

if [ -z "$APP_DIR" ]; then
  echo "Lỗi: Không tìm thấy thư mục .app!"
  exit 1
fi

echo "==> Đang đóng gói thành Ruffle.ipa..."
rm -rf Payload Ruffle.ipa
mkdir -p Payload
cp -r "$APP_DIR" Payload/
zip -qr Ruffle.ipa Payload
rm -rf Payload

echo "==> HOÀN TẤT: Đã tạo file Ruffle.ipa thành công!"
echo "    Bạn có thể dùng Sideloadly, AltStore hoặc TrollStore để cài vào iPhone."
