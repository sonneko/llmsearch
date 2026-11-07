# 設計書: テストと品質基準

本書は `REF:docs/specifications/testing.md` に対応する実装設計を記述する。パスは実装予定の配置である。

---

## 1. 受入基準

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 1.1 対話フローの受入基準

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

対話フローは `ScreenState` の遷移テスト（入力列 → 描画列のスナップショット）で検証する。入力を配列で与え、標準出力・標準エラーへの書き出しを記録する `tests/helpers/run-interactive.ts` を用い、実 `readline` を `PassThrough` に差し替える。各受入基準は 1 テストケース以上に対応させる。

### 1.2 非対話・エージェント連携の受入基準

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`tests/e2e/json-contract.test.ts` が「各行が単体で `JSON.parse` 可能」「`done`/`error` が末尾に 1 回だけ」「終了コードが分類と一致」を検証する。セッション引き継ぎは一時ディレクトリを状態ディレクトリにして別プロセス相当の呼び出しを連続実行して検証する。

### 1.3 変換の受入基準

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`tests/contract/markdown-conversion.test.ts` が、フィクスチャ HTML とその期待 Markdown（`tests/fixtures/markdown/*.html` / `*.expected.md`）を突き合わせる。決定論性は同一入力で 2 回変換し、結果文字列が一致することを検証する。

### 1.4 安全性の受入基準

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

出所ヘッダ・信頼境界マーカーの有無を `tests/contract/untrusted-marker.test.ts` で検証する。API キー混入は、ダミーの `BRAVE_API_KEY` を設定して全コマンドを実行し、出力・履歴・状態ファイルに文字列が含まれないことを検証する。robots.txt はモック応答で検証する。

---

## 2. テスト層

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 2.1 層構成

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

| 層 | ディレクトリ | 実行方法 |
|---|---|---|
| 契約テスト | `tests/contract/` | `vitest run tests/contract` |
| 単体テスト | `tests/unit/` | `vitest run tests/unit` |
| 結合テスト | `tests/integration/` | `vitest run tests/integration` |
| アーキテクチャ検査 | `tests/architecture/` | `vitest run tests/architecture` |
| E2E テスト | `tests/e2e/` | `vitest run tests/e2e`（既定はモック、`LLMSEARCH_E2E_LIVE=1` で実通信） |

### 2.2 外部依存の扱い

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

HTTP は `undici` の `MockAgent` で差し替える。フィクスチャは `scripts/record-fixtures.ts` の出力をレビューして確定し、手編集を禁止する（`REF:docs/developers.md` §7.1）。

### 2.3 決定論性の担保

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

テスト用の `FixedClock`、`SeqIdGenerator`、`FakeEnv` を `tests/helpers/ports.ts` に用意し、全テストでこれらを注入する。`Date.now()` と `Math.random()` の直接利用を `tests/architecture/no-nondeterminism.test.ts` で禁止する。

---

## 3. 品質ゲート

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 3.1 コミット前

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`bash scripts/verify.sh` にドキュメント検査・`tsc --noEmit`・`biome check`・`vitest run` を組み込み、すべて緑であることを必須とする。`scripts/verify.sh` のコメントアウトされたプレースホルダを実際のコマンドに置き換える。

### 3.2 リリース前

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

E2E は `LLMSEARCH_E2E_LIVE=1` で実行し、出力サンプルは `tests/fixtures/output/` の期待ファイルと一致することを確認する。`doctor` は既定プロバイダで全項目 `ok` であることを確認する。

### 3.3 カバレッジの方針

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`tests/contract/` の各テストケースに、対応する仕様書の節番号をコメントで付す。節番号の付与漏れを `tests/architecture/spec-mapping.test.ts` が検出する（仕様書の `###` 見出し数と対応コメント数を照合する）。

---

## 4. 回帰の扱い

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 4.1 期待出力の差分

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

スナップショットの更新は `vitest -u` で行い、差分は PR 上で必ずレビューする。期待値の変更を伴う場合は `REF:docs/history.md` に `SPEC-FIX` または `DECISION` として記録する。

### 4.2 固有名詞・サンプルの固定

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

フィクスチャのドメインは `example.jp`、`example.com` のみを用いる。実在サイトの URL を含めないことを `tests/architecture/fixtures.test.ts` で検査する。
