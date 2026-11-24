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

発生箇所は「引数解析（`ui/cli/args.rs`）」「設定検証（`core/config/*::validate`）」「エンジン定義検証（`usecases/engines/validate`）」「入力解釈（`core/parse/`）」「対象解決（`REF:src/ui/commands.rs` の `resolve_target`）」に限定する。各所は `Result<_, AppError>` を返し、`panic!` / `unwrap` を使わない（`tests/architecture.rs` で `unwrap_used` 系の clippy lint を有効化）。

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
| `W1001` | `REF:src/main.rs`（色付けを要求されたが端末でない） |
| `W1002` | `REF:src/usecases/config.rs`, `REF:src/adapters/engine/loader.rs` |
| `W1003` | `REF:src/usecases/search.rs`（検索語の切り詰め） |
| `W1004` | `REF:src/adapters/http.rs`（地の文の認証情報） |
| `W1005` | `REF:src/adapters/http.rs`（TLS 検証の無効化） |
| `W2001` | `REF:src/adapters/http.rs`（レート制限の待機） |
| `W3001` | `REF:src/core/convert.rs`（本文推定の失敗） |
| `W3002` | `REF:src/core/convert.rs`（本文が 200 文字未満） |
| `W3003` | `REF:src/adapters/extract/mod.rs`（文字コードの推定・応答サイズの打ち切り）, `REF:src/adapters/engine/eval.rs` |
| `W3004` | `REF:src/usecases/document.rs`（再取得で見出し構成が変化） |
| `W4001` | `REF:src/usecases/search.rs`（`refine` の検索語変更）, `REF:src/core/extract.rs`（抽出に失敗した項目の破棄） |
| `W4002` | `REF:src/adapters/engine/loader.rs` |
| `W4003` | `REF:src/usecases/search.rs`（フォールバック） |
| `W4004` | `REF:src/adapters/engine/eval.rs`, `REF:src/core/extract.rs`, `REF:src/usecases/search.rs` |
| `W4005` | `REF:src/usecases/search.rs`（マージ時の重複 URL 除去） |
| `W4006` | `REF:src/usecases/search.rs`（リダイレクト URL の展開） |
| `W5001` | `REF:src/ui/commands.rs`（クォータ解除の履歴記録失敗） |
| `W5002` | `REF:src/usecases/document.rs`（容量超過によるキャッシュ削除） |
| `W5005` | `REF:src/usecases/guard.rs` |

`W5004`（履歴の不正行スキップ）はコードとして定義するが、発行はしない。読み込み時に不正行は黙って読み飛ばす（`REF:docs/specifications/history.md` §4.4）。

---

## 4. メッセージの方針

状態: 実装完了（REF:src/core/errors.rs）

### 4.1 構成

状態: 実装完了（REF:src/core/errors.rs）

`src/core/errors.rs` に「コード → (message, hint)」の表を英語 1 言語で持つ。表は `match` で網羅し、`ErrorCode` の追加時にコンパイルエラーで漏れを検出する。

### 4.2 言語

状態: 実装完了（REF:src/core/errors.rs, REF:src/ui/render.rs）

メッセージは英語で固定する。`Locale` の列挙、`message(code, locale)` / `hint(code, locale)` の `locale` 引数、`Settings.ui.locale`、`App::locale()`、`Renderer::set_locale()`、`peek_locale()` は削除した。

- `pub fn message(code: ErrorCode) -> &'static str` / `pub fn hint(code: ErrorCode) -> &'static str` は `&'static str` を返し、実行時に文字列を組み立てない（動的な値は `details` へ）。
- `AppError` の `Display` はカタログの英語文言を返す。`Renderer::error()`、`doctor`、検証結果のいずれも同じ文言を使うため、表示経路ごとの差異が無くなる（`ui.locale` を先読みする必要も無いので `main.rs` の `peek_locale` 呼び出しを削除する）。
- 個別 `hint`（`AppError::hint`）は生成側が英語で用意し、`Renderer::error()` がカタログの定型より優先する。
- `src/ui/`・`src/usecases/` のユーザー可視文字列リテラル（clap の `about` / doc コメント由来の help、状態表示、対話メニュー、警告、`doctor`、進捗）も英語にする。コメント・doc コメント（`//` / `///` / `//!`）は対象外。
- 同梱アセット（`assets/engines/*.json` 等）の `description` は `engines list` / `show` に表示されるため英語にする。

`tests/architecture.rs` に、`src/` の非コメント行に含まれる日本語文字（`U+3000`〜`U+30FF`、`U+4E00`〜`U+9FFF`、全角記号）を検出して失敗させる検査を追加し、回帰を防ぐ。検出対象外は行コメントと doc コメントのみとし、raw 文字列も対象に含める。

### 4.3 出力先

状態: 実装完了（REF:src/core/errors.rs）

`Reporter::error` / `warn` は標準エラーへ書く。JSON モードでは加えて `error` イベントを標準出力へ出し、`warnings` は対象イベントへ付す。

### 4.4 秘匿情報

状態: 実装完了（REF:src/core/errors.rs）

`details` に動的な値を入れる場合は `redact::url_with_secrets` を通した値のみを使う。`SecretString` を `details` に変換する API を用意しない。

### 4.5 廃止した `ui.locale`

状態: 実装完了（REF:src/core/config/settings.rs, REF:src/usecases/config.rs）

`UiSettings` から `locale` フィールドを削除すると、旧ファイルの `ui.locale` は `Settings.extra` ではなく `ui` オブジェクトの未知キーになる。`UiSettings` に `#[serde(flatten)] extra: serde_json::Map<String, serde_json::Value>` を足して温存し、`Settings::validate()` がキー `locale` を見つけたら `W1002` を出す。文言は廃止の告知（表示は常に英語、検索言語は `search.language`）を含める。`LLMSEARCH_UI_LOCALE` は `ENV_MAPPINGS` から削除し、`PASSTHROUGH_ENVS` にも載せない（設定されていても参照しない）。

---

## 5. `doctor` による自己診断

状態: 実装完了（REF:src/core/errors.rs）

### 5.1 診断項目

状態: 実装完了（REF:src/core/errors.rs）

`src/usecases/doctor.rs` に 1 項目 1 関数で実装し、`doctor()` が順に実行する。各チェックは 3 秒でタイムアウトし、失敗しても後続を継続する。

### 5.2 結果の表示

状態: 実装完了（REF:src/ui/render.rs）

`src/ui/render.rs` が `[ok]/[warn]/[fail]` を付けて 1 行ずつ出力する。`--json` 時は `diagnostics` イベントで配列を返す。

### 5.3 ネットワーク診断の制約

状態: 実装完了（REF:src/core/errors.rs）

到達性確認は `HEAD` 要求 1 回（405 の場合は `GET` に `Range: bytes=0-0`）に限定し、検索クエリを送らない。リトライは行わず、クォータを消費しない（`Guard` を通さずに `HttpClient` を直接使う専用経路を 1 つだけ用意する）。
