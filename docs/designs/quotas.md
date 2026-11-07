# 設計書: クォータとレート制限（JSON）

本書は `REF:docs/specifications/quotas.md` に対応する実装設計を記述する。

---

## 1. 基本方針

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 1.1 2 種類の制御

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

純粋な判定ロジックは `src/core/quota.rs` に置く（`Counter`, `QuotaPolicy`, `Decision`）。レート制限は `src/usecases/guard/rate_limiter.rs`、クォータは `src/usecases/guard/quota.rs` が担当し、両者を `src/usecases/guard/mod.rs` の `Guard` がまとめる。

### 1.2 ポリシーの参照

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`QuotaPolicy` は `core::config::quota` に定義し、`adapters::config::loader` が `quotas/<id>.json` から読む。`RefOr::Inline` の場合は `settings.json` 内の定義をそのまま使う。

### 1.3 上限到達時の振る舞い

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`Guard::check(kind, amount) -> Result<Decision>`。失敗時は `AppError::Quota { metric, limit, used, resets_at, hint }` を返し、`src/ui/render/error_text.rs` が 4 要素を整形する。`onExceed=warn` の場合は `Decision::Allowed { warn: true }` を返し、呼び出し側が `W5005` を表示する。

---

## 2. クォータポリシーの構造

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 2.1 トップレベル

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`QuotaPolicy { schema_version, id, description, enabled, session: Limits, day: Limits, window: Vec<WindowLimits>, total: Limits, on_exceed: OnExceed, extra }`。`Limits` は 4 項目とも `Option<u64>`。

### 2.2 計測項目

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`enum Metric { Searches, Fetches, Results, Chars }` を定義し、`Limits::get(metric) -> Option<u64>` で取得する。表示名は `Metric::as_str()` に集約する（`searches` 等）。

### 2.3 日次の区切り

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`core::quota::day_key(now_local: DateTime<Local>) -> String`（`YYYY-MM-DD`）。`Counter.day.key` に保存し、キーが変わったら消費量を 0 に戻す。`Clock` は UTC を返し、`Local` への変換は `core::clock::to_local` に閉じる。

### 2.4 時間窓（`window`）

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`WindowCounter { started_at: DateTime<Utc>, used: Usage }` を `Vec` で保持し、`core::quota::window_active(&w, now) -> bool` で有効性を判定する。期限切れの窓は次の消費時に作り直す（判定は `started_at + minutes`）。

### 2.5 累積（`total`）

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`quota reset --scope total` でのみ 0 になる。それ以外の操作では減らさない。

### 2.6 `onExceed`

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`OnExceed::Warn` の「2 倍で停止」は `core::quota::hard_limit(limit) -> u64`（`limit.saturating_mul(2)`）で判定する。

---

## 3. 消費の単位と数え方

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 3.1 消費のタイミング

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`Guard::consume(usage: Usage)` を各ユースケースの成功直後に 1 回だけ呼ぶ。`retry` は試行ごとに `consume(Usage::searches(1))` を呼ぶ（`adapters::http::retry` にコールバックを渡す）。

### 3.2 失敗時の扱い

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`guard::check` と `guard::consume` を分離することで「失敗時に消費しない」を実現する。`AppError` を返す経路では `consume` を呼ばない。

### 3.3 消費の記録

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`CounterStore` トレイトの `save(&Counters)` は `adapters/state/atomic.rs` の原子的書き込みを用いる。`Counters` 全体を 1 ファイルに保存し、部分書き込みを避ける。

---

## 4. 状態と表示

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 4.1 カウンタの保持

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`Counters { day: DayCounter, window: Vec<WindowCounter>, total: Usage, updated_at }` を `counters.json` に保存する。セッション単位は `SessionState.usage` に保持する。

### 4.2 残量の表示

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`core::quota::remaining(policy, counters, session_usage, now) -> Vec<MetricUsage>` で一覧を作り、`src/ui/render/quota_text.rs` が 1 行に整形する。20% 未満の注記は `quota_text::low_warning(&[MetricUsage])` が返す。

### 4.3 例

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`quota_text::summary_line` が仕様の例と同じ形式を出力する。リセット時刻は `%Y-%m-%dT%H:%M%:z` で表示する。

### 4.4 コマンド

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/ui/cli/commands/quota.rs` に `show` / `reset` / `check` を実装する。`reset` は `--yes` を要求し、実行後に `delete` 相当の履歴イベント（`type: "quota_reset"`）を記録する。

---

## 5. 既定のポリシー

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

同梱の `assets/quotas/default.json` に仕様の表を記述し、`include_str!` で読み込む。コードに既定値を二重定義しない（既定値は JSON のみ）。

---

## 6. 監査

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 6.1 履歴への記録

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`HistoryEvent` に `cost: Usage` と `remaining: Vec<MetricUsage>` を持たせる。イベント生成は `usecases::history::record` が 1 か所で行う。

### 6.2 改変の検出

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`load_counters` の失敗時は `.corrupt-<timestamp>` へ退避し、`Counters::default()` で再開する。`W5001` と直前の `updated_at` を履歴の `quota_recovered` イベントに記録する。

### 6.3 上限の引き上げ

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

適用ポリシー ID を `SessionState.quota_policy` に保持し、履歴の各イベントに含める。
