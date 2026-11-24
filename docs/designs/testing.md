# 設計書: テストと品質基準

本書は `REF:docs/specifications/testing.md` に対応する実装設計を記述する。

---

## 1. 受入基準

状態: 実装完了（REF:tests/integration.rs, REF:tests/contract.rs, REF:tests/e2e.rs）

### 1.1 対話フローの受入基準

状態: 実装完了（REF:src/ui/interactive.rs, REF:tests/support/mod.rs）

`src/ui/interactive.rs` の `Interactive::run(input, out)` に `Cursor<&str>` を渡し、`Reporter` を無音実装に差し替えて、入力列と出力文字列の組で検証する。メニュー文言の固定は `REF:src/ui/menu.rs` の単体テスト（`results_menu_numbers_are_fixed`）が担う。キーボード入力と疑似端末を伴う確認は `REF:docs/handoff.md` の手作業記録に残す（自動テストでは端末を要求しない）。

### 1.2 非対話・エージェント連携の受入基準

状態: 実装完了（REF:tests/m5.rs, REF:tests/m9.rs, REF:tests/e2e.rs）

`REF:tests/m5.rs` が実プロセスを起動し、「各行が単体で `serde_json::from_str` 可能」「`done` / `error` が末尾に 1 回だけ」「終了コードが分類と一致」を検証する。セッション引き継ぎは `tempfile::TempDir` を状態ディレクトリにして別プロセスを連続実行して検証する。`--json` の全サブコマンドが `done` を出すことは `REF:tests/m9.rs` が確認する。

### 1.3 変換の受入基準

状態: 実装完了（REF:src/core/convert.rs, REF:tests/contract.rs）

HTML→Markdown の性質（見出し・リスト・表・コードブロック・リンクの保持、相対 URL の絶対化、コードブロック内 `#` の非見出し化）を `REF:tests/contract.rs` が固定の入力で検証する。決定論性は同一入力を 2 回変換して文字列一致を取る。

### 1.4 設定駆動の受入基準

状態: 実装完了（REF:tests/integration.rs, REF:tests/contract.rs, REF:tests/assets.rs）

`tempfile::TempDir` に定義 JSON を書き出してからユースケースを実行する。エンジン追加は「定義ファイルを置くと `engines list` と `search` の選択肢に現れる」ことを検証する。クォータは偽の `CounterStore` を注入して境界値（上限ちょうど・上限 +1）を検証する（`REF:tests/m8.rs`）。キャッシュは `REF:tests/support/mod.rs` の `FakeHttp` の呼び出し回数で検証する。

### 1.5 配布物の受入基準

状態: 実装完了（REF:scripts/check-release.sh, REF:tests/m11.rs）

配布ゲート（`REF:scripts/check-release.sh`）が、ステージしたパッケージに対して「バイナリの隣に `assets/` がある状態」「`assets/` を削除した状態」「`assets/assets/` に入れ子で重複した状態」の 3 通りで `--version` / `engines list` / `config show` を実行する。パッケージ化の再実行で入れ子が出ないことは `REF:scripts/test-check-release.sh` 側で検査する。CI では同じ手順を `REF:.github/workflows/ci.yml` の Build & Distribution Compatibility ジョブが Ubuntu 22.04 / 24.04 のコンテナで実行する。

### 1.6 安全性の受入基準

状態: 実装完了（REF:tests/integration.rs, REF:tests/m9.rs, REF:src/ui/render.rs）

出所ヘッダと `[untrusted]` マーカーの付与、および本文中のマーカー様行の無害化は `REF:src/ui/render.rs` の単体テストが検証する。API キー混入は、`TmplValue::Explicit` と `value_env` の双方を含む定義で `engines show` / `show --raw` / `test` を実行し、出力・履歴・状態ファイルに値が含まれないことを `REF:tests/m9.rs` と単体テストで検証する。robots.txt はモック応答で検証する。

---

## 2. テスト層

状態: 実装完了（REF:tests/)

### 2.1 層構成

状態: 実装完了（REF:tests/）

