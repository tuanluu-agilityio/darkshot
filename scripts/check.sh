#!/usr/bin/env bash
# Cong kiem tra truoc khi gop nhanh tren macOS/Linux. In "ALL CHECKS PASSED" va thoat 0 khi moi thu qua.
# Phai giu cung danh sach kiem voi scripts/check.ps1 (ban Windows); sua mot ben thi sua ben kia.
set -u
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.."

step() {
    echo "== $1"; shift
    "$@" || { echo "CHECK FAILED: $*"; exit 1; }
}

step 'cargo fmt --check' cargo fmt --all --check
step 'cargo test' cargo test --workspace
step 'cargo clippy' cargo clippy --workspace --all-targets -- -D warnings

echo '== darkshot-core dependencies'
tree=$(cargo tree -p darkshot-core -e normal --prefix none) || { echo 'CHECK FAILED: cargo tree'; exit 1; }
banned=$(printf '%s\n' "$tree" | grep -E '^(windows|native-tls|schannel|rustls-platform-verifier) v' | sort -u)
if [ -n "$banned" ]; then
    echo 'CHECK FAILED: darkshot-core depends on a banned crate:'
    printf '%s\n' "$banned" | sed 's/^/  /'
    exit 1
fi

echo '== no hard-coded Vietnamese strings'
python3 -I - <<'PY' || exit 1
import glob, os, re, sys
# Chu co dau Latin (U+00C0-U+024F) va Latin mo rong (U+1E00-U+1EFF) trong chuoi.
pat = re.compile('"[^"]*[À-ɏḀ-ỿ][^"]*"')
bad = False
for path in sorted(glob.glob('crates/*/src/*.rs')):
    if os.path.basename(path) == 'i18n.rs':
        continue
    lines = open(path, encoding='utf-8').read().split('\n')
    for i, line in enumerate(lines):
        if re.search(r'#\[cfg\(test\)\]', line):
            j = i + 1
            while j < len(lines) and lines[j].strip() == '':
                j += 1
            if j < len(lines) and re.match(r'\s*mod\s', lines[j]):
                break
            continue
        code = re.sub(r'//.*$', '', line)
        if pat.search(code):
            print(f'CHECK FAILED: hard-coded string in {os.path.basename(path)}:{i + 1}')
            bad = True
sys.exit(1 if bad else 0)
PY

echo '== no legacy name'
python3 -I - <<'PY' || exit 1
import os, re, sys
legacy = 'shot' + 'me'   # so khop KHONG phan biet hoa thuong, nhu Select-String cua check.ps1
skip = {'crates/darkshot-windows/src/legacy.rs', 'crates/darkshot-core/src/migrate.rs'}
exempt_heading = {
    'README.md': '## Chuyển từ ' + legacy,
    'docs/testing/manual-checklist.md': '## Đợt 4: chuyển từ ' + legacy,
}
files = []
for root in ('crates', 'scripts', 'docs/testing'):
    for d, dirs, names in os.walk(root):
        dirs[:] = [x for x in dirs if x != 'target']
        files += [os.path.join(d, n) for n in names]
files.append('README.md')
found = False
for rel in sorted(files):
    if rel in skip or rel == 'scripts/check.sh' or rel.endswith(('.ico', '.png')):
        continue
    if rel == 'README.md' or rel.startswith('docs/testing/'):
        exempt = False
        for i, line in enumerate(open(rel, encoding='utf-8').read().split('\n')):
            if line.startswith('## '):
                exempt = line.rstrip() == exempt_heading.get(rel)
            if exempt or re.search(r'tuanluu-agilityio/|2026 - Tuan', line):
                continue
            if legacy in line.lower():
                print(f'CHECK FAILED: legacy name in {rel}:{i + 1}'); found = True
        continue
    try:
        text = open(rel, encoding='utf-8').read()
    except (UnicodeDecodeError, OSError):
        continue
    for i, line in enumerate(text.split('\n')):
        if legacy in line.lower():
            print(f'CHECK FAILED: legacy name in {rel}:{i + 1}'); found = True
sys.exit(1 if found else 0)
PY

echo 'ALL CHECKS PASSED'
