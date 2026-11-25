# 設計書: 閲覧履歴

本書は `REF:docs/specifications/history.md` に対応する実装設計を記述する。

---

## 1. 記録対象

状態: 実装完了（REF:src/core/history.rs, REF:src/adapters/state/history_jsonl.rs）

### 1.1 記録するイベント

状態: 実装完了（REF:src/core/history.rs）

`src/core/history.rs` に `EventType` と `HistoryEntry`（`payload: serde_json::Value`）を置く。`search` / `next` は実際に使ったエンジンをトップレベルの `engine` に設定する。`JsonlHistory::read_all` は旧レコードのトップレベル `engine` が無い場合に文字列の `payload.engine` から補完し、一覧・JSON・検索・エクスポートで同じ値を使う。クォータで送信前に拒否された `next` も `added: 0` と `terminationReason: "quota"` の履歴を追記する。

### 1.2 記録しない情報

状態: 実装完了（REF:src/core/history.rs）

`HistoryEntry.payload` は汎用 `serde_json::Value` で、本文を型で禁止する仕組みや append 時の一括 redaction はない。各記録呼び出し側が本文を payload に入れず、URL を保存する場合は `core::url::redact` などで秘匿パラメータを伏せる（例: `usecases::document`）。履歴への新規フィールド追加時は、本文・認証情報を含めないことを呼び出し側で守る。

### 1.3 記録の単位

状態: 実装完了（REF:src/core/history.rs, REF:src/adapters/state/history_jsonl.rs）

呼び出し側は `HistoryStore::next_id()` で保存済み ID の最大番号 +1（`H<n>`）を取得し、`HistoryEntry::new()` で `at` を `Clock` から UTC 文字列として設定した後、`append()` する。`cost` / `remaining` は任意の `Cost`（4 種のカウンタ値）であり、現在は `quota reset` がリセット前後の値を設定する。検索・次ページの個別情報は汎用 `payload: serde_json::Value` に記録し、`search` / `next` はトップレベル `engine` も設定する。

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

`JsonlHistory::append` の後で `prune()` が `retentionDays` を過ぎたレコードと `maxEntries` 超過分をファイルから取り除く。`0` 日は無期限。`query` は読み込み時に保持期間を適用せず、`history clear` は全件を削除する。

### 4.2 上限

状態: 実装完了（REF:src/core/history.rs, REF:src/adapters/state/history_jsonl.rs）

追記後に件数を確認し、超過時は末尾 `maxEntries` 行を残した新ファイルを作成して `rename` する。

### 4.3 削除

状態: 実装完了（REF:src/core/history.rs, REF:src/adapters/state/history_jsonl.rs）

全削除は確認入力または `--yes` を要し、削除前に件数を表示する。削除後に空ファイルを作成し、`delete` イベントを 1 件だけ記録する。

### 4.4 記録の整合性

状態: 実装完了（REF:src/core/history.rs, REF:src/adapters/state/history_jsonl.rs）

追記は `OpenOptions::append(true)` で行い、既存行を書き換えない。読み込み時に不正行は読み飛ばして一覧を返す（警告は出さない）。
