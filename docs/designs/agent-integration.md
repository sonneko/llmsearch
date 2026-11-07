# 設計書: エージェント連携（JSON 出力）

本書は `REF:docs/specifications/agent-integration.md` に対応する実装設計を記述する。パスは実装予定の配置である。

---

## 1. 基本方針

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 1.1 JSON Lines

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/ui/render/json-events.ts` の `emitEvent(reporter, event)` が `JSON.stringify` して `reporter.data()` へ渡す。イベントの終端保証は `src/ui/cli/commands/runner.ts` の `runCommand()` が担い、成功時 `done`、失敗時 `error` を必ず 1 回だけ出力する。二重出力を防ぐため、`emit` はコマンド実行コンテキストで `closed` フラグを持つ。

### 1.2 無状態の原則

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`--session` 省略時の `search` は新規セッションを作成し、`search` イベントの `session` に識別子を含める。エージェントはこの値のみを次のコマンドへ渡せばよい。プロセス内グローバルに状態を保持しない。

### 1.3 出力サイズ

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

イベントの本文は `context.max_chars` の範囲に収める。メタ情報は必須フィールドのみとし、`details` は `--verbose` 時のみ付与する。

---

## 2. 共通構造

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 2.1 共通フィールド

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`v`（定数 `SCHEMA_VERSION = 1`）、`event`、`session`、`at` は `baseEvent()` で自動付与し、各イベント型はこれに固有フィールドを加えた型とする。`at` は注入した `Clock` から取得する。

### 2.2 イベント種別

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/core/json/events.ts` に `AgentEvent` の判別可能ユニオンを定義する。仕様の表と 1 対 1 に対応させ、テストで各 `event` の必須フィールドを検証する。

### 2.3 結果オブジェクト

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`SearchResult` をそのまま `results[]` へ射影する（`toAgentResult()`）。`snippet` の切り詰めは正規化時点で済んでいるため、JSON 生成時に再切り詰めしない。

### 2.4 見出しオブジェクト

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`sections[]` は `filterByDepth()` の結果から `toAgentSection()` で生成する。`line` は 1 始まりの行番号。

### 2.5 エラーオブジェクト

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/core/errors/catalog.ts` にコードごとの定型メッセージとヒントを持たせ、`toAgentError(code, details)` が組み立てる。カタログにないコードを生成しないことをテストで検査する。

---

## 3. エージェント向けの推奨手順

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 3.1 基本フロー

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`--help` の出力に `search` → `outline` → `read` の例を含める（`src/ui/cli/commands/help-text.ts`）。例は本設計の記述と一致させる。

### 3.2 一覧を読む際の指針

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`outline` イベントに `totalChars` と各節の `chars` を含めることで、本文取得前の見積もりを可能にする。この方針によりスニペット以上の判断材料を一覧段階で追加表示しない。

### 3.3 失敗時の回復

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`error.hint` はコードごとのカタログ文言とし、回復手順を 1 文で含める。リトライは自動で行わない（利用者・エージェントの判断に委ねる）。

### 3.4 予算管理

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`--max-chars` を小さく指定した場合の `truncated: true` と `nextSectionId` を返す。`read` は 1 回の呼び出しで 1 文書内の複数節を返せるが、文書をまたぐ場合は呼び出しを分ける。

---

## 4. 安全性

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 4.1 信頼境界

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`content` イベントは `url` を必ず含み、`text` は外部由来である。`v` と `event` によりデータ種別を機械的に判別できるようにする。指示文の解釈禁止はツール利用側の責務であり、本ツールは出所を明示することで支援する。

### 4.2 秘匿情報

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

JSON 生成の直前に `redactUrl()` を通し、API キー・Cookie は出力経路に存在しない（設定から読み取った直後に取得層へ渡し、`ResolvedConfig` には値を持たせない）。

### 4.3 再現性

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

見出し識別子は本文から決定する。キャッシュ失効後の再取得で見出し構成が変化した場合、新しい索引で識別子を振り直し、`warnings` に `W3004`（見出し構成の変化）を追加する。
