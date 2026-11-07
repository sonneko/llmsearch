# 設計書: 閲覧履歴

本書は `REF:docs/specifications/history.md` に対応する実装設計を記述する。

---

## 1. 記録対象

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 1.1 記録するイベント

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/core/history/event.rs` に `#[serde(tag = "type", rename_all = "snake_case")] enum HistoryEvent` を置き、種別ごとに `payload` の型を固定する。記録は各ユースケースの末尾で `HistoryStore::append(event)` を 1 回だけ呼ぶ（複数箇所から呼ばない）。

### 1.2 記録しない情報

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

保存直前の `src/core/history/redact.rs::redact(&mut event)` で、URL の秘匿パラメータを伏字化し、`payload` に本文が含まれていないことを型で担保する（本文フィールドを持たせない）。秘匿パラメータ名は定数配列。

### 1.3 記録の単位

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`id` は保存ファイル内の最大番号 +1 を `H<n>` として `HistoryStore` が付与する。`at` は `Clock` から取得し `%Y-%m-%dT%H:%M:%SZ` で整形する。`cost` / `remaining` は `Usage` / `Vec<MetricUsage>` の `Copy` 型。

---

## 2. 履歴の表示

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 2.1 一覧

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/ui/render/history_text.rs::render_list(entries)` が 1 行 1 件を生成する。要約は `src/core/history/summarize.rs::summary(&event)` に集約し、対話画面とコマンドで共有する。

### 2.2 詳細表示

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`history show <id>` はレコード全フィールドを整形して出力する。`read` レコードでは `usecases::document::read_by_ref` を呼び、キャッシュ有効時は取得を行わない。

### 2.3 検索

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/core/history/filter.rs::filter(events, criteria)` は純粋関数。キーワードは大文字小文字を区別せず部分一致、日付範囲は `at` の UTC 日付で比較する。

### 2.4 セッション単位の表示

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`Criteria { session: Some(..), .. }` を `filter` に渡す。対話画面は現行セッション ID を固定で渡す。

---

## 3. 再出力

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 3.1 節の再出力

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`read` レコードの `documentId` / `sectionId` から文書キャッシュを引く（セッションに依存しない）。出力ヘッダに `再出力（元: <日時>）` を追記する。取得失敗は `E2001`。

### 3.2 検索の再実行

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`history show <id> --rerun` は `usecases::search::run` を呼ぶ前に `guard::check` の結果を表示し、`--yes` が無ければ確認する。

### 3.3 エクスポート

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

Markdown は `src/ui/render/history_markdown.rs`。`--with-body` 時のみ `body.md` を読む。JSON Lines は履歴ファイルの行をそのまま出力し、`serde_json` で再整形しない（改変を避けるため）。

---

## 4. 保持と削除

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 4.1 保持期間

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

読み込み時に `retentionDays` を過ぎたレコードを除外し、`purge` 操作時にファイルからも削除する。`0` は無期限。

### 4.2 上限

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

追記後に件数を確認し、超過時は末尾 `maxEntries` 行を残した新ファイルを作成して `rename` する。

### 4.3 削除

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

全削除は確認入力または `--yes` を要し、削除前に件数を表示する。削除後に空ファイルを作成し、`delete` イベントを 1 件だけ記録する。

### 4.4 記録の整合性

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

追記は `OpenOptions::append(true)` で行い、既存行を書き換えない。読み込み時の不正行スキップ件数は `W5004` として返し、`history list` の末尾に注記する。
