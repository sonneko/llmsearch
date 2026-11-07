#!/usr/bin/env bash
# Rust ツールチェーンと vendored crates を復旧する。
#
# 背景:
#   Arena サンドボックスには rustc / cargo が無く、static.rust-lang.org や
#   crates.io にも到達できない。このスクリプトは GitHub Actions で事前に
#   cache/rust-toolchain ブランチに退避したツールチェーンを展開する。
#
# 使い方:
#   bash scripts/bootstrap-rust.sh              # 無ければ取得、あれば何もしない
#   FORCE=1 bash scripts/bootstrap-rust.sh      # 展開済みでも取り直す
#   SMOKE=1 bash scripts/bootstrap-rust.sh      # 展開後に cargo check を回す
#
# 前提:
#   - dependencies/rust/toolchain-export.yml が .github/workflows/ に配置済みであること
#   - 最低 1 回 Actions を実行して cache/rust-toolchain ブランチが作成済みであること
#
# 環境変数:
#   BUILD_DIR  展開先。既定は $HOME/build。**スナップショット対象外**。
#   BRANCH     取り出し元の孤立ブランチ。既定 cache/rust-toolchain。
#   REPO       owner/name。既定は origin のリモート URL から推定。
#   FORCE      1 で展開済みでも取り直す。
#   SMOKE      1 で展開後に cargo check を回す（コールドビルド込みで約 2 分）。
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
BUILD_DIR="${BUILD_DIR:-$HOME/build}"
BRANCH="${BRANCH:-cache/rust-toolchain}"

say() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
die() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

# --- 前提チェック -----------------------------------------------------------
for tool in curl tar gh python3; do
    command -v "$tool" >/dev/null 2>&1 || die "$tool が見つからない。"
done

if [ -z "${FORCE:-}" ] && [ -x "$BUILD_DIR/sysroot/bin/rustc" ]; then
    if PATH="$BUILD_DIR/sysroot/bin:$PATH" rustc --version >/dev/null 2>&1; then
        if [ -f "$BUILD_DIR/env.sh" ] && [ -d "$BUILD_DIR/vendor-src" ]; then
            say "すでにある: $BUILD_DIR（$(PATH="$BUILD_DIR/sysroot/bin:$PATH" rustc --version)）"
            echo "    使うときは次を各コマンドの前に置く: . $BUILD_DIR/env.sh"
            exit 0
        fi
        say "sysroot はあるので env.sh / cargo-home/config.toml だけ補う"
        REGENERATE_ONLY=1
    else
        say "$BUILD_DIR/sysroot/bin/rustc が動かないので取り直す"
    fi
fi

TOKEN="$(gh auth token 2>/dev/null || true)"
[ -n "$TOKEN" ] || die "gh の認証が無い（gh auth token が空）。GitHub 接続を確認すること。"

if [ -z "${REPO:-}" ]; then
    url="$(git -C "$ROOT" remote get-url origin 2>/dev/null || true)"
    REPO="$(printf '%s' "$url" | sed -E 's#.*github\.com[:/]##; s#\.git$##')"
