# 設計書: エージェント連携（JSON 出力）

本書は `REF:docs/specifications/agent-integration.md` に対応する実装設計を記述する。

---

## 1. 基本方針

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 1.1 JSON Lines

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/ui/render/json_events.rs` の `emit(reporter, event)` が `serde_json::to_string(&event)` を `reporter.data()` へ渡す。終端保証は `src/ui/cli/commands/dispatch.rs` の `run_command()` が行い、成功時に `done`、失敗時に `error` を 1 回だけ出力する。二重出力を防ぐため `Emitter { closed: bool }` を持つ。

### 1.2 無状態の原則

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`--session` 省略時の `search` は新規セッションを作成し、`search` イベントの `session` に識別子を含める。プロセス内にグローバルキャッシュを持たない（`tokio` タスクローカルも使わない）。

### 1.3 出力サイズ

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

本文は `context.maxChars` の範囲に収める。`details` は `--verbose` 時のみ付与し、既定では `code` / `message` / `hint` のみとする。

### 1.4 クォータの可視性

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`SearchEvent` / `NextEvent` / `DocumentEvent` / `ContentEvent` は `quota: QuotaView` を必須フィールドとして持つ（`Option` にしない）。`QuotaView` は `usecases::guard::quota::remaining` の結果から `From` 実装で生成する。テストでは「消費を伴うイベントに `quota` が存在する」ことを `tests/contract/json_contract.rs` で検証する。

---

## 2. 共通構造

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 2.1 共通フィールド

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`#[serde(flatten)] base: BaseEvent { v: u32, event: EventKind, session: String, at: String }` を各イベント構造体に持たせる。`SCHEMA_VERSION: u32 = 1`。`at` は `Clock` から取得し `%Y-%m-%dT%H:%M:%SZ` で整形する。

### 2.2 イベント種別

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`#[serde(tag = "event", rename_all = "snake_case")] enum AgentEvent { Search(..), Next(..), Document(..), Outline(..), Content(..), History(..), Engines(..), Quota(..), Done(..), Error(..) }`。`serde` の内部タグ付き列挙を使い、フィールド名を構造体側で固定する。各イベントの必須フィールドは `tests/contract/json_schema.rs` で検証する。

### 2.3 結果オブジェクト

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`From<&SearchHit> for AgentResult` を実装し、`snippet` の再切り詰めは行わない。

### 2.4 見出しオブジェクト

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`From<&Heading> for AgentSection`。`line` は 1 始まり。`hidden` フラグ（深度・フィルタで隠れた見出し）も含める。

### 2.5 フィルタ集計オブジェクト

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`From<&FilterStats> for AgentFiltered` を実装し、`reasons` は `BTreeMap<String, usize>` をそのまま `serde_json::Map` へ変換する（順序を固定するため）。

### 2.6 クォータオブジェクト

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`QuotaView { metrics: Vec<MetricView> }`、`MetricView { name, used, limit: Option<u64>, resets_at: Option<String> }`。`limit` が未設定（無制限）の項目も必ず含め、`limit: null` とする（項目の有無で判断させない）。

### 2.7 エラーオブジェクト

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/core/errors/catalog.rs` に `fn message(code: ErrorCode, locale: Locale) -> &'static str` と `fn hint(code: ErrorCode, locale: Locale) -> &'static str` を置き、`AgentError::from(&AppError)` が組み立てる。カタログにないコードはコンパイル時に検出するため、`ErrorCode` は `enum` とし `match` を網羅させる。

---

## 3. エージェント向けの推奨手順

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 3.1 基本フロー

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/ui/cli/help.rs` の `long_about` に 3 コマンドの例を含める。例は仕様書の記述と一致させる（差分が出たらテストで落とす）。

### 3.2 一覧を読む際の指針

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`outline` イベントに `totalChars` と各節の `chars` を含める。検索イベントには `filtered`（集計）のみを含め、本文は含めない。

### 3.3 失敗時の回復

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`error.hint` はコードごとの固定文言。自動リトライは `network.retry` の範囲でのみ行い、アプリケーション層での再試行はしない。

### 3.4 予算管理

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`--max-chars` を小さく指定した場合の `truncated: true` と `nextSectionId` を返す。`read` は 1 回の呼び出しで 1 文書内の複数節を返せるが、文書をまたぐ場合は呼び出しを分ける。

---

## 4. 安全性

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 4.1 信頼境界

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`content` イベントは `url` を必ず含み、`text` は外部由来である。`event` フィールドでデータ種別を機械的に判別できるようにする。指示文の解釈禁止は利用側の責務であり、本ツールは出所を明示することで支援する。

### 4.2 秘匿情報

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

JSON 生成の直前に `redact::url_with_secrets` を通す。`SecretString` は `Serialize` を実装しないため、誤って出力経路に載せるとコンパイルエラーになる。

### 4.3 再現性

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

見出し識別子は本文から決定する。再取得で見出し構成が変化した場合は新しい索引で識別子を振り直し、`warnings` に `W3004` を追加する。
