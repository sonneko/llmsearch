# 設計書: テストと品質基準

本書は `REF:docs/specifications/testing.md` に対応する実装設計を記述する。

---

## 1. 受入基準

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 1.1 対話フローの受入基準

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

状態機械の遷移テスト（入力列 → 描画列）で検証する。`tests/support/interactive.rs` の `run_script(inputs: &[&str]) -> (String, String)` が `Reporter` を行バッファに差し替え、`stdin` を `Cursor` で与える。各受入基準は 1 テストケース以上に対応させる。

### 1.2 非対話・エージェント連携の受入基準

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`tests/contract/json_contract.rs` が「各行が単体で `serde_json::from_str` 可能」「`done` / `error` が末尾に 1 回だけ」「終了コードが分類と一致」を検証する。セッション引き継ぎは `tempfile::TempDir` を状態ディレクトリにして関数呼び出しを連続実行して検証する。

### 1.3 変換の受入基準

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`tests/contract/markdown_conversion.rs` が `tests/fixtures/markdown/*.html` と `*.expected.md` を突き合わせる。決定論性は同一入力を 2 回変換して文字列一致を検証する。

### 1.4 設定駆動の受入基準

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`tests/contract/config_driven.rs` が、`tempfile::TempDir` に定義 JSON を書き出してからユースケースを実行する。エンジン追加は「`engines/extra.json` を置くと `engines list` と `search` の選択肢に現れる」ことを検証する。クォータは偽の `CounterStore` を注入して境界値（上限ちょうど・上限 +1）を検証する。キャッシュは偽 `HttpClient` の呼び出し回数で検証する。

### 1.5 安全性の受入基準

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

出所ヘッダ・マーカーの有無を `tests/contract/untrusted_marker.rs` で検証する。API キー混入は、ダミーの環境変数を設定して全コマンドを実行し、出力・履歴・状態ファイルに値が含まれないことを検証する。robots.txt はモック応答で検証する。`engines show` は `SecretString` が `Serialize` を実装しないため、コンパイル時に混入経路が存在しないことを `tests/architecture.rs` で確認する。

---

## 2. テスト層

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 2.1 層構成

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

| 層 | ディレクトリ | 実行方法 |
|---|---|---|
| 単体テスト | `src/**` の `#[cfg(test)] mod tests` と `tests/unit/` | `cargo test --lib` |
| 契約テスト | `tests/contract/` | `cargo test --test contract_*` |
| 結合テスト | `tests/integration/` | `cargo test --test integration_*` |
| アーキテクチャ検査 | `tests/architecture.rs` | `cargo test --test architecture` |
| E2E テスト | `tests/e2e.rs` | `LLMSEARCH_E2E_LIVE=1 cargo test --test e2e -- --ignored` |

### 2.2 外部依存の扱い

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

HTTP は `ports::HttpClient` の偽実装（`tests/support/fake_http.rs`）で差し替える。`reqwest` の `Mock` 機能は使わず、ポート境界で差し替える（アダプタの実装詳細にテストが依存しないようにするため）。フィクスチャは `scripts/record_fixtures.sh` の出力をレビューして確定し、手編集を禁止する。

### 2.3 決定論性の担保

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`tests/support/ports.rs` に `FixedClock`, `SeqIdGen`, `FakeEnv` を用意し、全テストで注入する。`SystemTime::now()` / `rand::` / `std::env::var` の直接利用を `tests/architecture.rs` で禁止する（`src/**` を走査し、`adapters/` 配下のみ許可）。

---

## 3. 品質ゲート

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 3.1 コミット前

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`scripts/verify.sh` に「ドキュメント検査（`verify-docs.py`）」「`cargo fmt --all --check`」「`cargo clippy --locked --all-targets -- -D warnings`」「`cargo test --locked --all-targets`」を組み込む。サンドボックスでは `. /home/user/build/env.sh` を先頭に付ける。

### 3.2 リリース前

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`LLMSEARCH_E2E_LIVE=1` で E2E を実行し、`tests/fixtures/output/` の期待ファイルと一致することを確認する。`doctor` が既定エンジンで全項目 `ok` であることを確認する。

### 3.3 カバレッジの方針

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

数値目標は設けない。代わりに、`tests/contract/` の各テストに仕様書の節番号をコメント（`// spec: cli-interface §2.1`）で付し、`tests/architecture.rs` が仕様書の `###` 見出し数と対応コメント数を照合する。

---

## 4. 回帰の扱い

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 4.1 期待出力の差分

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

スナップショット更新は `UPDATE_EXPECT=1` 環境変数で明示的に行い、通常実行では差分を失敗として扱う。期待値の変更を伴う場合は `REF:docs/history.md` に `SPEC-FIX` または `DECISION` として記録する。

### 4.2 固有名詞・サンプルの固定

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

フィクスチャのドメインは `example.jp` / `example.com` のみを用いる。実在サイトの URL を含めないことを `tests/architecture.rs` の禁止パターン検査で確認する。
