# 設計書: セッションと状態管理

本書は `REF:docs/specifications/session-state.md` に対応する実装設計を記述する。

---

## 1. セッション識別子

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

### 1.1 形式

状態: 実装完了（REF:src/ports/system.rs, REF:src/core/session.rs）

`ports::IdGen::session_id(&self, now_utc: &str) -> String`（既定実装は `REF:src/ports/system.rs` の `SystemIdGen`）が `YYYYMMDD-HHMMSS-xxxx` を組み立てる。テストでは `SeqIds` の固定値を注入する。

### 1.2 生成と継続

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

`App::select_session(id)` は `SessionStore::load(id)` で既存状態を確認し、見つからなければ `E5001` を返して新規作成しない。`App::ensure_session()` は ID を割り当てるだけで、新規 `search` の `apply_to_session` が状態ファイルを保存する。`App::resume_latest_session()` は空の現行 ID の場合に `counters.json` の `session_key` を読み、対応するセッションがあれば再開する。`App::require_existing_session()` は現行または直近の既存セッションを要求し、`next` / `refine` / 識別子を使う `read` / `outline` が利用する。

### 1.3 表示

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

`src/ui/interactive.rs` の `Interactive::run` が開始時に `session {id} (resume with --session {id})` を 1 行出す。非対話 `search` は `commands.rs` が案内を標準エラーへ出し、暗黙に再開した `next` / `refine` / `quota show` / `quota check` も再開 ID を通知する。`--quiet` 時は通知しない。JSON イベントには `session` フィールドで ID を含める。

### 1.4 有効期間

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

`session.ttlDays` と `session.maxSessions` は型と検証に取り込み、`config show` / `config explain` に値を出す。期限切れ判定と件数超過時の削除は行わない（削除操作も提供しない）。

---

## 2. 状態の単位

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

### 2.1 セッション状態

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

`SessionState` は `schema_version` / `session_id` / 作成・更新時刻 / `queries` / `active_query` / `results` / `page` / `documents` / `outline_depth` / `selections` / `pending_selections` / `last_query` / `last_engine` を持つ。`QueryRecord` は検索語・エンジン・時刻・結果件数・除外件数を保持する。`active_query` は `queries` の現在位置だが、結果一覧はクエリ別のスロットではなく `SessionState.results` 1 つであり、新しい検索・`refine` で置き換える。`PageState` は現在ページ・次ページ・終端フラグを持つ。クォータ消費量はセッション JSON ではなく `counters.json` の `session_key` 付きカウンタに保存する。全フィールドに既定値を適用して古いセッションを読み込む。

### 2.2 文書キャッシュ

状態: 実装完了（REF:src/adapters/state/session_store.rs, REF:src/core/model.rs）

`DocumentMeta`（`meta.json`）と `body.md` に分ける（`REF:docs/designs/document-fetching.md` §3.4）。`meta.json` の `headings` は見出し件数（`usize`）を保持し、`outline` / `read` 時は `body.md` から見出しを再抽出する。

### 2.3 履歴

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

`adapters::state::history_jsonl` が 1 行 1 JSON で追記する。読み込みは `BufReader::lines()` で行い、`serde_json` の失敗行をスキップして件数を返す。上限超過時の削除は末尾 N 行を残した新ファイルの作成と `rename` で行う。

---

## 3. 保存先

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

### 3.1 探索順

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

`REF:src/adapters/config/mod.rs` の `state_dir_with(env, explicit, configured) -> Option<PathBuf>` が解決する。優先順は `--state-dir` → `LLMSEARCH_STATE_DIR` → 設定 `state.dir` → `env.state_home()`（`XDG_STATE_HOME` または `dirs::state_dir()`、無ければ `~/.local/state`）+ `llmsearch`。すべて外れた場合は `E5002`。

### 3.2 ディレクトリ構成

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

`StatePaths` が `sessions()`, `documents()`, `document_dir(url)`, `history_file()`, `counters_file()`, `robots_file()` を返す。パス文字列の連結はこの型に集約し、`ensure()` が `sessions` / `documents` / `history` / `cache` を権限 0700 で作る。`settings.json` は状態ディレクトリ直下に置き、`network/` / `filters/` / `quotas/` は `config init` が書き出す。

### 3.3 権限

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

`StatePaths::ensure()` が `std::fs::create_dir_all` の後に `set_permissions(0o700)` でディレクトリを絞る（`cfg(unix)`）。ファイルは `write_atomic` が一時ファイルに `0o600` を設定してから `rename` する。既存ファイルの権限は変更しない。

### 3.4 原子的な書き込み

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

`REF:src/adapters/state/mod.rs` の `write_atomic(path, bytes)` が `path.tmp<pid>` へ書いて `fs::rename` で置換し、失敗時は一時ファイルを消す。`revision` による楽観的排他は実装していない（`E5003` は定義のみで、発行箇所は無い）。

---

## 4. 復元と破損時の扱い

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

### 4.1 復元

状態: 実装完了（REF:src/adapters/state/session_store.rs, REF:src/usecases/document.rs）

`--session` 再開時は `REF:src/adapters/state/session_store.rs` の `FsSessionStore::load` が `SessionState` をそのまま復元する（文書キャッシュ取得時の `expires_at` 失効判定漏れは Issue #60 で追跡）。

### 4.2 破損

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/adapters/state/session_store.rs）

`load` の JSON 解析に失敗したら `adapters::state::quarantine(path, stamp)` で退避し、`E5001` を詳細（退避先を含む）付きで返す（退避ファイル名末尾に `.json` が付かない点は Issue #62 で追跡）。警告は出さず、新規セッションへ自動で切り替えることもしない。

### 4.3 不整合

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

`documents` の参照先が存在しない場合は、その文書を未取得として扱い再取得を案内する。識別子の欠番は詰めない（新しい識別子は既存の最大番号 +1）。

### 4.4 容量管理

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

`REF:src/adapters/state/session_store.rs` の `FsDocumentStore::evict_to_limit()` が `documents/*/meta.json` の `fetched_at` 昇順で削除し、`StatePaths::total_bytes()` が `cache.maxSizeMb` 以下になるまで続ける。`total_bytes` は `fs::read_dir` の再帰で合計する（状態ディレクトリ全体が対象）。`put` の後に呼ぶため、上限超過時の `W5002`（`REF:src/usecases/document.rs`）と併せて通知される。

---

## 5. 削除

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

### 5.1 セッションの削除

状態: 実装完了（REF:src/adapters/state/mod.rs）

セッションを削除するコマンドは持たない。`FsSessionStore::delete(id)` はポートの実装としてあるが、CLI からは呼ばない（期限切れの自動削除もしない）。

### 5.2 文書キャッシュの削除

状態: 実装完了（REF:src/adapters/state/session_store.rs, REF:src/usecases/document.rs）

`FsDocumentStore::evict_to_limit()` が `cache.maxSizeMb` を上限として `fetched_at` の古い順に削除する。`history clear` は履歴ファイルだけを置き換え、文書キャッシュには触れない。

### 5.3 秘匿情報

状態: 実装完了（REF:src/usecases/history.rs, REF:src/core/history.rs）

削除操作の履歴レコードは `delete` 種別で識別子と件数のみを記録する（`history clear` は `target` / `count`）。本文と URL は記録しない。
