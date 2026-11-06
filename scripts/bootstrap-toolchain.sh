#!/usr/bin/env bash
# ツールチェーンの展開・復旧スクリプト。
#
# 背景:
#   Arena の開発サンドボックスには初期状態で目的のツールチェーンが入っていない場合がある。
#   また、外部通信が制限されているため、直接レジストリ等から取得できない。
#   このスクリプトは GitHub Actions で事前に作成したツールチェーンの退避データを
#   展開して開発環境を復旧する。
#
# 使い方:
#   bash scripts/bootstrap-toolchain.sh              # 無ければ取得、あれば何もしない
#   FORCE=1 bash scripts/bootstrap-toolchain.sh      # 展開済みでも取り直す
#
# 環境変数:
#   BUILD_DIR  展開先。既定は $HOME/build。スナップショット対象外。
#   BRANCH     取り出し元のブランチ。既定 cache/toolchain。
#   REPO       owner/name。既定は origin のリモート URL から推定。
#   FORCE      1 で展開済みでも取り直す。
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
BUILD_DIR="${BUILD_DIR:-$HOME/build}"
BRANCH="${BRANCH:-cache/toolchain}"

say() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
die() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

# --- 前提 -------------------------------------------------------------------
for tool in curl tar gh python3; do
    command -v "$tool" >/dev/null 2>&1 || die "$tool が見つからない。"
done

# 既に展開済みのチェック（プロジェクトに合わせて調整）
if [ -z "${FORCE:-}" ] && [ -f "$BUILD_DIR/env.sh" ]; then
    say "すでに展開済み: $BUILD_DIR"
    echo "    使うときは次を各コマンドの前に置く: . $BUILD_DIR/env.sh"
    exit 0
fi

TOKEN="$(gh auth token 2>/dev/null || true)"
[ -n "$TOKEN" ] || die "gh の認証が無い。GitHub 接続を確認すること。"

if [ -z "${REPO:-}" ]; then
    url="$(git -C "$ROOT" remote get-url origin 2>/dev/null || true)"
    REPO="$(printf '%s' "$url" | sed -E 's#.*github\.com[:/]##; s#\.git$##')"
fi
case "$REPO" in */*) ;; *) die "REPO を推定できなかった（origin=$url）。REPO=owner/name を指定すること。";; esac

mkdir -p "$BUILD_DIR"
say "展開先: $BUILD_DIR（スナップショット対象外）"

# --- プロジェクト固有のツールチェーン展開処理をここに記述 ---
# 例（Rust）:
#   codeload.github.com から cache/rust-toolchain ブランチの tarball を取得し、
#   sysroot と vendor を展開する。
#
# 例（Node.js）:
#   cache/node-toolchain ブランチから node_modules の退避を展開する。
#
# このテンプレートではダミーの env.sh を生成する:
cat > "$BUILD_DIR/env.sh" <<ENVS
# ツールチェーン環境変数（scripts/bootstrap-toolchain.sh が生成）
# bash の呼び出しごとに環境は引き継がれないので、毎回これを読むこと:
#   . $BUILD_DIR/env.sh && <コマンド>
export PATH=$BUILD_DIR/bin:\$PATH
ENVS

say "復旧完了。各コマンドの前に次を置くこと:"
echo "    . $BUILD_DIR/env.sh"
