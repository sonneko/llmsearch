# 設計書: クォータとレート制限（JSON）

本書は `REF:docs/specifications/quotas.md` に対応する実装設計を記述する。

---

## 1. 基本方針

状態: 実装完了（REF:src/core/config/quota.rs, REF:src/usecases/guard.rs）

### 1.1 2 種類の制御

状態: 実装完了（REF:src/core/config/quota.rs, REF:src/usecases/guard.rs）

純粋な判定ロジックは `src/core/config/quota.rs` に置く（`Counter`, `QuotaPolicy`, `Decision`）。レート制限は `src/usecases/guard.rs`、クォータは `src/usecases/guard.rs` が担当し、両者を `src/usecases/guard.rs` の `Guard` がまとめる。

### 1.2 ポリシーの参照

状態: 実装完了（REF:src/core/config/quota.rs, REF:src/usecases/guard.rs）

`QuotaPolicy` は `core::config::quota` に定義し、`adapters::config::loader` が `quotas/<id>.json` から読む。`RefOr::Inline` の場合は `settings.json` 内の定義をそのまま使う。

### 1.3 上限到達時の振る舞い

状態: 実装完了（REF:src/ui/render.rs）

`Guard::check(metric, amount) -> Result<(), AppError>` が `core::config::quota::check` に委譲する。超過時は `ExceedInfo::to_error()` が `E5005` を作り、`detail` に `<scope>.<項目> = <使用量>/<上限>`（窓なら `(resets <時刻>)` 付き）を入れる。`onExceed=warn` で上限の 2 倍未満のときは `QuotaDecision::Allowed { warn: true }` を返し、`Guard` が `W5005` を出す。

---

## 2. クォータポリシーの構造

状態: 実装完了（REF:src/core/config/quota.rs, REF:src/usecases/guard.rs）

### 2.1 トップレベル

状態: 実装完了（REF:src/core/config/quota.rs, REF:src/usecases/guard.rs）

`QuotaPolicy { schema_version, id, description, enabled, session: Limits, day: Limits, window: Vec<WindowLimits>, total: Limits, on_exceed: OnExceed, extra }`。`Limits` は 4 項目とも `Option<u64>`。

### 2.2 計測項目

状態: 実装完了（REF:src/core/config/quota.rs, REF:src/usecases/guard.rs）

`enum Metric { Searches, Fetches, Results, Chars }` を定義し、`Limits::get(metric) -> Option<u64>` で取得する。表示名は `Metric::as_str()` に集約する（`searches` 等）。

### 2.3 日次の区切り

状態: 実装完了（REF:src/core/config/quota.rs, REF:src/usecases/guard.rs）

`Counters.day_key` に `YYYY-MM-DD` のローカル日付を保存し、キーが変わったら消費量を 0 に戻す。`Clock` は UTC を返し、ローカル日付は `Clock::today_local()`（`ports::system::SystemClock` が `chrono::Local::now().date_naive()` を返す）で得る。キーの生成と比較は `usecases::guard::Guard::new` が行い、`core` 側に専用関数は置かない。

### 2.4 時間窓（`window`）

状態: 実装完了（REF:src/core/config/quota.rs, REF:src/usecases/guard.rs）

`WindowCounter { started_at: DateTime<Utc>, used: Usage }` を `Vec` で保持し、`core::quota::window_active(&w, now) -> bool` で有効性を判定する。期限切れの窓は次の消費時に作り直す（判定は `started_at + minutes`）。

### 2.5 累積（`total`）

状態: 実装完了（REF:src/core/config/quota.rs, REF:src/usecases/guard.rs）

`quota reset --scope total` でのみ 0 になる。それ以外の操作では減らさない。

### 2.6 `onExceed`

状態: 実装完了（REF:src/core/config/quota.rs, REF:src/usecases/guard.rs）

`OnExceed::Warn` の「2 倍で停止」は `core::config::quota::check` が `OnExceed::Stop` なら `limit`、`Warn` なら `limit.saturating_mul(2)` を実効上限として判定する。

---

## 3. 消費の単位と数え方

状態: 実装完了（REF:src/core/config/quota.rs, REF:src/usecases/guard.rs）

### 3.1 消費のタイミング

状態: 実装完了（REF:src/core/config/quota.rs, REF:src/usecases/guard.rs）

`Guard::consume_all(usage: Usage)` を各ユースケースの成功直後に 1 回だけ呼ぶ。消費は 1 か所にまとめ、経路ごとに散らばらせない。

- 検索（`REF:src/usecases/search.rs`）: 送信した回数を `Usage.searches`、得られた件数（フィルタ前）を `Usage.results` に入れる。`chars` は消費しない（仕様 §2.2 の `chars` は `read` の出力のみ）。フォールバックで複数エンジンを試した場合は、試行ごとに 1 を積む。積み上げた `Usage` はローカル変数 `pending` に保持し、**操作が成功したときだけ** `consume_all` で永続化する。そのため、次に試すエンジンの判定は `guard.check(Searches, pending.searches + 1)` のように「既に送った分 + これから送る 1 回」で行う。
- 同一エンジン内での再送信（抽出結果 0 件による再試行。`REF:src/adapters/engine/eval.rs`）は `EngineCall.requests` として送信回数を返し、検索がそれを `searches` に合算する。
- 取得（`REF:src/usecases/document.rs`）: `Usage.fetches` のみを消費する。取得した本文の文字数は `chars` に入れない（`outline` だけで本文の予算を減らさないため）。`chars` は `read` が出力文字数で消費する。

