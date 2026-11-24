# 設計書: セッションと状態管理

本書は `REF:docs/specifications/session-state.md` に対応する実装設計を記述する。

---

## 1. セッション識別子

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

### 1.1 形式

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

`core::session::id::new_session_id(clock: &dyn Clock, ids: &dyn IdGen) -> String` が `YYYYMMDD-HHmmss-xxxx` を組み立てる。時刻は `Clock::now()`（UTC）を `chrono::format` で整形し、末尾 4 桁は `IdGen::alnum(4)`（`[a-z0-9]`）。テストでは固定値を注入する。

### 1.2 生成と継続

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

`SessionStore::open(id: Option<&str>) -> Result<OpenedSession>`。`OpenedSession { state, created: bool, expired: bool }`。期限切れの場合は `state` を返さず `expired: true` を返し、呼び出し側が新規作成を提案する。

### 1.3 表示

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

`src/ui/interactive.rs` の `Interactive::run` が開始時に `session {id} (resume with --session {id})` を 1 行出す。`--quiet` 時は出さない。

### 1.4 有効期間

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

`session.ttlDays` と `session.maxSessions` は型と検証に取り込み、`config show` / `config explain` に値を出す。期限切れ判定と件数超過時の削除は行わない（削除操作も提供しない）。

---

## 2. 状態の単位

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

### 2.1 セッション状態

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

`SessionState` は仕様の項目に加え、`usage: Usage`（クォータ消費）、`quota_policy: String`、`engine: Option<String>` を持つ。`serde` の `#[serde(default)]` を全フィールドに付け、項目追加時に古いセッションを読めるようにする。未知フィールドは `extra` で温存する。

### 2.2 文書キャッシュ

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

`DocumentMeta` と `body.md` に分ける（`REF:docs/designs/document-fetching.md` §3.4）。見出し索引は `meta.json` に保存し、`read` 時に Markdown を再解析しない。

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

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

`--session` 再開時は `REF:src/adapters/state/session_store.rs` の `FsSessionStore::load` が `SessionState` をそのまま復元する。文書キャッシュの鮮度はセッションに持たせず、取得のたびに `load_cached` が `expires_at` で判定する（`stale` フラグは持たない）。

### 4.2 破損

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

`load` の JSON 解析に失敗したら `quarantine(path, stamp)` で `sessions/<id>.corrupt-<YYYYMMDDHHMMSS>.json` へ退避し、`E5001` を詳細（退避先を含む）付きで返す。`W5001` を出して新規セッションへ自動で切り替えることはしない。

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
