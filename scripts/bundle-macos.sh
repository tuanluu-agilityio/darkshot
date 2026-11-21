#!/usr/bin/env bash
# Dung goi target/macos/darkshot.app tu crate darkshot-macos roi ky (quyet dinh 137).
#
#   scripts/bundle-macos.sh [--release]
#
# Danh tinh ky lay tu bien moi truong DARKSHOT_SIGN_IDENTITY (ten chung chi tu ky cuc bo, xem
# docs/testing/macos-local-signing.md). Khong dat thi ky ad-hoc ("-"): quyen Screen Recording va
# Keychain se bi hoi lai sau moi lan build. Script khong in danh tinh ky va chi xoa trong
# target/macos.
set -eu
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.."

BUNDLE_ID="app.darkshot.macos"
USAGE_EN="darkshot captures the screen only when you choose Take screenshot."
USAGE_VI="darkshot chỉ chụp màn hình khi bạn chọn Chụp màn hình."
profile="debug"
release_flag=""
case "${1:-}" in
    "") ;;
    --release)
        profile="release"
        release_flag="--release"
        ;;
    *)
        echo "usage: scripts/bundle-macos.sh [--release]" >&2
        exit 2
        ;;
esac

# shellcheck disable=SC2086 # release_flag rong hoac mot tu.
cargo build -p darkshot-macos $release_flag

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' crates/darkshot-macos/Cargo.toml | head -n 1)
if [ -z "$version" ]; then
    echo "bundle-macos: cannot read the version from crates/darkshot-macos/Cargo.toml" >&2
    exit 1
fi

bin="target/$profile/darkshot-macos"
out_dir="target/macos"
app="$out_dir/darkshot.app"
[ -f "$bin" ] || { echo "bundle-macos: missing $bin" >&2; exit 1; }

# Chi xoa goi cu trong target/macos.
mkdir -p "$out_dir"
rm -rf "$out_dir/darkshot.app"
mkdir -p "$app/Contents/MacOS"
cp "$bin" "$app/Contents/MacOS/darkshot"

# Bieu tuong (scripts/make-icns.sh tao target/macos/darkshot.icns). Khong co thi bo qua, khong loi.
icon_plist=""
if [ -f "$out_dir/darkshot.icns" ]; then
    mkdir -p "$app/Contents/Resources"
    cp "$out_dir/darkshot.icns" "$app/Contents/Resources/darkshot.icns"
    icon_plist="    <key>CFBundleIconFile</key>
    <string>darkshot</string>"
else
    echo "bundle-macos: no $out_dir/darkshot.icns (run scripts/make-icns.sh first); building without an icon." >&2
fi

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>darkshot</string>
    <key>CFBundleIdentifier</key>
    <string>$BUNDLE_ID</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
$icon_plist
    <key>CFBundleName</key>
    <string>darkshot</string>
    <key>CFBundleDisplayName</key>
    <string>darkshot</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$version</string>
    <key>CFBundleVersion</key>
    <string>$version</string>
    <key>LSMinimumSystemVersion</key>
    <string>14.0</string>
    <key>LSUIElement</key>
    <true/>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSScreenCaptureUsageDescription</key>
    <string>$USAGE_EN</string>
    <key>CFBundleLocalizations</key>
    <array>
        <string>en</string>
        <string>vi</string>
    </array>
</dict>
</plist>
PLIST
plutil -lint "$app/Contents/Info.plist"

# Ban dich cua chuoi giai thich quyen (InfoPlist.strings, UTF-8).
for lang in en vi; do
    mkdir -p "$app/Contents/Resources/$lang.lproj"
    if [ "$lang" = vi ]; then text="$USAGE_VI"; else text="$USAGE_EN"; fi
    printf '"NSScreenCaptureUsageDescription" = "%s";\n' "$text" \
        > "$app/Contents/Resources/$lang.lproj/InfoPlist.strings"
    plutil -lint "$app/Contents/Resources/$lang.lproj/InfoPlist.strings"
done

identity="${DARKSHOT_SIGN_IDENTITY:--}"
if [ "$identity" = "-" ]; then
    echo "WARNING: DARKSHOT_SIGN_IDENTITY is not set; signing ad-hoc." >&2
    echo "WARNING: macOS will ask again for Screen Recording and Keychain access after every build." >&2
    echo "WARNING: see docs/testing/macos-local-signing.md to create a stable local signing certificate." >&2
    sign_kind="ad-hoc"
else
    sign_kind="local certificate"
fi
# Loi cua codesign co the nhac ten danh tinh: khong in ra.
if ! codesign --force --sign "$identity" --identifier "$BUNDLE_ID" "$app" 2>/dev/null; then
    echo "bundle-macos: codesign failed ($sign_kind). Check that DARKSHOT_SIGN_IDENTITY names a" >&2
    echo "bundle-macos: code signing certificate in your login keychain." >&2
    exit 1
fi
codesign --verify --strict "$app"

echo "Built $app (version $version, $profile, signed: $sign_kind)"
