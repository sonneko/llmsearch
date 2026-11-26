# 設計書: エージェント連携（JSON 出力）

本書は `REF:docs/specifications/agent-integration.md` に対応する実装設計を記述する。

---

## 1. 基本方針

状態: 実装完了（REF:docs/plans.md#m2-implement）

### 1.1 JSON Lines

状態: 実装完了（REF:src/ui/commands.rs, REF:src/ui/render.rs, REF:docs/plans.md#m2-implement）

`src/ui/render.rs` の `Renderer::event(&Value)` が `serde_json::to_string` した 1 行を標準出力へ書く。共通枠（`v` / `event` / `session` / `at`）は `Renderer::envelope(event, fields)` が付ける。終端は `src/ui/commands.rs` の `run` が各分岐の末尾で `done()` を呼んで 1 回だけ出し、失敗時は `main.rs` が `Renderer::error(&AppError)` で `error` を 1 回だけ出す（`Emitter` のような状態型は持たない）。

### 1.2 無状態の原則

状態: 実装完了（REF:src/ui/render.rs, REF:src/core/errors.rs, REF:docs/plans.md#m2-implement）

`--session` 省略時の `search` は新規セッションを作成し、`search` イベントの `session` に識別子を含める。プロセス内にグローバルキャッシュを持たない（`tokio` タスクローカルも使わない）。

### 1.3 出力サイズ

状態: 実装完了（REF:src/ui/render.rs, REF:src/core/errors.rs, REF:docs/plans.md#m2-implement）

本文は `context.maxChars` の範囲に収める。`error` イベントは常に `code` / `message` / `hint` / `details` / `warnings[]` を出し、`details` は補足が無い場合 `null` とする（キーの有無で判断させない）。`--verbose` は診断行を標準エラーへ足すだけで、JSON の形を変えない。

### 1.4 クォータの可視性

状態: 実装完了（REF:src/ui/commands.rs, REF:src/ui/render.rs）

検索・次ページ・本文に `quota: {metrics: [...]}` を付与する。`outline` / `read` では取得後に `document` イベントを先行出力し、URL・タイトル・警告・クォータを提示する。検索系イベントにはユースケースが返した警告を `{code, message}` の配列で載せる。

管理系コマンド（`history` / `engines` / `network` / `filters` / `config` / `quota`）も `--json` では同じ封筒でイベントを出し、最後に `done` を出す。生の設定 JSON をそのまま出していた箇所（`network show` など）も `--json` では封筒に入れる。生の JSON が必要な場合は `--raw` / `--json` なしの出力を使う。

---

## 2. 共通構造

状態: 実装完了（REF:src/ui/render.rs, REF:src/core/errors.rs, REF:docs/plans.md#m2-implement）

### 2.1 共通フィールド

状態: 実装完了（REF:src/ui/render.rs, REF:src/ui/commands.rs）

`Renderer` が注入された `Clock` から UTC 秒精度・Z 終端の日時を作る。`done` / `error` にも `session` / `at` を付与する。検索・参照成功時とエラー処理時に現行セッションを Renderer に反映する。セッション作成前は空文字を使う。コマンド間の `--session` 引き継ぎとイベント形式は `REF:tests/m5.rs` で実プロセス検証する。

警告は `Resolved.warnings`（起動時）・各ユースケースの戻り値・`Reporter::warn`（アダプターや `Guard` を含む）から発生する。`main.rs` は App 構築後、起動時警告を `Renderer` に渡す前に環境 secret と定義上の固定値を伏字化対象へ登録する。`Renderer::warn` は操作中警告を同じコマンドスコープで収集する。`Renderer::event` は次のデータイベントに `warnings[]` があれば警告を統合し、無ければ直前に `warnings` イベントを出す。エラー時は `AppError::warnings` と収集済み警告を `error.warnings[]` に統合する。警告はコードとメッセージで重複を除き、`--quiet` は標準エラー表示だけを抑制する。`main.rs` の `W1001` は従来どおり独立イベントで、`--quiet` では抑制する。

`Renderer` はアプリが参照する秘匿値を保持し、JSON イベント全体・警告・標準エラーのエラー詳細を出力境界で伏字化する。URL と定義の表示経路固有の整形は引き続き各ハンドラと `core::secret` / `core::url` で行う。

### 2.2 イベント種別

状態: 実装完了（REF:src/ui/render.rs, REF:src/ui/commands.rs）

