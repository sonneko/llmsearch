# 設計書: エラーと診断

本書は `REF:docs/specifications/errors.md` に対応する実装設計を記述する。

---

## 1. コード体系

状態: 実装完了（REF:src/core/errors.rs）

### 1.1 分類

状態: 実装完了（REF:src/core/errors.rs）

`src/core/errors.rs` に `enum ErrorCode`（`E1001`〜`E5005` を網羅）と `enum Severity` を置く。分類は `ErrorCode::kind() -> ErrorKind` が返し、`src/core/errors.rs` が `ErrorKind` → 終了コードへ写像する。`AppError` は `thiserror` の `#[derive(Error)]` で定義し、`code()` を実装する。

### 1.2 警告

状態: 実装完了（REF:src/core/errors.rs）

`enum WarningCode`（`W1001`〜`W5005`）と `struct Diagnostic { code: WarningCode, message: String, detail: Option<String> }`。`Reporter::warn` は重複を除いて 1 回だけ表示する（`core/errors/dedupe.rs` の `HashSet<WarningCode>` を使用）。

---

## 2. エラー一覧

状態: 実装完了（REF:src/core/errors.rs）

### 2.1 入力エラー（`E1xxx`）

状態: 実装完了（REF:src/core/errors.rs）

発生箇所は「引数解析（`ui/cli/args.rs`）」「設定検証（`core/config/*::validate`）」「エンジン定義検証（`usecases/engines/validate`）」「入力解釈（`core/parse/`）」「対象解決（`usecases/target.rs`）」に限定する。各所は `Result<_, AppError>` を返し、`panic!` / `unwrap` を使わない（`tests/architecture.rs` で `unwrap_used` 系の clippy lint を有効化）。

### 2.2 ネットワークエラー（`E2xxx`）

状態: 実装完了（REF:src/core/errors.rs）

`adapters::http::errors::map_reqwest(e: reqwest::Error) -> AppError` が `is_timeout` / `is_connect` / `is_redirect` を見て写像する。TLS 失敗は `reqwest` の `is_request` + エラーチェーン文字列で判定し、`E2008` へ写像する。

### 2.3 変換エラー（`E3xxx`）

状態: 実装完了（REF:src/core/errors.rs）

`adapters::extract::errors` が `E3001`〜`E3004` を生成する。`E3001` は「抽出結果が空」、`E3002` は Content-Type 判定、`E3003` は文字コード、`E3004` は Markdown 化の例外に対応する。

### 2.4 エンジンエラー（`E4xxx`）

状態: 実装完了（REF:src/core/errors.rs）

`adapters::engine::errors` が抽出段階の失敗を写像する。`E4001` は「抽出後に 0 件」、`E4004` は「抽出ルール不一致（破棄件数が全件）」。`E4005` は読み込み時の重複検出、`E4006` は相対 URL の破棄が全件だった場合。

### 2.5 状態エラー（`E5xxx`）

状態: 実装完了（REF:src/core/errors.rs）

`adapters::state::errors::map_io(io::Error, path) -> AppError` が `PermissionDenied` / `StorageFull` / `NotFound` を写像する。`E5005` は `usecases::guard::quota` が生成する。

---

## 3. 警告一覧

状態: 実装完了（REF:src/core/errors.rs）

警告の発行箇所を固定する。

| コード | 発行箇所 |
|---|---|
| `W1001` | `ui/render/color.rs` |
| `W1002` | `usecases/config/resolve.rs`, `adapters/engine/loader.rs` |
| `W1003` | `core/text/query.rs` |
| `W1004` | `core/config/network.rs` |
| `W1005` | `adapters/http/client.rs` |
| `W2001` | `usecases/guard/rate_limiter.rs` |
| `W3001` | `adapters/extract/density.rs` |
| `W3002` | `core/quality/js_hint.rs` |
| `W3003` | `adapters/http/fetcher.rs` |
| `W3004` | `usecases/document/outline.rs` |
| `W4001` | `adapters/engine/eval.rs` |
| `W4002` | `adapters/engine/loader.rs` |
| `W4003` | `usecases/search/fallback.rs` |
| `W4004` | `adapters/engine/eval.rs` |
| `W4005` | `core/search/merge.rs` |
| `W5001` | `adapters/state/session_store.rs`, `counters_store.rs` |
| `W5002` | `usecases/state/cleanup.rs` |
| `W5004` | `adapters/state/history_jsonl.rs` |
| `W5005` | `usecases/guard/quota.rs` |

---

## 4. メッセージの方針

状態: 実装完了（REF:src/core/errors.rs）

### 4.1 構成

状態: 実装完了（REF:src/core/errors.rs）

`src/core/errors.rs` に「コード → (message, hint)」の表を `ja` / `en` の 2 言語で持つ。表は `match` で網羅し、`ErrorCode` の追加時にコンパイルエラーで漏れを検出する。

### 4.2 言語

状態: 実装完了（REF:src/core/errors.rs）

`enum Locale { Ja, En }` を `Settings.ui.locale` から決め、`catalog::message(code, locale)` で取得する。`&'static str` を返し、実行時に文字列を組み立てない（動的な値は `details` へ）。

### 4.3 出力先

状態: 実装完了（REF:src/core/errors.rs）

`Reporter::error` / `warn` は標準エラーへ書く。JSON モードでは加えて `error` イベントを標準出力へ出し、`warnings` は対象イベントへ付す。

### 4.4 秘匿情報

状態: 実装完了（REF:src/core/errors.rs）

`details` に動的な値を入れる場合は `redact::url_with_secrets` を通した値のみを使う。`SecretString` を `details` に変換する API を用意しない。

---

## 5. `doctor` による自己診断

状態: 実装完了（REF:src/core/errors.rs）

### 5.1 診断項目

状態: 実装完了（REF:src/core/errors.rs）

`src/usecases/doctor/checks/` に 1 項目 1 ファイルで `trait Check { fn name(&self) -> &'static str; async fn run(&self, ctx: &DoctorCtx<'_>) -> Diagnostic }` を実装し、`run.rs` が順に実行する。各チェックは 3 秒でタイムアウトし、失敗しても後続を継続する。

### 5.2 結果の表示

状態: 実装完了（REF:src/ui/render.rs）

`src/ui/render.rs` が `[ok]/[warn]/[fail]` を付けて 1 行ずつ出力する。`--json` 時は `diagnostics` イベントで配列を返す。

### 5.3 ネットワーク診断の制約

状態: 実装完了（REF:src/core/errors.rs）

到達性確認は `HEAD` 要求 1 回（405 の場合は `GET` に `Range: bytes=0-0`）に限定し、検索クエリを送らない。リトライは行わず、クォータを消費しない（`Guard` を通さずに `HttpClient` を直接使う専用経路を 1 つだけ用意する）。
