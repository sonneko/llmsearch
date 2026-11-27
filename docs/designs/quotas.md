# 設計書: クォータとレート制限（JSON）

本書は `REF:docs/specifications/quotas.md` に対応する実装設計を記述する。

---

## 1. 基本方針

状態: 実装完了（REF:src/core/config/quota.rs, REF:src/usecases/guard.rs）

### 1.1 2 種類の制御

状態: 実装完了（REF:src/core/config/quota.rs, REF:src/usecases/guard.rs）

純粋なクォータ判定ロジックは `src/core/config/quota.rs` に置く（`Counters`, `QuotaPolicy`, `QuotaDecision`）。`src/usecases/guard.rs` の `Guard` はクォータ判定と消費を担当する。ホスト単位のレート間隔は `src/adapters/http.rs::ReqwestClient::wait_interval` が `NetworkProfile.min_interval_ms` に従って待機し、必要時に `W2001` を通知する。

### 1.2 ポリシーの参照

状態: 実装完了（REF:src/core/config/quota.rs, REF:src/adapters/config/mod.rs）

`QuotaPolicy` は `core::config::quota` に定義し、`src/adapters/config/mod.rs` の `resolve_quota` が `quotas/<id>.json`（未発見時は `src/assets.rs` の埋め込み既定）から読む。`RefOr::Inline` の場合は `settings.json` 内の定義をそのまま使う。

### 1.3 上限到達時の振る舞い

状態: 実装完了（REF:src/core/config/quota.rs, REF:src/usecases/guard.rs, REF:src/ui/commands.rs）

`Guard::check(metric, amount) -> Result<(), AppError>` が `core::config::quota::check` に委譲する。超過時は `ExceedInfo::to_error()` が `E5005` を作り、`detail` に `<scope>.<項目>: requested <要求量>, used <現在>/<上限> (projected <要求後>/<上限>)`（日次・窓ならリセット時刻付き）を入れる。`quota check` の許可時は `used/limit` と残量を返す。`onExceed=warn` は実効上限の 2 倍未満を許可して `W5005` を出す設計だが、現状 `PolicyAction::Warn` が上限未達でも `warn: true` を返す不具合があり Issue #61 で追跡する。

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

`WindowCounter { minutes: u32, started_at: String, used: Usage }` を `Vec` で保持し、`core::config::quota` 内で窓の有効期限（`started_at + minutes`）を判定する。期限切れの窓は次の消費時に作り直す。

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

- 検索（`REF:src/usecases/search.rs`）: 実際の HTTP 送信ごとに `Guard::check(Metric::Searches, pending.searches + 1)` を行い、通過した回数を `pending.searches` に積む。`call_engine` は `evaluate_with_preflight` に callback を渡し、`HttpClient::send_blocking_with_preflight` が `ReqwestClient` の transport retry ごとにも callback を呼ぶため、page-style の隣接 provider page・抽出 retry・HTTP retry すべてを送信直前に判定する。取得できた件数（フィルタ前）は `Usage.results` に入れ、`chars` は消費しない（仕様 §2.2 の `chars` は `read` の出力のみ）。積み上げた `Usage` はローカル変数 `pending` に保持し、**操作が成功したときだけ** `consume_all` で永続化する。
- `EngineCall.requests` は成功した評価で実際の送信回数を返すが、クォータ判定の累積値は各送信前の callback が保持するため、エンジン評価が最終的に失敗しても fallback / merge の次試行で既送信回数を含められる。
- 取得（`REF:src/usecases/document.rs`）: `Usage.fetches` のみを消費する。取得した本文の文字数は `chars` に入れない（`outline` だけで本文の予算を減らさないため）。`chars` は `read` が出力文字数で消費する。

### 3.2 失敗時の扱い

状態: 実装完了（REF:src/core/config/quota.rs, REF:src/usecases/guard.rs）

`guard::check` と `guard::consume` を分離することで「失敗時に消費しない」を実現する。`AppError` を返す経路では `consume` を呼ばない。

フォールバックは「送信したが失敗した試行」を `pending` に積んだまま次のエンジンへ進み、最終的に成功した時点で合算して永続化する。全ての候補が失敗した場合は `pending` を捨てるため、消費は 0 のままになる（仕様 §3.2 の「操作全体が失敗した場合は消費しない」に対応）。

### 3.3 消費の記録

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/adapters/state/session_store.rs）

`CounterStore` トレイトの `save(&Counters)` は `REF:src/adapters/state/mod.rs` の `write_atomic` を用いる。`Counters` 全体を状態ディレクトリ直下の `counters.json` 1 ファイルに保存し、部分書き込みを避ける。

---

## 4. 状態と表示

状態: 実装完了（REF:src/core/config/quota.rs, REF:src/usecases/guard.rs）

### 4.1 カウンタの保持

状態: 実装完了（REF:src/core/config/quota.rs, REF:src/usecases/guard.rs）

