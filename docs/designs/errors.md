# 設計書: エラーと診断

本書は `REF:docs/specifications/errors.md` に対応する実装設計を記述する。パスは実装予定の配置である。

---

## 1. コード体系

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 1.1 分類

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`ErrorCode` は `src/core/errors/codes.ts` の定数オブジェクトとし、`E1xxx`〜`E5xxx` を列挙する。分類の対応は `exitCodeFor()`（`src/ui/cli/exit-code.ts`）が先頭桁から導出する。文字列リテラルの直接記述を禁止し、定数経由のみ許可することを `tests/architecture/error-codes.test.ts` で検査する。

### 1.2 警告

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`WarningCode` も同様に定数化し、`Severity`（`warn` / `fail`）と組み合わせた `Diagnostic` 型（`src/core/errors/diagnostic.ts`）で扱う。`Diagnostic` は `Reporter` 経由で標準エラーへ出し、JSON 時は対象イベントの `warnings` へ射影する。

---

## 2. エラー一覧

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 2.1 入力エラー（`E1xxx`）

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

発生箇所は「引数解析（`src/ui/cli/options.ts`）」「設定検証（`src/core/config/schema.ts`）」「入力解釈（`src/core/parse/`）」「対象解決（`src/usecases/target/resolve-target.ts`）」に限定する。各所は `Result<T, AppError>` を返し、例外を投げない。

### 2.2 ネットワークエラー（`E2xxx`）

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/adapters/http/errors.ts` が `fetch` の例外とステータスを `AppError` へ写像する。タイムアウトは `AbortController` と `AbortSignal.timeout()` で実装し、`E2002` へ写像する。

### 2.3 変換エラー（`E3xxx`）

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

抽出・変換の失敗は `src/adapters/extract/index.ts` で捕捉し、`E3001`〜`E3004` に写像する。`E3001` は「抽出結果が空」、`E3002` は Content-Type 判定、`E3003` は文字コード、`E3004` は turndown の例外に対応する。

### 2.4 プロバイダエラー（`E4xxx`）

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/adapters/providers/errors.ts` が各プロバイダの失敗を写像する。レート制限（429・Captcha）は `E4003` とし、`details.retryAfterMs` に待機時間を含める。

### 2.5 状態エラー（`E5xxx`）

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/adapters/state/errors.ts` が `fs` の例外（`EACCES`, `ENOSPC`, `ENOENT`）を写像する。`ENOSPC` は `E5002`（容量不足）としてヒントに「キャッシュの削除」を含める。

---

## 3. 警告一覧

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

警告の発行箇所は次のとおりに固定する。

| コード | 発行箇所 |
|---|---|
| `W1001` | `src/ui/render/color.ts` |
| `W1002` | `src/usecases/config/resolve.ts` |
| `W2001` | `src/usecases/net/rate-limiter.ts` |
| `W3001` | `src/adapters/extract/index.ts` |
| `W3002` | `src/core/quality/js-hint.ts` |
| `W3003` | `src/adapters/http/fetcher.ts` |
| `W5001` | `src/adapters/state/session-store.ts` |
| `W5002` | `src/usecases/state/cleanup.ts` |
| `W5003` | `src/core/parse/query.ts` |
| `W3004` | `src/usecases/document/outline.ts` |
| `W5004` | `src/adapters/state/history-jsonl.ts` |

同一コードの警告は 1 実行あたり 1 回に集約する（`src/core/errors/dedupe.ts`）。

---

## 4. メッセージの方針

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 4.1 構成

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

メッセージは `src/core/errors/catalog.ts` の `{ message, hint, detail? }` として定義し、表示側（`src/ui/render/error-text.ts`）が「1 文 + 箇条書き」に整形する。原因が不明な場合はヒントに推測を断定する表現を書かない。

### 4.2 言語

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

カタログは `ja` / `en` の 2 言語を持ち、`ui.locale` で選択する。型で両言語の存在を強制する（`Record<Locale, Catalog>`）。

### 4.3 出力先

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`Reporter.error()` と `Reporter.warn()` は常に標準エラーへ書く。JSON モードでは加えて `error` / `warnings` イベントを標準出力へ出す。

### 4.4 秘匿情報

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

カタログの文言に動的な値を埋め込む場合は `redactUrl()` を通した値のみを使う。`details` にも生の環境変数を含めない。

---

## 5. `doctor` による自己診断

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 5.1 診断項目

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/usecases/doctor/checks/` に 1 項目 1 ファイルで `Check`（`name`, `run(ctx)`）を実装し、`src/usecases/doctor/run.ts` が順に実行する。各チェックは 3 秒以内に完了し、失敗しても後続を継続する。

### 5.2 結果の表示

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/ui/render/doctor-text.ts` が `[ok]` / `[warn]` / `[fail]` を付けて 1 行ずつ出力する。`--json` 時は `{ checks: Diagnostic[] }` を 1 イベントで返す。

### 5.3 ネットワーク診断の制約

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

到達性確認は `HEAD` 要求 1 回（または `GET` で `Range: bytes=0-0`）に限定し、検索クエリの送信は行わない。リトライは行わず、結果をそのまま表示する。
