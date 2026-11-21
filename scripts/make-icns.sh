#!/usr/bin/env bash
# Tao target/macos/darkshot.icns tu bieu tuong ung dung (crates/darkshot-windows/res/darkshot.ico).
#
#   scripts/make-icns.sh
#
# Anh nguon la anh lon nhat co san trong repo: ban 256 px trong .ico (khong co anh 512 hay 1024 px).
# Chi tao cac co khong phong to (<= 256 px); thieu ban 256@2x (512 px) va 512, macOS tu co gian khi
# can. Chi dung sips va iconutil (co san tren macOS) va chi ghi trong target/macos.
set -eu
cd "$(dirname "$0")/.."

src="crates/darkshot-windows/res/darkshot.ico"
out_dir="target/macos"
work="$out_dir/darkshot.iconset.tmp"
png="$work/source.png"
iconset="$out_dir/darkshot.iconset"
[ -f "$src" ] || { echo "make-icns: missing $src" >&2; exit 1; }

mkdir -p "$out_dir"
rm -rf "$work" "$iconset"
mkdir -p "$work" "$iconset"
sips -s format png "$src" --out "$png" >/dev/null
width=$(sips -g pixelWidth "$png" | sed -n 's/.*pixelWidth: //p')
if [ -z "$width" ]; then
    echo "make-icns: cannot read the size of the source image" >&2
    exit 1
fi
echo "make-icns: source is ${width}px; sizes above that are skipped (no upscaling)."

# Ten tep iconset: kich thuoc diem (+ @2x) -> kich thuoc pixel.
make() {
    name="$1"
    px="$2"
    if [ "$px" -le "$width" ]; then
        sips -z "$px" "$px" "$png" --out "$iconset/$name" >/dev/null
    fi
}
make icon_16x16.png 16
make icon_16x16@2x.png 32
make icon_32x32.png 32
make icon_32x32@2x.png 64
make icon_128x128.png 128
make icon_128x128@2x.png 256
make icon_256x256.png 256
make icon_256x256@2x.png 512
make icon_512x512.png 512
make icon_512x512@2x.png 1024

iconutil -c icns "$iconset" -o "$out_dir/darkshot.icns"
rm -rf "$work" "$iconset"
echo "Built $out_dir/darkshot.icns"