fi
case "$REPO" in */*) ;; *) die "REPO を推定できなかった（origin=$url）。REPO=owner/name を指定すること。";; esac

mkdir -p "$BUILD_DIR/sysroot" "$BUILD_DIR/cargo-home"

if [ "${REGENERATE_ONLY:-0}" != 1 ]; then
    say "取り出し元: https://codeload.github.com/$REPO/tar.gz/refs/heads/$BRANCH"
    say "展開先:     $BUILD_DIR（スナップショット対象外）"
    cd "$BUILD_DIR"

    # codeload 経由で取得（objects.githubusercontent.com は到達不可）
    curl -fsSL -H "Authorization: Bearer $TOKEN" -o tc.tgz \
        "https://codeload.github.com/$REPO/tar.gz/refs/heads/$BRANCH" \
        || die "ダウンロードに失敗した。ブランチ $BRANCH が存在するか確認すること。"
    say "ダウンロード完了: $(du -h tc.tgz | cut -f1)"

    rm -rf ./std-app-dev-template-*
    tar xzf tc.tgz
    work="$(find . -maxdepth 1 -type d -name 'std-app-dev-template-*' | head -1)"
    [ -n "$work" ] || die "tarball の中に展開先ディレクトリが見つからない"
    cd "$work"

    say "sha256 の検証"
    if [ -f SHA256SUMS ]; then
        sha256sum -c SHA256SUMS || die "sha256 が合わない。ブランチを再生成すること。"
    fi

    say "sysroot を展開"
    if [ -f chunks/sysroot.tar.gz.* ]; then
        cat chunks/sysroot.tar.gz.* > sysroot.tar.gz
    elif [ -f sysroot.tar.gz ]; then
        : # 既に単一ファイル
    else
        die "sysroot.tar.gz が見つからない"
    fi
    tar xzf sysroot.tar.gz -C "$BUILD_DIR/sysroot"

    say "vendor を展開"
    if [ -f chunks/vendor.tar.gz.* ]; then
        cat chunks/vendor.tar.gz.* > vendor.tar.gz
    elif [ -f vendor.tar.gz ]; then
        : # 既に単一ファイル
    else
        die "vendor.tar.gz が見つからない"
    fi
    tar xzf vendor.tar.gz -C "$BUILD_DIR"

    # cargo-home/config.toml を生成（vendored sources を使用）
    cat > "$BUILD_DIR/cargo-home/config.toml" <<CFG
[source.crates-io]
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "$BUILD_DIR/vendor-src"

[net]
offline = true
CFG

    # env.sh を生成
    cat > "$BUILD_DIR/env.sh" <<ENVS
# Rust ツールチェーン環境変数（scripts/bootstrap-rust.sh が生成）
# bash の呼び出しごとに環境は引き継がれないので、**毎回**これを読むこと:
#   . $BUILD_DIR/env.sh && cargo test --locked --all-targets
export PATH=$BUILD_DIR/sysroot/bin:\$PATH
export RUSTC=$BUILD_DIR/sysroot/bin/rustc
export CARGO_HOME=$BUILD_DIR/cargo-home
export CARGO_NET_OFFLINE=true
# debuginfo を抑えてディスク使用量を抑える
export CARGO_PROFILE_DEV_DEBUG=line-tables-only
export CARGO_INCREMENTAL=0
ENVS

    # 中間生成物を削除
    rm -rf "$BUILD_DIR/tc.tgz" "$BUILD_DIR/$work"
fi

# --- 追加クレート（Git Blob）の展開 ----------------------------------------
# .crate ファイルを /home/user/build/crates/ に取得して vendor-src に展開する。
# 使い方: install_extra_crate <blob_sha> <expected_sha256> <filename>
install_extra_crate() {
    local blob="$1" sha="$2" file="$3"
    say "追加クレート $file を git blob から展開"
    gh api "repos/$REPO/git/blobs/$blob" --jq .content | base64 -d > "$BUILD_DIR/$file"
    echo "$sha  $BUILD_DIR/$file" | sha256sum -c - >/dev/null || die "$file の sha256 が合わない"
    tar xzf "$BUILD_DIR/$file" -C "$BUILD_DIR"
    # .crate → vendor-src/<name>/ に展開して .cargo-checksum.json を生成
    BUILD_DIR="$BUILD_DIR" python3 - <<'PY'
import glob, hashlib, json, os, shutil, tarfile
build_dir = os.environ["BUILD_DIR"]
vendor_root = os.path.join(build_dir, "vendor-src")
for crate_path in sorted(glob.glob(os.path.join(build_dir, "crates", "*.crate"))):
    base = os.path.basename(crate_path)[:-6]
    # name-version の形式から name を抽出（最後のハイフン以降がバージョン）
    parts = base.rsplit("-", 1)
    if len(parts) != 2:
        continue
    name = parts[0]
    pkg_sha = hashlib.sha256(open(crate_path, "rb").read()).hexdigest()
    dest = os.path.join(vendor_root, name)
    if os.path.exists(dest):
        shutil.rmtree(dest)
    os.makedirs(dest, exist_ok=True)
    with tarfile.open(crate_path, "r:gz") as tf:
        for member in tf.getmembers():
            segs = member.name.split("/", 1)
            if len(segs) == 2 and segs[1]:
                member.name = segs[1]
                tf.extract(member, dest)
    files = {}
    for root, _, fnames in os.walk(dest):
        for fn in fnames:
            if fn == ".cargo-checksum.json":
                continue
            full = os.path.join(root, fn)
            rel = os.path.relpath(full, dest).replace(os.sep, "/")
            files[rel] = hashlib.sha256(open(full, "rb").read()).hexdigest()
    with open(os.path.join(dest, ".cargo-checksum.json"), "w") as f:
        json.dump({"files": files, "package": pkg_sha}, f, separators=(",", ":"))
PY
    rm -rf "$BUILD_DIR/$file" "$BUILD_DIR/crates"
}

# --- 追加クレートの登録（ここに追記していく） --------------------------------
# 例:
#   if [ ! -d "$BUILD_DIR/vendor-src/some-crate" ]; then
#       install_extra_crate \
#           <blob_sha> \
#           <expected_sha256> \
#           some-crates.tar.gz
#   fi

# --- 確認 -------------------------------------------------------------------
say "確認"
# shellcheck disable=SC1091
. "$BUILD_DIR/env.sh"
cd "$ROOT"
rustc --version
cargo --version

if [ "${SMOKE:-0}" = "1" ]; then
    say "SMOKE=1: cargo check を回す"
    cargo check --locked
fi

say "復旧完了。各コマンドの前に次を置くこと:"
echo "    . $BUILD_DIR/env.sh && cd $ROOT && cargo test --locked --all-targets"
