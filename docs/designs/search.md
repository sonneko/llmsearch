# 設計書: 検索とページング

本書は `REF:docs/specifications/search.md` に対応する実装設計を記述する。エンジン定義の評価は `REF:docs/designs/engine-definitions.md` を参照する。

---

## 1. 検索の実行

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

### 1.1 エンジンの選択順

状態: 実装完了（REF:src/usecases/search.rs）

`src/usecases/search.rs` の `select_engine(opts, config, registry) -> Result<EngineDef>` が 5 層を順に解決し、`Origin` を併せて返す。解決過程は `--verbose` と `engines list --resolved` で表示できる。利用不能時は `AppError::Provider { kind: Misconfigured }` に `available` の一覧を `details` として付ける。

### 1.2 フォールバックとマージ

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

`run_with_fallback` は `for def in chain { match search_once(def).await { Ok(v) => return Ok(v.with_engine(def)), Err(e) if e.is_fallbackable() => continue, Err(e) => return Err(e) } }` の形にする。`is_fallbackable()` は `Provider { kind: NoResults | Blocked }` のみ `true` を返す（`E2xxx` は含めない）。マージは `search.merge=true` のとき `tokio::join!` ではなく逐次に実行し、レート制限を守る。

### 1.3 送信の制約

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

`usecases::guard::check(&GuardCtx)` を送信直前に 1 回だけ呼ぶ。`guard` はレート制限（`RateLimiter`）とクォータ（`Quota`）を順に判定し、消費は送信成功後に行う（失敗時に消費しない）。

---

## 2. 結果の正規化

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

### 2.1 フィールド

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

`core::model::SearchHit` に仕様のフィールドを持たせる。タイトルとスニペットの切り詰めは `core::text::truncate_chars` を用い、`…` を付す。`site` の導出は `core::url::host_of` が `www.` を 1 回だけ除去する。

### 2.2 URL の正規化

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

`core::url::normalize` が `url::Url` を用いて実施する。追跡パラメータの一覧は `core::url::TrackingParams` として設定から注入する。フラグメントは保持し、重複判定用のキーは `core::url::dedup_key`（フラグメント除去後の文字列）で作る。

### 2.3 重複の除去

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

`dedup_by_key(hits, dedup_key)` は `HashSet` で初出を残す。相対 URL の結果は `url::Url::parse` 失敗として破棄し、破棄件数を `W4004` に加算する。

### 2.4 並び順

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

ソート処理を入れない。フィルタ適用後も相対順序が保存されることを `tests/unit/search_order.rs` で検証する。

---

## 3. ページング

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

### 3.1 ページサイズ

状態: 実装完了（REF:src/core/search.rs）

`src/core/search.rs` の `plan_requests(limit, max_page_size) -> Vec<(offset, size)>` が分割を行う。`limit > max_page_size` の場合に複数リクエストへ分割し、合計が `limit` になるようにする。

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

`tokio::sync::Semaphore` で `network.maxConcurrency` を制御する。検索は `Semaphore::acquire` を 1 本に固定した専用セマフォで直列化する。

### 5.3 robots.txt

状態: 実装完了（REF:src/core/robots.rs）

robots.txt の解析は自前の小さな実装（`src/core/robots.rs`）とする。`User-agent: *` と `User-agent: llmsearch` の `Allow`/`Disallow` を評価し、最長一致を採用する。判定結果は `HashMap<host, RobotsRules>` にキャッシュする。

### 5.4 User-Agent

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

既定値は `env!("CARGO_PKG_VERSION")` と `env!("CARGO_PKG_REPOSITORY")` から組み立てる。設定側で空文字を許さない検証を `core::config::NetworkProfile::validate` に置く。

### 5.5 クォータとの関係

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

`guard::check` は `rate::check` → `quota::check` の順に呼び、両方通過した場合のみ送信する。消費は `guard::consume` として送信成功後に別途呼ぶ。
