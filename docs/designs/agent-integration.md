# 設計書: エージェント連携（JSON 出力）

本書は `REF:docs/specifications/agent-integration.md` に対応する実装設計を記述する。

---

## 1. 基本方針

状態: 実装完了（REF:docs/plans.md#m2-implement）

### 1.1 JSON Lines

状態: 実装完了（REF:src/ui/commands.rs, REF:src/ui/render.rs, REF:docs/plans.md#m2-implement）

`src/ui/render.rs` の `emit(reporter, event)` が `serde_json::to_string(&event)` を `reporter.data()` へ渡す。終端保証は `src/ui/commands.rs` の `run_command()` が行い、成功時に `done`、失敗時に `error` を 1 回だけ出力する。二重出力を防ぐため `Emitter { closed: bool }` を持つ。

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

警告の載せ方は 2 経路ある。各ハンドラが `crate::ui::render::warnings_json(&diagnostics)` で `warnings[]` を組み立てる経路（`search` / `next` / `refine` / `document` / `outline`）と、`Renderer::error()` が `AppError::warnings` を `error` イベントへ写す経路である。どちらも空配列を必ず出す（キーの有無で警告の有無を推測させない）。

### 2.2 イベント種別

状態: 実装完了（REF:src/ui/render.rs, REF:src/ui/commands.rs）

イベントは `Renderer::envelope(event, fields)` が共通枠（`v` / `event` / `session` / `at`）を付け、`Renderer::event()` が 1 行に出力する。種別ごとのフィールドは `src/ui/commands.rs` の各ハンドラが `json!` で組み立てる。共通枠を 1 か所で足すため、ハンドラ側で `v` / `session` / `at` を書き漏らす事故が起きない（`session` は呼び出し側で明示した値があればそれを使う）。種別名は `search` / `next` / `refine` / `document` / `outline` / `content` / `history` / `engines` / `engine` / `network` / `filters` / `config` / `quota` / `diagnostics` / `done` / `error`。各イベントの必須フィールドは `REF:tests/m5.rs` と `REF:tests/m9.rs` で検証する。

`--json` の応答性は「生テキストを標準出力へ出さない」ことを不変条件にする。`engines show` / `engines test` / `network show` / `filters show` / `config *` / `quota *` / `history export` は `renderer.json` で分岐し、必ずイベントと `done` を出す（`REF:tests/m9.rs`）。`engines list` の `apiKey` は表示用の日本語文ではなく `not-required` / `configured` / `missing` の識別子とし、参照する環境変数名は `apiKeyEnvs` に分ける（`REF:src/usecases/engines.rs`）。

### 2.3 結果オブジェクト

状態: 実装完了（REF:src/ui/render.rs, REF:src/core/errors.rs, REF:docs/plans.md#m2-implement）

`From<&SearchHit> for AgentResult` を実装し、`snippet` の再切り詰めは行わない。

### 2.4 見出しオブジェクト

状態: 実装完了（REF:src/ui/render.rs, REF:src/core/errors.rs, REF:docs/plans.md#m2-implement）

`From<&Heading> for AgentSection`。`line` は 1 始まり。`hidden` フラグ（深度・フィルタで隠れた見出し）も含める。

### 2.5 フィルタ集計オブジェクト

状態: 実装完了（REF:src/ui/render.rs, REF:src/core/errors.rs, REF:docs/plans.md#m2-implement）

`From<&FilterStats> for AgentFiltered` を実装し、`reasons` は `BTreeMap<String, usize>` をそのまま `serde_json::Map` へ変換する（順序を固定するため）。

### 2.6 クォータオブジェクト

状態: 実装完了（REF:src/ui/render.rs, REF:src/core/errors.rs, REF:docs/plans.md#m2-implement）

`QuotaView { metrics: Vec<MetricView> }`、`MetricView { name, used, limit: Option<u64>, resets_at: Option<String> }`。`limit` が未設定（無制限）の項目も必ず含め、`limit: null` とする（項目の有無で判断させない）。`resets_at` は `2026-11-22T00:00 local` の形（`src/core/config/quota.rs` の `format!` で組み立てる。日次・窓の判定が端末ローカル日付のため、UTC の `Z` 終端にはしない）。

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

### 4.2 秘匿情報

状態: 実装完了（REF:src/ui/render.rs, REF:src/ui/commands.rs, REF:src/core/secret.rs, REF:src/core/url.rs）

JSON 生成の直前に `crate::core::url::redact()`（`REF:src/core/url.rs`）を通す。`SecretString` は `Serialize` を実装しないため、誤って出力経路に載せるとコンパイルエラーになる。

設定値を値として直接書いた場合（`TmplValue::Explicit`）は表示時に漏れるため、定義を JSON で出す経路（`engines show` / `network show` / `filters show`）では `crate::core::secret::redact_json()` を通す。`--raw` の定義ダンプも同じ基準で伏せる。`value_env`（環境変数名）は秘匿値ではないので残す。取得用の URL はセッション内の実体をそのまま保持し、表示・JSON には伏字化した `url` を出す。

検索結果・文書の表示 URL は `src/ui/commands.rs` の `display_url(url, secrets)` を必ず通す。`display_url` は URL として解釈できれば `crate::core::url::redact()`（クエリの秘匿パラメータ名、およびパス中の `名前=値` を伏せる）、できなければ `redact_text()` に落とす。`hits_json()` と `hit_line_with()` の呼び出し側がこの 1 か所に集約されているため、新しい出力経路を足すときも同じ関数を使う。`outline` / `read` の見出しヘッダと `[untrusted]` 行、`content` / `document` の `url` / `finalUrl` も同じ関数を通す。

送信エラーの文字列（`E2001` / `E2002` / `E2004` の `details`）には URL や認証値が混ざるため、`crate::core::secret::redact_text()` を通してから表示する（`REF:src/adapters/http.rs`）。robots.txt の取得失敗理由（`E2006` の `details`）も `reqwest` のエラー文字列を含むため同じ処理を通す（`REF:src/adapters/robots.rs`）。

### 4.3 再現性

状態: 実装完了（REF:src/ui/render.rs, REF:src/core/errors.rs, REF:docs/plans.md#m2-implement）

見出し識別子は本文から決定する。再取得で見出し構成が変化した場合は新しい索引で識別子を振り直し、`warnings` に `W3004` を追加する。
