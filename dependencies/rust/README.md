# Rust 環境整備ガイド

Arena.ai サンドボックスで Rust プロジェクトを開発するための環境構築手順をまとめる。

---

## 0. 背景と課題

Arena サンドボックスには以下の制約がある：

1. **初期状態に `rustc` / `cargo` が入っていない**
2. **外部通信が 6 ホストに限定されている**：
   - `github.com` / `codeload.github.com` / `api.github.com`
   - `registry.npmjs.org`
   - `pypi.org` / `files.pythonhosted.org`
3. **到達できないホスト**：
   - `static.rust-lang.org`（rustup の配布元）
   - `crates.io` / `static.crates.io`（クレートレジストリ）
   - `objects.githubusercontent.com`（GitHub Releases のアセット）

これらの制約により、通常の `rustup` や `cargo build` がそのままでは使えない。

---

## 1. 解決策：ツールチェーンの Git 退避方式

**GitHub Actions 上で Rust ツールチェーンと vendored crates を取得し、孤立ブランチに push して退避する。** サンドボックス側ではそれを `codeload.github.com`（到達可能）経由で取得して展開する。

### 全体の流れ

```
[GitHub Actions]                               [Arena サンドボックス]
   rustup + cargo vendor                           │
         │                                         │
         ▼                                         │
  cache/rust-toolchain ブランチに push              │
  (sysroot.tar.gz + vendor.tar.gz)                 │
         │                                         │
         └─────────────────────────────────────────►
                                                   │
                              bash scripts/bootstrap-rust.sh
                                                   │
                                                   ▼
                              /home/user/build/ に展開
                              env.sh を source して cargo 利用可能に
```

---

## 2. 初期セットアップ（プロジェクト側）

### 2.1 `.github/workflows/toolchain-export.yml` の配置

`dependencies/rust/toolchain-export.yml` を `.github/workflows/` にコピーして配置する。

### 2.2 `bootstrap-rust.sh` の配置

`dependencies/rust/bootstrap-rust.sh` を `scripts/bootstrap-rust.sh` にコピーして配置する。

### 2.3 実行

```bash
# .github/toolchain.env を ENABLED=1 にして push → Actions が走る
# 完了後、サンドボックス側で：
bash scripts/bootstrap-rust.sh

# 以降、cargo を使うコマンドには毎回 env.sh を source する：
. /home/user/build/env.sh && cargo build
```

---

## 3. 各コマンドの実行

`bash` ツール呼び出しは毎回新しいシェルで起動するため、Rust コマンドを実行する際は必ず先頭に `. /home/user/build/env.sh` を付ける：

```bash
. /home/user/build/env.sh && cargo test --locked --all-targets --no-fail-fast
. /home/user/build/env.sh && cargo clippy --locked --all-targets -- -D warnings
. /home/user/build/env.sh && cargo fmt --all --check
```

---

## 4. `Cargo.lock` の管理

- `Cargo.lock` は format version 4（Rust 1.90+）を使用する場合、古い `cargo` では `--locked` が読めない
- `.github/toolchain.env` で指定したバージョンで CI も回すこと
- `Cargo.lock` をリポジトリにコミットし、`--locked` で一貫性を保証する

---

## 5. 追加クレートの導入

新たなクレート（`[dependencies]` / `[dev-dependencies]`）を追加する場合、`static.crates.io` に到達できないため以下のいずれかを行う：

### 5.1 差分追加（少数のクレート）

`dependencies/rust/bootstrap-rust.sh` に Blob 展開処理を追記する。
`dependencies/rust/toolchain-export.yml` に `.crate` 取得ジョブを追加する。

### 5.2 大規模な依存更新

`cache/rust-toolchain` ブランチの `vendor.tar.gz` 全体を再生成する：
1. `.github/toolchain.env` の `ENABLED=1` + `SALT` を +1 して push
2. Actions が `cargo vendor` を実行して新しい vendor を push
3. サンドボックス側で `FORCE=1 bash scripts/bootstrap-rust.sh`

---

## 6. WASM ターゲット対応

Web アプリ（`wasm32-unknown-unknown`）をビルドする場合：

```bash
. /home/user/build/env.sh && cargo check --locked --target wasm32-unknown-unknown
```

`sysroot` に `wasm32-unknown-unknown` の `rust-std` も含まれている必要がある（`toolchain-export.yml` の `rustup target add` で追加）。

---

## 7. トラブルシューティング

### 7.1 `cargo` が見つからない

```bash
bash scripts/bootstrap-rust.sh    # 再展開
. /home/user/build/env.sh          # 毎回 source する
```

### 7.2 `Cargo.lock` version 4 エラー

`.github/toolchain.env` で指定した Rust バージョンが `Cargo.lock` のバージョンと一致していることを確認する。

### 7.3 コンテナ再起動後

```bash
bash scripts/bootstrap-rust.sh
git fetch origin <現在のブランチ名>
git reset --mixed FETCH_HEAD
```

### 7.4 `wasm-bindgen-cli` 等のバイナリツール

Git Blob 方式で取得する（`dependencies/rust/README.md` §1.1 のパターン A を参照）。

---

## 8. ファイル構成

```
dependencies/rust/
├── README.md                  # 本書
├── bootstrap-rust.sh          # サンドボックス用ツールチェーン展開スクリプト
├── toolchain-export.yml       # GitHub Actions: ツールチェーン退避ワークフロー
└── toolchain.env              # 退避ワークフローの有効化フラグ
```
