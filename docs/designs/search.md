# 設計書: 検索とページング

本書は `REF:docs/specifications/search.md` に対応する実装設計を記述する。エンジン定義の評価は `REF:docs/designs/engine-definitions.md` を参照する。

---

## 1. 検索の実行

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

### 1.1 エンジンの選択順

状態: 実装完了（REF:src/usecases/search.rs）

既定エンジンは `src/usecases/config.rs` が `Bootstrap.engine` → `LLMSEARCH_ENGINE` → 設定 `search.engine` → 認証不要の有効エンジンの定義順先頭 → 有効エンジンの先頭、の順に解決し、`App.default_engine` に保持する。`--engine` などの明示指定が未知の ID だった場合は `E4002`（`engine '<id>' was not found`）、1 件も使えるエンジンが無い場合も `E4002`。

### 1.2 フォールバックとマージ

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

`usecases::search::run` は `search.merge=true` なら `run_merge`、そうでなければ `run_fallback` に分岐する（`candidate_engines` がどちらの連鎖を作るかも同じ条件で決める）。`run_fallback` は候補を順に試し、`ports::classify(&err) == FallbackClass::RetryOtherEngine`（`E4001` / `E4003` / `E4004`）のときだけ次の候補へ進む。`E2xxx` を含む他は即座に返す。

`run_merge` は `search.engines` を列挙順に逐次呼び、成功したエンジンの結果だけを集めて `core::search::merge_round_robin` で統合する。並列送信はしない（レート制限を守るため）。`search.merge=true` かつ `search.engines` が空の設定は `Settings::validate()` が `E1004` とするため、`run_merge` には空で入らない（防御的に空なら `E4002`）。

### 1.3 送信の制約

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

`Guard::check(Metric::Searches, n)` を送信直前に呼ぶ（`run_fallback` は試行ごとに `pending.searches + 1` で再判定する）。消費は `Guard::consume_all(Usage)` で送信成功後にまとめて行い、全候補が失敗した場合は消費しない。レート制限（同一ホストへの最小間隔）は `NetworkProfile.min_interval_ms` に基づき HTTP 層が待機する。

---

## 2. 結果の正規化

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

### 2.1 フィールド

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

`core::model::SearchHit` に仕様のフィールドを持たせる。タイトルとスニペットの切り詰めは `core::text::truncate_chars` を用い、`…` を付す。`site` の導出は `core::url::host_of` が `www.` を 1 回だけ除去する。

### 2.2 URL の正規化

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

`core::url::normalize(url, extra_tracking)` が `url::Url` を用いて実施し、スキームとホストを小文字化し、既定ポートを落とし、追跡パラメータ（`core::url::DEFAULT_TRACKING` と `extra_tracking`）を除去する。追跡パラメータの一覧は `App.tracking`（`usecases::config::default_tracking` が `DEFAULT_TRACKING` から作る固定リスト）で、設定キーからは注入しない。フラグメントは保持し、重複判定用のキーは `core::url::dedup_key`（フラグメント除去後の文字列）で作る。

### 2.3 重複の除去

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

`core::search::dedup(hits)` が `HashSet` に `core::url::dedup_key` を入れて初出を残し、除去件数を返す。`dedup_key` を作れない URL（`url::Url::parse` の失敗）は原文のままキーにする（破棄しない）。

### 2.4 並び順

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

ソート処理を入れない。フィルタ適用後も相対順序が保存されることを `REF:tests/contract.rs` で検証する。

---

## 3. ページング

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

### 3.1 ページサイズ

状態: 実装完了（REF:src/core/search.rs）

`src/usecases/search.rs` がエンジンへ渡す `limit` を `def.limits.max_page_size.min(limit)` で頭打ちにする。分割取得は行わず、1 ページにつき 1 リクエストとする（`maxPageSize` を超える件数は次ページで取る）。

### 3.2 次ページの取得

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

`core::search::paging::next_position(style, current, page_base, offset_step, page_size)` が次の位置を計算する純粋関数。`usecases::search::next` はセッションの `page` を進める前に取得し、成功時のみ更新する。

### 3.3 取得上限と終端

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

`TerminationReason`（`MaxResults`, `Quota`, `EmptyPage`, `AllFiltered`）を返し、描画側が理由別の文言を出す。判定は `effective_limits` とクォータ残量の最小値で行う。

### 3.4 失敗時の扱い

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

`next` は状態を書き戻す前に結果を返し、エラー時は書き戻しを行わない（`page` を進めない）。

---

## 4. 検索語の修正

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

### 4.1 挙動

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

`SessionState.queries` に要素を追加し、`active_query` を切り替える。結果一覧は `results_by_query` のスロットとして保持し、過去の一覧を保持する。

非対話の `refine` は `REF:src/ui/commands.rs` から `search::refine` を呼び、変更前の検索語をセッションの `last_query`（または明示の `--previous`）から取得する。履歴に `Refine` を追記し、JSON に `previousQuery` を含める。結果の別スロット保存は未対応のため、Issue #7 の残項目とする。

### 4.2 検索語の履歴

状態: 実装完了（REF:src/core/select.rs）

`!<n>` の展開は `src/core/select.rs`。範囲外は `E1005`。展開後に `normalize_query` を通す。

### 4.3 入力の正規化

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

`core::text::normalize_query` が全角空白→半角、連続空白の圧縮、制御文字の除去、512 文字での切り詰めを行う。切り詰め時は `Diagnostic::warn(W1003)` を返す。

---

## 5. 礼儀とレート

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

### 5.1 ホスト単位の制限

状態: 実装完了（REF:src/usecases/guard.rs）

`src/usecases/guard.rs` が `HashMap<String, Instant>` を持ち、`Clock` 経由で現在時刻を取得する。待機は `Clock::sleep` を呼び、テストでは即座に進む仮想時計を使う。

### 5.2 同時実行

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

`network.maxConcurrency` は HTTP クライアントの接続プール（`reqwest::ClientBuilder::pool_max_idle_per_host`）に渡し、同時リクエスト数を間接的に制限する（`src/adapters/http.rs`）。検索エンジンの呼び出しを直列化するセマフォは持たない（`usecases::search` はエンジンを 1 つずつ順に呼ぶ）。

### 5.3 robots.txt

状態: 実装完了（REF:src/core/robots.rs）

robots.txt の解析は自前の小さな実装（`src/core/robots.rs`）とする。`User-agent: *` と `User-agent: llmsearch` の `Allow`/`Disallow` を評価し、最長一致を採用する。判定結果は `HashMap<host, RobotsRules>` にキャッシュする。

### 5.4 User-Agent

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

既定値は `env!("CARGO_PKG_VERSION")` と `env!("CARGO_PKG_REPOSITORY")` から組み立てる。設定側で空文字を許さない検証を `core::config::NetworkProfile::validate` に置く。

### 5.5 クォータとの関係

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

`guard::check` は `rate::check` → `quota::check` の順に呼び、両方通過した場合のみ送信する。消費は `guard::consume` として送信成功後に別途呼ぶ。