| 層 | 実体 | 実行方法 |
|---|---|---|
| 単体テスト | `src/**` の `#[cfg(test)] mod tests` | `cargo test --lib` |
| 結合テスト・契約テスト | `REF:tests/integration.rs`, `REF:tests/contract.rs`, `REF:tests/assets.rs` | `cargo test --test integration --test contract --test assets` |
| マイルストーン別の回帰 | `REF:tests/m5.rs`, `REF:tests/m6.rs`, `REF:tests/m7.rs`, `REF:tests/m8.rs`, `REF:tests/m9.rs`, `REF:tests/m10.rs`, `REF:tests/m11.rs` | `cargo test --test m5` など |
| アーキテクチャ検査 | `REF:tests/architecture.rs` | `cargo test --test architecture` |
| ネットワーク・TLS・プロキシ | `REF:tests/proxy.rs`, `REF:tests/tls.rs`（証明書は `REF:tests/fixtures/tls/`） | `cargo test --test proxy --test tls` |
| E2E テスト | `REF:tests/e2e.rs` | 既定では起動と決定論性のみ。実ネットワークは `LLMSEARCH_E2E_LIVE=1` |

### 2.2 外部依存の扱い

状態: 実装完了（REF:tests/support/mod.rs）

HTTP はポート境界で偽実装（`REF:tests/support/mod.rs` の `FakeHttp`。登録した応答を順に返す）に差し替える。`reqwest` の `Mock` 機能は使わない（アダプタの実装詳細にテストが依存しないようにするため）。実 HTTP の応答が必要な検査はローカルの `http.server` スタブ（サンプルは `REF:tests/fixtures/tls/server.py`）で行い、外部ネットワークへは出ない。期待値は手作業で編集せず、テストコードが生成した出力をレビューして確定する。

### 2.3 決定論性の担保

状態: 実装完了（REF:tests/support/mod.rs, REF:tests/architecture.rs）

`REF:tests/support/mod.rs` に `FixedClock`（固定時刻）、`SeqIds`（連番）、`FakeEnv`（環境変数と状態ディレクトリ）、`Harness`（組み立て済みの `App`）を用意し、全テストで注入する。実装側は `ports::Clock` / `ports::Env` を受け取り、`SystemTime::now()` や `std::env::var` を直接呼ばない。同梱アセットを読むテストは `current_dir` を `CARGO_MANIFEST_DIR` に設定する（`Harness` が行う）。

---

## 3. 品質ゲート

状態: 実装完了（REF:scripts/verify.sh）

### 3.1 コミット前

状態: 実装完了（REF:scripts/verify.sh）

`REF:scripts/verify.sh` に「ドキュメント検査（`REF:docs/script/verify-docs.py`）」「`cargo fmt --all --check`」「`cargo clippy --locked --all-targets -- -D warnings`」「`cargo test --locked --all-targets`」を組み込む。サンドボックスでは `. /home/user/build/env.sh` を先頭に付ける。`--quick` はビルド段を省略する。

### 3.2 リリース前

状態: 実装完了（REF:scripts/check-release.sh, REF:.github/workflows/release.yml）

`LLMSEARCH_E2E_LIVE=1` で E2E を実行し、`doctor` が既定エンジンで全項目 `ok` であることを確認する。配布物は `REF:scripts/check-release.sh` の 3 形態で検査し、CI の Build & Distribution Compatibility ジョブと同じ手順を通す。

### 3.3 カバレッジの方針

状態: 実装完了（REF:tests/architecture.rs）

行カバレッジの数値目標は設けない。代わりに、仕様書の `###` 節とテストの対応を、仕様書の節名を引用するテスト名と `REF:docs/designs/` の各節で保つ。

---

## 4. 回帰の扱い

状態: 実装完了（REF:tests/m5.rs）

### 4.1 期待出力の差分

状態: 実装完了（REF:tests/e2e.rs）

出力の期待値に差分が生じた場合は退行として扱う。仕様変更に伴う差分である場合は、仕様書の改訂承認を得たうえで期待値を更新する。決定論性の検査（同一入力 2 回の一致）を `REF:tests/e2e.rs` と単体テストに置く。

### 4.2 固有名詞・サンプルの固定

状態: 実装完了（REF:tests/)

出力サンプルに用いる URL・タイトルは実在のページに依存させず、`example.com` / `example.jp` と `REF:tests/support/mod.rs` の固定データを用いる。実在サイトの URL をテストに含めない。
