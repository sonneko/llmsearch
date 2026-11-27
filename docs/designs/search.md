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

`run_merge` は `search.engines` を列挙順に逐次呼び、成功したエンジンの結果だけを集めて `core::search::merge_round_robin` で統合する。通常の失敗は警告として除外するが、`E5005` は全体を直ちに中止し、後続エンジンへ送信しない。並列送信はしない（レート制限を守るため）。`search.merge=true` かつ `search.engines` が空の設定は `Settings::validate()` が `E1004` とするため、`run_merge` には空で入らない（防御的に空なら `E4002`）。

### 1.3 送信の制約

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

`call_engine` は `evaluate_with_preflight` に callback を渡し、各 HTTP 送信の直前（page-style の隣接 provider page、抽出 retry、HTTP 層の transport retry を含む）に `Guard::check(Metric::Searches, pending.searches + 1)` を行う。`HttpClient::send_blocking_with_preflight` は `ReqwestClient` の各 transport 試行前にも callback を呼ぶ。通過した送信数は `pending.searches` に積む。`Guard` はクォータだけを担当し、成功後に `Guard::consume_all(Usage)` でまとめて永続化する。全候補が失敗した場合は消費しない。ホスト単位のレート制限は `ReqwestClient::wait_interval`（`src/adapters/http.rs`）が `NetworkProfile.min_interval_ms` に基づき待機し、必要時に `W2001` を出す。

---

## 2. 結果の正規化

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

### 2.1 フィールド

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

`core::model::SearchHit` に仕様のフィールドを持たせる。`site` の導出は `core::url::host_of` が `www.` を 1 回だけ除去する。`search.titleChars` / `search.snippetChars` による切り詰めは現状 `REF:src/ui/interactive.rs` の対話表示でのみ参照され、非対話出力（`REF:src/ui/render.rs` は端末幅のみで切り詰め）および `--json`・セッション保存値には適用されていない（Issue #55）。

### 2.2 URL の正規化

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

`core::url::normalize(url, extra_tracking)` が `url::Url` を用いて実施し、スキームとホストを小文字化し、既定ポートを落とし、追跡パラメータ（`core::url::DEFAULT_TRACKING` と `extra_tracking`）を除去する。追跡パラメータの一覧は `App.tracking`（`usecases::config::default_tracking` が `DEFAULT_TRACKING` から作る固定リスト）で、設定キーからは注入しない。フラグメントは保持し、重複判定用のキーは `core::url::dedup_key`（フラグメント除去後の文字列）で作る。なお現状 `REF:src/core/extract.rs` は `absolutize` 変換の指定有無に関わらず相対 URL を `base_url.join` で自動解決し、また `REF:src/core/url.rs` のクエリ秘匿判定が部分一致のため一般語（`author` 等）も伏字化する既知課題がある（Issue #58）。

### 2.3 重複の除去

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

`core::search::dedup(hits)` が `HashSet` に `core::url::dedup_key` を入れて初出を残し、除去件数を返す。`dedup_key` を作れない URL（`url::Url::parse` の失敗）は原文のままキーにする（破棄しない）。

### 2.4 並び順

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

検索ユースケースは独自にソートしない。規則フィルタは相対順序を保つが、`filters.diversity=true` の場合は `core::config::filter::interleave_by_host` がホストごとに交互配置する。

---

## 3. ページング

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

### 3.1 ページサイズ

状態: 実装完了（REF:src/core/search.rs）

`search_once` は各要求の `limit` を `def.limits.max_page_size` で制限する。`offset` 方式は 1 回要求する。`page` 方式では `paging.page_size`（要求件数を既定とし `max_page_size` で制限）をプロバイダのページ幅として、論理ページの必要範囲を埋める隣接ページを読み、`search.limit` 件の範囲を切り出す。

### 3.2 次ページの取得

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

`core::template::paging_values(paging, page, limit)` が offset / provider-page 値を計算する。offset 要求数は `min(search.limit, limits.maxPageSize)` とし、その実要求数がネイティブ幅より小さい場合は同じ値を offset 増分に使う。それ以外は `offsetStep` に従う。CLI の `search --page` の範囲検証もこの有効増分で最大ページ数を求める。`page` 方式では論理ページを満たす範囲のプロバイダページを取得して切り出す。CLI の `next` / `refine` はセッションの `last_engine` を復元してから実行し、`apply_to_session` は成功した結果だけでページ状態と識別子を保存する。

### 3.3 取得上限と終端

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

`SearchOutcome` は `has_more` と文字列の `termination_reason` を返す。終端理由は `pagingUnsupported` / `maxResults` / `filtered` / `emptyPage`（`next` が送信前にクォータで拒否された場合は `quota`）で、JSON と履歴に同じ識別子を載せる。上限・quota で `next` が拒否されたときはセッションの検索状態を先に書き換えず、ページ状態を保持したまま `added: 0` の `next` 履歴を記録する。

### 3.4 失敗時の扱い

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

`next` は状態を書き戻す前に結果を返し、エラー時は書き戻しを行わない（`page` を進めない）。

---

## 4. 検索語の修正

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

### 4.1 挙動

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

`apply_to_session` は page 1 または直近の検索語と異なる結果を新しい `QueryRecord` として `queries` に追加し、`active_query` / `last_query` / `last_engine` を更新する。ページングでは同一クエリの結果を単一の `SessionState.results` に追記し、結果 ID は表示時と同じ値を保存する。クエリごとの別結果スロットは持たず、refine は現在の結果一覧を置き換える。

非対話の `refine` は `commands.rs` が既存セッションの `last_query`（空なら最後の `QueryRecord`）を変更前の検索語として `search::refine` に渡す。新しい検索は page 1 として実行し、`apply_to_session` が現在の `results` を置き換えて `queries` に `QueryRecord` を追加する。`last_query` / `last_engine` / `page` も更新し、履歴には `Search` と `Refine` のイベントを記録する。JSON の `refine` イベントは `previousQuery` を含む。変更通知 `W4001` は `previous != new_query` のときだけ追加し、同一クエリ再検索時の自己矛盾警告を抑止する（`REF:tests/m16.rs`。ただし `new_query` が正規化前に比較されるため前後空白のみで `W4001` が出る点と履歴要約表示は Issue #58 / #62 で追跡）。

### 4.2 検索語の履歴

状態: 実装完了（REF:src/usecases/search.rs）

`recent_queries(app, limit)` は `search.rs` が履歴から最近の検索語を列挙する補助関数だが、`search` 入力で `!<n>` を展開する機能は実装していない。通常の `search` / `refine` の入力は `prepare_query` で正規化・上限長検査する。

### 4.3 入力の正規化

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

`core::text::normalize_query` が全角空白→半角、連続空白の圧縮、制御文字の除去、512 文字での切り詰めを行う。切り詰め時は `Diagnostic::warn(W1003)` を返す。

---

## 5. 礼儀とレート

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

### 5.1 ホスト単位の制限

状態: 実装完了（REF:src/usecases/guard.rs）

`src/adapters/http.rs::ReqwestClient` がホスト別の最終送信時刻を `Mutex<HashMap<String, u64>>` で保持し、`NetworkProfile.min_interval_ms` に足りない場合は `Clock::sleep` で待機する。待機が発生した場合は reporter 経由で `W2001` を通知する。

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

クォータは §1.3 の `evaluate_with_preflight` callback が各送信前に `Guard::check` で判定し、成功した操作の全使用量を `Guard::consume_all` で確定する。レート待機は `ReqwestClient::wait_interval` が送信処理内で実施し、`Guard` の責務には含めない。
