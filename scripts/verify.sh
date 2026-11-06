#!/usr/bin/env bash
# ドキュメント検査（verify-docs.py）と CI ジョブ（lint / test / build）を手元で回す。
#
# 使い方:
#   bash scripts/verify.sh              # 全ステップ（既定）
#   bash scripts/verify.sh --quick      # lint と test のみ（build を省く）
#   bash scripts/verify.sh --fix        # 自動フォーマットを先に実行する
#
# 終了コード: すべて緑なら 0、1 つでも赤なら 1。
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
BUILD_DIR="${BUILD_DIR:-$HOME/build}"

QUICK=0; FIX=0
for arg in "$@"; do
    case "$arg" in
        --quick)   QUICK=1 ;;
        --fix)     FIX=1 ;;
        -h|--help) sed -n '2,10p' "${BASH_SOURCE[0]}"; exit 0 ;;
        *) printf 'error: 未知の引数 %s（--help を見る）\n' "$arg" >&2; exit 2 ;;
    esac
done

cd "$ROOT"

green() { printf '\033[1;32m%s\033[0m\n' "$*"; }
red()   { printf '\033[1;31m%s\033[0m\n' "$*"; }
head_() { printf '\n\033[1;34m==>\033[0m %s\n' "$*"; }

names=(); statuses=(); times=()
run() {  # run <名前> <コマンド...>
    local name="$1"; shift
    head_ "$name"
    local started ended status
    started=$(date +%s)
    "$@" 2>&1
    status=$?
    ended=$(date +%s)
    if [ "$status" = 0 ]; then
        green "ok   $name（$((ended - started)) 秒）"
    else
        red   "FAIL $name（exit $status）"
    fi
    names+=("$name"); statuses+=("$status"); times+=("$((ended - started))")
}

# --- ツールチェーン ---------------------------------------------------------
if [ -f "$BUILD_DIR/env.sh" ]; then
    # shellcheck disable=SC1091
    . "$BUILD_DIR/env.sh"
fi

if [ "$FIX" = 1 ]; then
    head_ "自動フォーマット"
    # プロジェクト固有のフォーマットコマンドをここに追加する
    echo "（プロジェクト固有のフォーマットコマンドをここに設定する）"
fi

run docs      python3 "$ROOT/docs/script/verify-docs.py"

# --- プロジェクト固有の lint / test / build をここに追加する ---
# run lint    <lint コマンド>
# run test    <test コマンド>
# if [ "$QUICK" = 0 ]; then
#     run build <build コマンド>
# fi

head_ "まとめ"
exit_code=0
for i in "${!names[@]}"; do
    if [ "${statuses[$i]}" = 0 ]; then
        printf '    \033[1;32mok  \033[0m %-10s %s 秒\n' "${names[$i]}" "${times[$i]}"
    else
        printf '    \033[1;31mFAIL\033[0m %-10s %s 秒\n' "${names[$i]}" "${times[$i]}"
        exit_code=1
    fi
done
if [ "$exit_code" = 0 ]; then
    green "すべて緑。コミットしてよい。"
else
    red "失敗があるので push しないこと。"
fi
exit "$exit_code"