### 3.2 失敗時の扱い

状態: 実装完了（REF:src/core/config/quota.rs, REF:src/usecases/guard.rs）

`guard::check` と `guard::consume` を分離することで「失敗時に消費しない」を実現する。`AppError` を返す経路では `consume` を呼ばない。

フォールバックは「送信したが失敗した試行」を `pending` に積んだまま次のエンジンへ進み、最終的に成功した時点で合算して永続化する。全ての候補が失敗した場合は `pending` を捨てるため、消費は 0 のままになる（仕様 §3.2 の「操作全体が失敗した場合は消費しない」に対応）。

### 3.3 消費の記録

状態: 実装完了（REF:src/core/config/quota.rs, REF:src/usecases/guard.rs）

`CounterStore` トレイトの `save(&Counters)` は `adapters/state/atomic.rs` の原子的書き込みを用いる。`Counters` 全体を 1 ファイルに保存し、部分書き込みを避ける。

---

## 4. 状態と表示

状態: 実装完了（REF:src/core/config/quota.rs, REF:src/usecases/guard.rs）

### 4.1 カウンタの保持

状態: 実装完了（REF:src/core/config/quota.rs, REF:src/usecases/guard.rs）

`Counters { day_key, session_key, session: Usage, day: Usage, window: Vec<WindowCounter>, total: Usage, updated_at }` を `counters.json` に保存する。セッション消費量もこのファイルが持ち、`session_key` が現在のセッションと異なれば 0 に戻す（`Guard::new`）。

### 4.2 残量の表示

状態: 実装完了（REF:src/ui/render.rs）

`core::config::quota::remaining(policy, counters, session, now_utc, now_local_date) -> Vec<MetricUsage>` で一覧を作り、`src/ui/commands.rs` が `quota show` の表に、`usecases::guard::usage_line(metric)` が対話モードの 1 行注記に整形する。

### 4.3 例

状態: 実装完了（REF:src/core/config/quota.rs, REF:src/usecases/guard.rs）

`remaining` の各要素は `name` / `used` / `limit` / `resets_at` を持ち、`quota show` が `name used / limit resets_at` の 3 列で表示する。`resets_at` は日次項目についてだけ `YYYY-MM-DDT00:00 local` を返し、それ以外は `null` とする。

### 4.4 コマンド

状態: 実装完了（REF:src/ui/commands.rs）

`src/ui/commands.rs` の `run_quota` が `show` / `reset` / `check` を実装する。`reset` は `--yes` を要求し、`--scope` が `session` / `day` / `window` / `total` / `all` 以外なら `E1004`（全消去にしない）。成功後は `type: "quota"` の履歴 1 件（`payload.action = "reset"`）を記録する。

---

## 5. 既定のポリシー

状態: 実装完了（REF:src/core/config/quota.rs, REF:src/usecases/guard.rs）

同梱の `assets/quotas/default.json` に仕様の表を記述し、`include_str!` で読み込む。コードに既定値を二重定義しない（既定値は JSON のみ）。

---

## 6. 監査

状態: 実装完了（REF:src/core/config/quota.rs, REF:src/usecases/guard.rs）

### 6.1 履歴への記録

状態: 実装完了（REF:src/ui/commands.rs, REF:src/core/history.rs）

消費を伴う操作は `HistoryEntry` の `cost` / `remaining` に消費量と残量を入れて記録する。`quota reset` は `EventType::Quota` の 1 レコードとして `src/ui/commands.rs` の `record_quota_reset` が記録する。`payload` は `{ "action": "reset", "scope": ..., "policy": ..., "note": ... }` とし、`cost` にリセット前、`remaining` にリセット後の消費量を入れる（`Cost` は 4 項目のため、`MetricUsage` の一覧は `payload` に入れない）。

リセット前の値は `Counters` から読み、`app.counters.reset(scope)` の直後に記録する。履歴の追記に失敗してもリセットは成功として扱い、`Reporter::warn` で 1 行通知する（監査の失敗で解除を巻き戻すと、上限に張り付いたまま操作できなくなるため）。`scope` の解釈は `CounterStore::reset` と同じ語（`session` は `Counters.session`、`day` は `day` と `day_key`、`window` は `window`、`total` は `total`、`all` は `Counters::default()`）を用いる。

### 6.2 改変の検出

状態: 実装完了（REF:src/core/config/quota.rs, REF:src/usecases/guard.rs）

読めない `counters.json` は `FsCounters::load` が `.corrupt-<timestamp>` へ退避し、`Counters::default()` を返す。警告も履歴も出さない（退避先のファイルで事後に確認できる）。

### 6.3 上限の引き上げ

状態: 実装完了（REF:src/core/config/quota.rs, REF:src/usecases/guard.rs）

適用ポリシー ID を `SessionState.quota_policy` に保持し、履歴の各イベントに含める。