イベントは `Renderer::envelope(event, fields)` が共通枠（`v` / `event` / `session` / `at`）を付け、`Renderer::event()` が 1 行に出力する。種別ごとのフィールドは `src/ui/commands.rs` の各ハンドラが `json!` で組み立てる。共通枠を 1 か所で足すため、ハンドラ側で `v` / `session` / `at` を書き漏らす事故が起きない（`session` は呼び出し側で明示した値があればそれを使う）。種別名は `search` / `next` / `refine` / `document` / `outline` / `content` / `history` / `engines` / `engine` / `network` / `filters` / `config` / `quota` / `diagnostics` / `warnings` / `done` / `error`。`warnings` は、対象イベントが `warnings[]` を持たない場合の設定・操作警告、または `main.rs` が出す `--color --json` の `W1001` を運ぶ。各イベントの必須フィールドは `REF:tests/m5.rs` / `REF:tests/m9.rs` / `REF:tests/m15.rs` で検証する。

`--json` の応答性は「生テキストを標準出力へ出さない」ことを不変条件にする。`engines show` / `engines test` / `network show` / `filters show` / `config *` / `quota *` / `history export` は `renderer.json` で分岐し、必ずイベントと `done` を出す（`REF:tests/m9.rs`）。`engines list` の `apiKey` は表示用の日本語文ではなく `not-required` / `configured` / `missing` の識別子とし、参照する環境変数名は `apiKeyEnvs` に分ける（`REF:src/usecases/engines.rs`）。

### 2.3 結果オブジェクト

状態: 実装完了（REF:src/ui/render.rs, REF:src/core/errors.rs, REF:docs/plans.md#m2-implement）

`From<&SearchHit> for AgentResult` を実装し、`snippet` の再切り詰めは行わない。

### 2.4 見出しオブジェクト

状態: 実装完了（REF:src/ui/render.rs, REF:src/core/errors.rs, REF:docs/plans.md#m2-implement）

`From<&Heading> for AgentSection`。`line` は 1 始まり。`hidden` フラグ（深度・フィルタで隠れた見出し）も含める。

### 2.5 フィルタ集計オブジェクト

状態: 実装完了（REF:src/ui/render.rs, REF:src/core/errors.rs, REF:docs/plans.md#m2-implement）

`commands.rs::filtered_json(stats, truncated)` が `kept` / `removed` / `reasons` / `disabled` / `truncatedByMaxPerHost` / `truncatedByMaxResults` / `truncatedByLimit` を組み立てる。`reasons` は `BTreeMap<String, usize>` で順序を固定する。`removed` には規則除外・重複 URL・`maxPerHost` 超過が含まれ、`truncatedByLimit` は `FilterStats` のフィールドではなく今回の表示上限による件数。

### 2.6 クォータオブジェクト

状態: 実装完了（REF:src/ui/render.rs, REF:src/core/errors.rs, REF:docs/plans.md#m2-implement）

`commands.rs::quota_json(app)` が `Guard::metrics()` から `metrics[]` を組み立てる。`MetricUsage` は `name` / `used` / `limit` / `resets_at` を持ち、`remaining()` で `limit - used` を求める。設定された上限のあるメトリクスだけを列挙し、セッション未選択時は `session.*` を省く。JSON は各要素に `remaining` を加える。`resets_at` は日次項目の次のローカル 0 時、セッション・累積・窓は `null`。クォータ超過エラーの detail は日次・窓のリセット時刻も含む。

### 2.7 エラーオブジェクト

状態: 実装完了（REF:src/core/errors.rs, REF:docs/plans.md#m2-implement）

`src/core/errors.rs` に `fn message(code: ErrorCode) -> &'static str` と `fn hint(code: ErrorCode) -> &'static str` を置き（M10 で英語 1 言語に固定）、`src/ui/render.rs` の `error()` が組み立てる。カタログにないコードはコンパイル時に検出するため、`ErrorCode` は `enum` とし `match` を網羅させる。失敗に至る過程で出た警告は `AppError::warnings` に積み、`error` イベントの `warnings[]` と標準エラーの双方に出す（`REF:src/adapters/engine/eval.rs`。抽出の再試行で出た `W4004` が、最終的な `E4004` と一緒に失われないようにするため）。

---

## 3. エージェント向けの推奨手順

状態: 実装完了（REF:src/ui/render.rs, REF:src/core/errors.rs, REF:docs/plans.md#m2-implement）

### 3.1 基本フロー

状態: 実装完了（REF:src/ui/cli.rs, REF:docs/plans.md#m2-implement）

`src/ui/cli.rs` の `long_about` に 3 コマンドの例を含める。例は仕様書の記述と一致させる（差分が出たらテストで落とす）。

### 3.2 一覧を読む際の指針

状態: 実装完了（REF:src/ui/render.rs, REF:src/core/errors.rs, REF:docs/plans.md#m2-implement）

`outline` イベントに `totalChars` と各節の `chars` を含める。検索イベントには `filtered`（集計）のみを含め、本文は含めない。

### 3.3 失敗時の回復

状態: 実装完了（REF:src/ui/render.rs, REF:src/core/errors.rs, REF:docs/plans.md#m2-implement）

