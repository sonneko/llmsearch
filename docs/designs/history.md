# 設計書: 閲覧履歴

本書は `REF:docs/specifications/history.md` に対応する実装設計を記述する。パスは実装予定の配置である。

---

## 1. 記録対象

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 1.1 記録するイベント

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

イベント種別は `src/core/history/event.ts` の判別可能ユニオン `HistoryEvent` で表現し、`type` ごとに `payload` の型を定める。記録は各ユースケースの末尾で `HistoryStore.append(event)` を 1 回だけ呼ぶ。

### 1.2 記録しない情報

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

保存直前のフック `redact(event)`（`src/core/history/redact.ts`）で、URL の秘匿パラメータを伏字化し、`payload` に本文が含まれていないことを検査する。伏字対象のパラメータ名は定数配列で管理する。

### 1.3 記録の単位

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`id` は保存ファイル内の最大番号 +1 を `H<n>` として付与する。付与は `HistoryStore` の責務とし、呼び出し側は指定しない。`at` は `Clock` から取得する。

---

## 2. 履歴の表示

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 2.1 一覧

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/ui/render/history-text.ts` の `renderHistoryList(entries)` が 1 行 1 件を生成する。要約の生成は `src/core/history/summarize.ts` に置き、イベント種別ごとに同じ関数を使う（対話画面とコマンドで表記を共有する）。

### 2.2 詳細表示

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`history show <id>` はレコード全フィールドを整形して出力する。`read` レコードでは `read` ユースケースを再実行し、キャッシュ有効時は取得を行わない。

### 2.3 検索

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

絞り込みは `src/core/history/filter.ts` の `filterEvents(events, criteria)` で行う純粋関数とし、大文字小文字を区別しない部分一致で検索する。日付範囲は `at` の UTC 日付で比較する。

### 2.4 セッション単位の表示

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`--session <id>` 指定時は `filterEvents` にセッション条件を渡す。対話画面の「履歴を見る」は現行セッションのみを対象とする。

---

## 3. 再出力

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 3.1 節の再出力

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`read` レコードの `documentId` / `sectionId` から `SessionState` を介さずに文書キャッシュを引く（履歴はセッションをまたぐため）。出力ヘッダに「再出力（元: <日時>）」を追記する。取得失敗時は `E2001` と理由を返す。

### 3.2 検索の再実行

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`history show <id> --rerun` 相当の操作で `search` ユースケースを呼ぶ。セッション指定の有無で新規／継続を切り替える。

### 3.3 エクスポート

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

Markdown 出力は `src/ui/render/history-markdown.ts` が担う。`--with-body` 指定時のみ `body.md` を読み込んで含める。JSON Lines 出力は履歴ファイルの行をそのまま出力し、整形し直さない。

---

## 4. 保持と削除

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 4.1 保持期間

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

読み込み時に `retention_days` を過ぎたレコードを除外し、`purge` 操作時にファイルからも削除する。`0` は無期限として扱う。

### 4.2 上限

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

追記時に件数を確認し、上限を超える場合は追記後に末尾 `max_entries` 行で切り詰める（一時ファイル＋リネーム）。

### 4.3 削除

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

全削除は確認入力または `--yes` を必要とし、削除前に件数を表示する。削除後の空ファイルを作成し、`delete` イベントを 1 件だけ記録する。

### 4.4 記録の整合性

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

追記は `fs.appendFile` により行い、既存行を書き換えない。読み込み時の不正行スキップ件数は `W5004` として返し、`history list` の末尾に注記する。
