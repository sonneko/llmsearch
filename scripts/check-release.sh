#!/usr/bin/env bash
# GNU Linux x86_64 配布物の glibc 基準と実起動を検証する。
# Usage: bash scripts/check-release.sh <binary> [--containers]
#
# 配布物の形（仕様 testing.md §1.5）:
#   1. normal     バイナリの隣に完全な `assets/` がある
#   2. no-assets  `assets/` が無い（埋め込み既定で起動できること）
#   3. nested     `assets/assets/` にだけ既定値がある入れ子の形（M11 の事故そのもの）
#
# 2 と 3 は「既定値がファイルとして見つからない」状態なので、埋め込み既定
# （`crate::assets`）で起動できなければ失敗する。3 は今回の CI 事故の再現形である。
set -euo pipefail

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ] || { [ "$#" = 2 ] && [ "$2" != --containers ]; }; then
    echo 'usage: check-release.sh <binary> [--containers]' >&2
    exit 2
fi
binary="$(realpath "$1")"
baseline=2.35
versions="$(readelf --version-info --wide "$binary")"
required="$(printf '%s\n' "$versions" | grep -oE 'GLIBC_[0-9]+(\.[0-9]+)+' | sed 's/GLIBC_//' | sort -Vu | tail -1)"
if [ -z "$required" ] || [ "$(printf '%s\n' "$baseline" "$required" | sort -V | tail -1)" != "$baseline" ]; then
    echo "error: required GLIBC_${required:-unknown} exceeds baseline GLIBC_${baseline}" >&2
    exit 1
fi
echo "glibc requirement: $required (baseline: $baseline)"

package_dir="$(dirname "$binary")"
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# 3 形態を 1 か所に組み立てる。コンテナへはこの木をそのまま読み取り専用で渡す。
shapes="$work/shapes"
mkdir -p "$shapes/normal" "$shapes/no-assets" "$shapes/nested/assets"
for shape in normal no-assets nested; do
    cp "$binary" "$shapes/$shape/llmsearch"
done
if [ -d "$package_dir/assets" ]; then
    cp -R "$package_dir/assets" "$shapes/normal/assets"
else
    echo "note: no assets/ next to the binary, only the embedded shapes are checked"
fi
if [ -d "$package_dir/assets" ]; then
    cp -R "$package_dir/assets" "$shapes/nested/assets/assets"
fi

bash "$script_dir/check-shapes.sh" "$shapes"

if [ "${2:-}" = --containers ]; then
    for image in ubuntu:22.04 ubuntu:24.04; do
        echo "Runtime smoke: $image"
        docker run --rm --network none --read-only --tmpfs /tmp:rw,mode=1777 \
            -e HOME=/tmp \
            -v "$shapes:/opt/shapes:ro" \
            -v "$script_dir:/opt/scripts:ro" "$image" \
            bash /opt/scripts/check-shapes.sh /opt/shapes
    done
fi
