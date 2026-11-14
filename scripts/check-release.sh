#!/usr/bin/env bash
# GNU Linux x86_64 配布物の glibc 基準と実起動を検証する。
# Usage: bash scripts/check-release.sh <binary> [--containers]
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

smoke='set -eu
"$1" --version | grep -E "^llmsearch [0-9]"
state="$(mktemp -d)"
trap '\''rm -rf "$state"'\'' EXIT
"$1" --state-dir "$state" engines list | grep duckduckgo
"$1" --state-dir "$state" config show | grep search.engine'
# いずれも外部 HTTP を行わないコマンドのみ。
bash -c "$smoke" _ "$binary"
if [ "${2:-}" = --containers ]; then
    for image in ubuntu:22.04 ubuntu:24.04; do
        echo "Runtime smoke: $image"
        docker run --rm --network none --read-only --tmpfs /tmp:rw,mode=1777 \
            -e HOME=/tmp -v "$binary:/llmsearch:ro" "$image" \
            bash -c "$smoke" _ /llmsearch
    done
fi
