# 設計書: 閲覧履歴

本書は `REF:docs/specifications/history.md` に対応する実装設計を記述する。

---

## 1. 記録対象

状態: 実装完了（REF:src/core/history.rs, REF:src/adapters/state/history_jsonl.rs）

### 1.1 記録するイベント

状態: 実装完了（REF:src/core/history.rs）

`src/core/history.rs` に `#[serde(tag = "type", rename_all = "snake_case")] enum HistoryEvent` を置き、種別ごとに `payload` の型を固定する。記録は各ユースケースの末尾で `HistoryStore::append(event)` を 1 回だけ呼ぶ（複数箇所から呼ばない）。

### 1.2 記録しない情報

状態: 実装完了（REF:src/core/history.rs）

保存直前の `src/core/history.rs::redact(&mut event)` で、URL の秘匿パラメータを伏字化し、`payload` に本文が含まれていないことを型で担保する（本文フィールドを持たせない）。秘匿パラメータ名は定数配列。

### 1.3 記録の単位

状態: 実装完了（REF:src/core/history.rs, REF:src/adapters/state/history_jsonl.rs）

`id` は保存ファイル内の最大番号 +1 を `H<n>` として `HistoryStore` が付与する。`at` は `Clock` から取得し `%Y-%m-%dT%H:%M:%SZ` で整形する。`cost` / `remaining` は `Usage` / `Vec<MetricUsage>` の `Copy` 型。

---

## 2. 履歴の表示

状態: 実装完了（REF:src/core/history.rs, REF:src/adapters/state/history_jsonl.rs）

### 2.1 一覧

状態: 実装完了（REF:src/core/history.rs, REF:src/ui/render.rs）

`src/ui/render.rs` の `Renderer::history_line(entry)` が 1 行 1 件を生成する。要約は `src/core/history.rs` の `HistoryEntry::summary()` に集約し、対話画面とコマンドで共有する。並び順の反転と件数の切り詰めは `REF:src/adapters/state/history_jsonl.rs` の `query` が行う。

### 2.2 詳細表示

状態: 実装完了（REF:src/core/history.rs, REF:src/adapters/state/history_jsonl.rs）

`history show <id>` はレコード全フィールドを整形して出力する。`REF:src/usecases/history.rs` の `get` が `next_id` を使わず全件から識別子で引く。本文の再出力は行わない（§3.1）。

### 2.3 検索

状態: 実装完了（REF:src/core/history.rs）

`src/core/history.rs` の `Filter::matches(entry)` は純粋な判定関数で、`HistoryQuery` を写して使う。キーワードは `summary` / `payload` / `engine` を連結した文字列への部分一致（大文字小文字を区別しない）、日付は `at` の文字列比較、種別は `EventType::parse` が解釈できた最初の 1 件だけを使う。

### 2.4 セッション単位の表示

状態: 実装完了（REF:src/core/history.rs, REF:src/adapters/state/history_jsonl.rs）

`HistoryQuery { session: Some(..), .. }` を `query` に渡す。対話画面は現行セッション ID を固定で渡す。

---

## 3. 再出力と再実行

状態: 実装完了（REF:src/core/history.rs, REF:src/adapters/state/history_jsonl.rs）

### 3.1 再出力・再実行の位置づけ

状態: 未実装・未計画（`plans.md` §7 に記載）

`history show` は表示のみを行い、`--rerun` や本文の再出力は提供しない（`REF:src/ui/commands.rs` の `run_history`。`REF:tests/m9.rs` で `--previous` と併せて未提供を確認している）。節の再出力は `read` レコードの `payload.document` / `payload.section` をそのまま `read` に渡す経路で代替する。

### 3.2 エクスポート

状態: 実装完了（REF:src/ui/render.rs）

Markdown は `src/core/history.rs` の `export_markdown(&[HistoryEntry])` が組み立てる（`REF:src/usecases/history.rs` の `export` が新しい順の一覧を古い順へ直してから渡す）。JSON Lines は各エントリを `serde_json::to_string` で 1 行ずつ出す。`--json` のときは形式にかかわらず `history` イベント（`entries[]` / `total`）へ包み、生テキストを標準出力へ出さない（`REF:src/ui/commands.rs` の `run_history`）。

---

## 4. 保持と削除

状態: 実装完了（REF:src/core/history.rs, REF:src/adapters/state/history_jsonl.rs）

### 4.1 保持期間

状態: 実装完了（REF:src/core/history.rs, REF:src/adapters/state/history_jsonl.rs）

読み込み時に `retentionDays` を過ぎたレコードを除外し、`purge` 操作時にファイルからも削除する。`0` は無期限。

### 4.2 上限

状態: 実装完了（REF:src/core/history.rs, REF:src/adapters/state/history_jsonl.rs）

追記後に件数を確認し、超過時は末尾 `maxEntries` 行を残した新ファイルを作成して `rename` する。

### 4.3 削除

状態: 実装完了（REF:src/core/history.rs, REF:src/adapters/state/history_jsonl.rs）

全削除は確認入力または `--yes` を要し、削除前に件数を表示する。削除後に空ファイルを作成し、`delete` イベントを 1 件だけ記録する。

### 4.4 記録の整合性

状態: 実装完了（REF:src/core/history.rs, REF:src/adapters/state/history_jsonl.rs）

追記は `OpenOptions::append(true)` で行い、既存行を書き換えない。読み込み時に不正行は読み飛ばして一覧を返す（警告は出さない）。
