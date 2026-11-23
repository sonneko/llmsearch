#!/usr/bin/env bash
# 配布物の 3 形態でスモークを実行する（`check-release.sh` から呼ばれる内部用）。
#
# Usage: bash scripts/check-shapes.sh <shapes-root>
#
# <shapes-root> には次の 3 ディレクトリが用意されていること。
#   normal/     バイナリ + 完全な assets/
#   no-assets/  バイナリのみ（埋め込み既定で起動することを見る）
#   nested/     バイナリ + assets/assets/ のみ（M11 の CI 事故の形）
#
# 実行バイナリは読み取り専用のマウント先にあってよい（コピーしない）。
# コンテナの `/tmp` は `noexec` でマウントされるため、ここでは実行ファイルを
# `/tmp` へコピーしない。状態ディレクトリだけを書き込み可能な場所に作る。
set -euo pipefail

if [ "$#" -ne 1 ]; then
    echo 'usage: check-shapes.sh <shapes-root>' >&2
    exit 2
fi
root="$1"

# すべて外部 HTTP を行わないコマンドのみ。
smoke() {
    local dir="$1"
    (
        cd "$dir"
        ./llmsearch --version | grep -E "^llmsearch [0-9]"
        local state
        state="$(mktemp -d)"
        trap 'rm -rf "$state"' EXIT
        ./llmsearch --state-dir "$state" engines list | grep duckduckgo
        ./llmsearch --state-dir "$state" config show | grep search.engine
    )
}

for shape in normal no-assets nested; do
    if [ ! -x "$root/$shape/llmsearch" ]; then
        echo "error: $root/$shape/llmsearch is missing or not executable" >&2
        exit 1
    fi
    echo "shape: $shape"
    smoke "$root/$shape"
done
echo "all shapes passed"
