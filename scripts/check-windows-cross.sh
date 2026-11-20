#!/usr/bin/env bash
# Kiem tra nhanh lop Windows tu may khac Windows (vi du macOS): cargo clippy -D warnings cho dich
# x86_64-pc-windows-msvc, GOM CA test (--all-targets). Chi BIEN DICH va clippy, KHONG lien ket va KHONG chay test.
# Khong thay the scripts/check.ps1 va job `windows` cua .github/workflows/check.yml (cong that su, quyet dinh 130).
#
# Cach lam: `ring` (qua `cc`) va `embed-resource` can trinh bien dich C va `llvm-rc` cua MSVC. Clippy chi can
# build script chay xong, nen script nay dung cac chuong trinh gia tao file doi tuong rong trong thu muc tam.
# Cac chuong trinh gia KHONG bao gio duoc dung cho `cargo build`/`cargo test`: tep tao ra se rong.
set -eu
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.."
ROOT=$(pwd)
TARGET=x86_64-pc-windows-msvc

if ! rustup target list --installed 2>/dev/null | grep -qx "$TARGET"; then
    echo "== rustup target add $TARGET"
    rustup target add "$TARGET"
fi

TMP=$(mktemp -d "${TMPDIR:-/tmp}/darkshot-wincross.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/bin"

cat > "$TMP/fakecc.sh" <<'EOS'
#!/bin/bash
out=""
while [ $# -gt 0 ]; do case "$1" in -o) out="$2"; shift;; /Fo*) out="${1#/Fo}";; esac; shift; done
[ -n "$out" ] && : > "$out"
exit 0
EOS
cat > "$TMP/fakear.sh" <<'EOS'
#!/bin/bash
for a in "$@"; do case "$a" in -out:*) : > "${a#-out:}"; exit 0;; *.a|*.lib) : > "$a"; exit 0;; esac; done
exit 0
EOS
cat > "$TMP/bin/llvm-rc" <<'EOS'
#!/bin/bash
for a in "$@"; do case "$a" in /FO*|-FO*) : > "${a#??}";; esac; done
prev=""; for a in "$@"; do [ "$prev" = "/FO" -o "$prev" = "-FO" ] && : > "$a"; prev="$a"; done
exit 0
EOS
chmod +x "$TMP/fakecc.sh" "$TMP/fakear.sh" "$TMP/bin/llvm-rc"

export PATH="$TMP/bin:$PATH"
export CC_x86_64_pc_windows_msvc="$TMP/fakecc.sh"
export AR_x86_64_pc_windows_msvc="$TMP/fakear.sh"
# Thu muc dich rieng (da nam trong /target cua .gitignore) de khong lan voi ban dung that.
export CARGO_TARGET_DIR="$ROOT/target/windows-cross"

echo "== cargo clippy -p darkshot-windows --target $TARGET --all-targets -D warnings"
cargo clippy -p darkshot-windows --target "$TARGET" --all-targets -- -D warnings
echo "WINDOWS CROSS CHECK PASSED (chi bien dich + clippy; chua chay test)"