`Counters { day_key, session_key, session: Usage, day: Usage, window: Vec<WindowCounter>, total: Usage, updated_at }` を `counters.json` に保存する。明示セッション ID がある場合だけ `Guard::new` は `session_key` を比較・切り替え、異なる ID ならセッション消費量を 0 に戻す。ID が空のコマンドはセッション判定・表示・更新を行わず、既存 `session_key` と使用量を保持する。

### 4.2 残量の表示

状態: 実装完了（REF:src/ui/render.rs）

`core::config::quota::remaining(policy, counters, session: Option<&Usage>, now_utc, now_local_date) -> Vec<MetricUsage>` で設定済み上限の一覧を作る。セッション未選択時は session scope を含めない。`quota show` / `quota check` は `counters.session_key` が指す有効なセッションを暗黙再開して使用量を表示し、該当セッションが無ければ session scope を省く。`src/ui/commands.rs` がテキスト / JSON に整形し、`usecases::guard::usage_line(metric)` が対話モードの注記に使う。

### 4.3 例

状態: 実装完了（REF:src/core/config/quota.rs, REF:src/usecases/guard.rs）

`MetricUsage` は `name` / `used` / `limit: Option<u64>` / `resets_at` を持ち、`remaining()` は上限から使用量を差し引く。`quota show` はテキストで `used / limit (remaining N)` を出し、JSON 各要素は `name` / `used` / `limit` / `remaining` / `resetsAt` を持つ。設定済み上限のあるメトリクスだけ列挙し、セッション未選択時は `session.*` を省く。`resets_at` は日次の次のローカル 0 時、その他は `null`。

### 4.4 コマンド

状態: 実装完了（REF:src/ui/commands.rs）

`src/ui/commands.rs` の `run_quota` が `show` / `reset` / `check` を実装する。`show` / `check` は最新の有効セッションを再開し、セッション ID と使用量を標準エラーへ通知する（`--quiet` なら抑制）。`check` は `--kind` に `search(es)` / `fetch(es)` / `result(s)` / `char(s)` を受け付け（仕様 §4.4 の `read` との差分は Issue #61）、許可時に要求メトリクスの `used/limit` と残量を返し、超過時は要求量・現在量・要求後の見込み量を `E5005` detail に含める。`reset` は `--yes` を要求し、`--scope` が `session` / `day` / `window` / `total` / `all` 以外なら `E1004` を返し、許容される `--scope` 値を hint に全て示す（全消去にしない）。成功後は `type: "quota"` の履歴 1 件（`payload.action = "reset"`）を記録する。

---

## 5. 既定のポリシー

状態: 実装完了（REF:src/core/config/quota.rs, REF:src/usecases/guard.rs）

同梱の `assets/quotas/default.json` に仕様の表を記述し、`include_str!` で読み込む。コードに既定値を二重定義しない（既定値は JSON のみ）。

---

## 6. 監査

状態: 実装完了（REF:src/core/config/quota.rs, REF:src/usecases/guard.rs）

### 6.1 履歴への記録

状態: 実装完了（REF:src/ui/commands.rs, REF:src/core/history.rs）

検索・ページングなど消費を伴う操作の件数やエンジン情報は対応する `HistoryEntry.payload` に記録する。現在 `cost` / `remaining` の両フィールドを設定するのは `quota reset` の監査記録で、`EventType::Quota` の 1 レコードとして `src/ui/commands.rs::record_quota_reset` が書く。`payload` は `{ "action": "reset", "scope": ..., "policy": ..., "note": ... }` とし、`cost` にリセット前、`remaining` にリセット後の消費量を入れる（`Cost` は 4 項目のため、`MetricUsage` の一覧は `payload` に入れない）。

リセット前の値は `Counters` から読み、`app.counters.reset(scope)` の直後に記録する。履歴の追記に失敗してもリセットは成功として扱い、`Reporter::warn` で 1 行通知する（監査の失敗で解除を巻き戻すと、上限に張り付いたまま操作できなくなるため）。`scope` の解釈は `CounterStore::reset` と同じ語（`session` は `Counters.session`、`day` は `day` と `day_key`、`window` は `window`、`total` は `total`、`all` は `Counters::default()`）を用いる。

### 6.2 改変の検出

状態: 実装完了（REF:src/core/config/quota.rs, REF:src/usecases/guard.rs）

読めない `counters.json` は `FsCounters::load` が `.corrupt-<timestamp>` へ退避し、`Counters::default()` を返す。警告も履歴も出さない（退避先のファイルで事後に確認できる）。

### 6.3 上限の引き上げ

状態: 実装完了（REF:src/ui/commands.rs, REF:src/core/config/quota.rs）

現在有効なポリシー ID は `App.quota.id` に保持し、`quota show` / `quota check` / `quota reset` の出力および `quota reset` の履歴 `payload.policy` に記録する。