`error.hint` はコードごとの固定文言。自動リトライは `network.retry` の範囲でのみ行い、アプリケーション層での再試行はしない。ただし「応答は成功したが抽出規則に 1 件も一致しなかった」場合に限り、`src/adapters/engine/eval.rs` が同じ送信を `retry.maxAttempts`（上限 3）まで繰り返す（一時的な応答の揺れ対策。`REF:src/adapters/engine/eval.rs`）。追加の再試行では警告 `W4004` を出し、最終的に失敗した場合は `E4004` の `warnings[]` に含める。

### 3.4 予算管理

状態: 実装完了（REF:src/ui/render.rs, REF:src/core/errors.rs, REF:docs/plans.md#m2-implement）

`--max-chars` を小さく指定した場合の `truncated: true` と `nextSectionId` を返す。`read` は 1 回の呼び出しで 1 文書内の複数節を返せるが、文書をまたぐ場合は呼び出しを分ける。

---

## 4. 安全性

状態: 実装完了（REF:src/ui/render.rs, REF:src/core/errors.rs, REF:docs/plans.md#m2-implement）

### 4.1 信頼境界

状態: 実装完了（REF:src/ui/render.rs, REF:src/core/errors.rs, REF:docs/plans.md#m2-implement）

`content` イベントは `url` を必ず含み、`text` は外部由来である。`event` フィールドでデータ種別を機械的に判別できるようにする。指示文の解釈禁止は利用側の責務であり、本ツールは出所を明示することで支援する。

M16: JSON の明示的信頼境界（#48）

仕様 §4.1 に従い、`src/ui/commands.rs` の content 生成と共通の `document_event()` に `untrusted: true` を追加する。Renderer の秘匿化は維持する。本文・chars・v は変更せず、テキストの section_block 境界処理も変更しない。追加フィールドを許容する消費側と後方互換とする（未知フィールドを拒否する独自スキーマは更新が必要）。tests/m16.rs で実バイナリから outline/read 両経路、本文の不変性、テキストの境界を確認する。

### 4.2 秘匿情報

状態: 実装完了（REF:src/ui/render.rs, REF:src/ui/commands.rs, REF:src/core/secret.rs, REF:src/core/url.rs）

`main.rs` と `commands::run` は App 構築後に環境 secret とエンジン定義の固定値を `Renderer` の伏字化対象へ登録し、`Renderer::event()` は JSON 全体を出力境界で再帰的に伏字化する。テキストの `out` / `err` / `info` / `warn` と `Renderer::error()` も既知の秘匿値と本文中の URL を伏せる。`Secret` は `Serialize` を実装しないため、保持値を直接シリアライズすることもできない。

定義の表示は経路別に二重化する。`engines show` / `network show` / `filters show` の定義と `--raw` は `core::secret::redact_json()` を通し、明示値・秘匿名の query/header・URL の認証情報を伏せる。`engines show` の要約 URL も `core::url::redact()` を通す。`value_env`（環境変数名）は秘匿値ではないため残す。`engines test --dry-run` は解決済み環境変数と `TmplValue::Explicit` の値を redaction list に加え、URL・ヘッダ名・本文のフィールド名と値を使って伏字化する。取得用 URL はセッション内で再取得可能な実体を保持する。

検索結果・文書 URL の既存経路は `src/ui/commands.rs` の `display_url(url, secrets)` に集約し、`core::url::redact()` が URL ユーザー情報、秘匿 query/path パラメータ、既知の秘匿値を伏せる。`hits_json()` / `hit_line_with()`、見出しヘッダ、`[untrusted]` 行、`content` / `document` の `url` / `finalUrl` はこの関数を使う。履歴は `App::append_history()` が追記前にエンジン名と payload 内の文字列を伏字化し、URL のユーザー情報・秘匿 query/path パラメータも含めて保存する。履歴の JSON・テキスト出力は表示境界でも再度 sanitize する。

送信エラーの `E2001` / `E2002` / `E2004` は `reqwest::Error::without_url()` と `core::secret::redact_text()` を通してから `details` にする（`REF:src/adapters/http.rs`）。本文読み取りエラーも同じ処理にする。robots.txt の取得失敗理由（`E2006`）も既知の秘匿値を伏せる（`REF:src/adapters/robots.rs`）。`Renderer::error()` と JSON イベント全体が出力境界の最終防御となり、個別の `AppError::detail` / 警告に入った既知の秘匿値も出さない。

### 4.3 再現性

状態: 実装完了（REF:src/ui/render.rs, REF:src/core/errors.rs, REF:docs/plans.md#m2-implement）

見出し識別子は本文から決定する。再取得で見出し構成が変化した場合は新しい索引で識別子を振り直し、`warnings` に `W3004` を追加する。
