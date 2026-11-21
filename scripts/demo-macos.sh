#!/usr/bin/env bash
# Chay thu darkshot cho macOS trong ho so cach ly (khong dung cau hinh, anh hay khoa R2 that).
#
#   scripts/demo-macos.sh [--release]
#
# Dung goi bang scripts/bundle-macos.sh, dat DARKSHOT_PROFILE_DIR ve mot thu muc tam rieng, mo
# ban dung bang `open -n -W` (cho toi khi thoat tu menu), roi xoa muc Keychain cua ho so cach ly
# (service darkshot-sandbox-..., khong bao gio la service `darkshot` that, quyet dinh 138).
set -eu
cd "$(dirname "$0")/.."

bash scripts/bundle-macos.sh "$@"

app="target/macos/darkshot.app"
bin="$app/Contents/MacOS/darkshot"
profile=$(mktemp -d "${TMPDIR:-/tmp}/darkshot-demo.XXXXXX")

# Ten service Keychain do chinh ung dung tinh cho ho so nay (khong cham Keychain, khong hien UI).
service=$(DARKSHOT_PROFILE_DIR="$profile" "$bin" --keychain-service)
case "$service" in
    darkshot-sandbox-*) ;;
    *)
        echo "demo-macos: unexpected keychain service '$service'; refusing to continue." >&2
        exit 1
        ;;
esac

cleanup() {
    echo
    echo "== Cleaning up the sandbox keychain item:"
    echo "   security delete-generic-password -s $service -a r2-secret-access-key"
    security delete-generic-password -s "$service" -a r2-secret-access-key >/dev/null 2>&1 || true
    echo "== Sandbox profile kept for inspection (screenshots are in $profile/Pictures/darkshot)."
    echo "   Remove it when done:  rm -r \"$profile\""
}
trap cleanup EXIT

cat <<EOF

== darkshot demo (isolated profile)
   Profile:  $profile
   Keychain: service $service (the real 'darkshot' item is never used)

Try:
  1. A camera icon appears in the menu bar; there is no Dock icon.
  2. The menu lists only "Take screenshot" and "Quit darkshot" (in the system language).
  3. Choose "Take screenshot": the first time macOS asks for Screen Recording permission;
     without it a notification explains how to enable it (reopen darkshot after granting).
  4. With permission: every display shows the dimmed screenshot. Drag to select a region
     (the label shows its size in real pixels); move it or resize it with the eight handles.
     Pressing on another display starts a new selection there (a region never spans displays).
  5. After releasing the mouse a Save / Copy / Cancel bar appears next to the region.
     Save (or Cmd+S) writes a PNG into $profile/Pictures/darkshot and a notification shows
     the path; Copy (or Cmd+C) puts the image on the clipboard; Cancel or Esc closes the
     overlay at any time. Switching apps (Cmd+Tab) or changing displays also closes it, and it
     closes by itself after 120 s without input.
  6. While the demo is open, run in another terminal:
       open -n --env "DARKSHOT_PROFILE_DIR=$profile" "$app"
     A second copy for the same profile exits at once (no second menu bar icon).
  7. Quit from the menu; this script then removes the sandbox keychain item.

Logs (stderr): $profile/darkshot.log
EOF

open -n -W --env "DARKSHOT_PROFILE_DIR=$profile" --stderr "$profile/darkshot.log" "$app"
