#!/usr/bin/env bash
# glibc ゲートが新し過ぎる要求を確実に拒否することを、ELF を偽造せず検査する。
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/bin"
cat > "$tmp/bin/readelf" <<'SH'
#!/usr/bin/env bash
printf 'Name: GLIBC_2.2.5\nName: GLIBC_2.34\nName: GLIBC_%s\n' "$TEST_GLIBC"
SH
cat > "$tmp/llmsearch" <<'SH'
#!/usr/bin/env bash
case "$*" in
    --version) echo 'llmsearch 0.1.0' ;;
    *'engines list') echo duckduckgo ;;
    *'config show') echo search.engine ;;
    *) exit 1 ;;
esac
SH
chmod +x "$tmp/bin/readelf" "$tmp/llmsearch"
for version in 2.34 2.35; do
    PATH="$tmp/bin:$PATH" TEST_GLIBC="$version" bash "$root/scripts/check-release.sh" "$tmp/llmsearch"
done
for version in 2.36 2.39 2.43; do
    if PATH="$tmp/bin:$PATH" TEST_GLIBC="$version" bash "$root/scripts/check-release.sh" "$tmp/llmsearch" > "$tmp/output" 2>&1; then
        echo "error: GLIBC_$version should have been rejected" >&2
        exit 1
    fi
    grep "exceeds baseline GLIBC_2.35" "$tmp/output"
done
echo 'glibc gate regression checks passed'
