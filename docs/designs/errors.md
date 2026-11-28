# 設計書: エラーと診断

本書は `REF:docs/specifications/errors.md` に対応する実装設計を記述する。

---

## 1. コード体系

状態: 実装完了（REF:src/core/errors.rs）

### 1.1 分類

状態: 実装完了（REF:src/core/errors.rs）

`src/core/errors.rs` に `ErrorCode`（`E1001`〜`E5005`）、`ErrorKind`、`Diagnostic`、`AppError` を置く。`ErrorCode::kind()` が種別を返し、`ErrorKind::exit_code()` が終了コードを決める。`AppError` は `code` / 任意の `detail` / `hint` / 警告列などを保持する通常の構造体で、`Display` はコードカタログの文言を使う。

### 1.2 警告

状態: 実装完了（REF:src/core/errors.rs, REF:src/ui/render.rs）

`enum WarningCode`（`W1001`〜`W5005`）と `struct Diagnostic { pub code: WarningCode, pub message: String }`。重複抑止は `usecases::CollectingReporter` および `Renderer` のコマンドスコープ警告収集が `(code, message)` 単位で行う。

---

## 2. エラー一覧

状態: 実装完了（REF:src/core/errors.rs）

### 2.1 入力エラー（`E1xxx`）

状態: 実装完了（REF:src/core/errors.rs）

`AppError` の既定文面・hint は `src/core/errors.rs` に置き、原因が分かるコマンド経路では生成側が個別 hint を設定する。`commands::resource_not_found` は E1003 detail に要求された network profile / filter set ID を示し、hint に利用可能な ID と対応する list command を添える。`quota reset --scope` の E1004 は許可される各 scope を `--scope` 付きで列挙する。`parse_sections` は `read --section` の E1005 を `S0` / `S3` / `D1:S3` の例で案内し、存在しない節の E1006 は `outline` を案内する。

### 2.2 ネットワークエラー（`E2xxx`）

状態: 実装完了（REF:src/adapters/http.rs, REF:src/core/errors.rs）

`src/adapters/http.rs` の `map_reqwest_error` が `is_timeout` / `is_connect` / `is_redirect` と TLS エラーチェーン文字列を見て `E2001` / `E2002` / `E2004` / `E2008` へ写像する。`E2007` は応答サイズ上限を打ち切り＋`W3003` で扱う方針のため定義のみで未発行。

### 2.3 変換エラー（`E3xxx`）

状態: 実装完了（REF:src/adapters/extract/mod.rs, REF:src/core/convert.rs）

`src/adapters/extract/mod.rs` が Content-Type 判定で `E3002`、`src/core/convert.rs` が Markdown 変換失敗で `E3004` を返す。`E3001` と `E3003` は現状カタログ定義のみで発行箇所が無い（Issue #56）。

### 2.4 エンジンエラー（`E4xxx`）

状態: 実装完了（REF:src/adapters/engine/eval.rs, REF:src/usecases/search.rs）

`src/adapters/engine/eval.rs` と `src/usecases/search.rs` が抽出段階・選択段階の失敗を写像する。`E4001` は「抽出・終端で 0 件」、`E4002` はエンジン未発見／API キー未設定、`E4003` は `401` / `403` / `429` の拒否（仕様 network.md §8.3）、`E4004` は抽出ルール不一致（全件破棄）。`E4005`（重複 ID は `W4002` でスキップ）と `E4006`（`absolutize` 未指定の相対 URL は自動解決せず破棄）はハードエラーとして現状未発行（Issue #56 / #58）。

### 2.5 状態エラー（`E5xxx`）

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/usecases/guard.rs）

状態ストアの読み書きは `src/adapters/state/` から `AppError` を返す（`E5003` は未発行。Issue #56）。`E5005` は `src/usecases/guard.rs` の `ExceedInfo::to_error()` が要求量、現在量／上限、要求後の見込み量、該当すればリセット時刻を detail に含めて生成する。

---

## 3. 警告一覧

状態: 実装完了（REF:src/core/errors.rs）

警告の発行箇所を固定する。

| コード | 発行箇所 |
|---|---|
| `W1001` | `REF:src/main.rs`（`--color --json` 併用時に ANSI を無効化。JSON `warnings` イベントで通知） |
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

`Renderer::error` は標準エラーへ書き、`--quiet` でもエラー表示は維持する。`Renderer::warn` は `--quiet` でない場合だけ標準エラーへ書く。JSON モードでは加えて `error` イベントを出し、操作内の警告配列は quiet に関わらず対象イベントへ付す。`--color --json` の `W1001` は `main.rs` が標準エラーと独立した `warnings` JSON イベントの両方へ出すが、`--quiet` ではイベントも含めて抑制する。

### 4.4 秘匿情報

状態: 実装完了（REF:src/core/errors.rs）

`details` に動的な値を入れる場合は `core::url::redact(url, secrets)` または `core::secret::redact_text(text, secrets)` を通した値のみを使う。`Secret` の保持する値を `details` へそのまま渡す API を用意しない。さらに `Renderer::error()` は既知の秘匿値を JSON と標準エラーの双方で伏字化し、個別 `AppError::hint` をカタログ hint より優先する。

### 4.5 廃止した `ui.locale`

状態: 実装完了（REF:src/core/config/settings.rs, REF:src/usecases/config.rs）

`UiSettings` から `locale` フィールドを削除すると、旧ファイルの `ui.locale` は `Settings.extra` ではなく `ui` オブジェクトの未知キーになる。`UiSettings` に `#[serde(flatten)] extra: serde_json::Map<String, serde_json::Value>` を足して温存し、`Settings::validate()` がキー `locale` を見つけたら `W1002` を出す。文言は廃止の告知（表示は常に英語、検索言語は `search.language`）を含める。`LLMSEARCH_UI_LOCALE` は `ENV_MAPPINGS` から削除し、`PASSTHROUGH_ENVS` にも載せない（設定されていても参照しない）。

---

## 5. `doctor` による自己診断

状態: 実装完了（REF:src/usecases/doctor.rs）

### 5.1 診断項目

状態: 実装完了（REF:src/usecases/doctor.rs, REF:build.rs）

`src/usecases/doctor.rs` に項目別チェックを実装し、`run()` が順に実行して失敗しても後続を継続する（`build.rs` 由来の `RUSTC_VERSION` 先頭 `"rustc "` と整形文字列の重複は Issue #56 で追跡）。

### 5.2 結果の表示

状態: 実装完了（REF:src/ui/render.rs）

`src/ui/render.rs` が `[ok]/[warn]/[fail]` を付けて 1 行ずつ出力する。`--json` 時は `diagnostics` イベントで配列を返す。

### 5.3 ネットワーク診断の制約

状態: 実装完了（REF:src/usecases/doctor.rs）

クォータを消費しないよう `Guard` を通さずに `HttpClient` を直接呼ぶ（現状 `HttpRequest::get` を通常の `app.network` 設定で送っており、仕様の `HEAD`/`Range`・3 秒固定タイムアウト・再試行なしとの差分は Issue #56 で追跡）。`--no-network` 指定時は到達性チェックをスキップする。
