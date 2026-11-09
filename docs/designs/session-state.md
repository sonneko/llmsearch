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

`ui::render::status_text::session_line(&state)` が `session 20261107-142233-a7f3（継続: --session 20261107-142233-a7f3）` を返す。`--quiet` 時は呼ばない。

### 1.4 有効期間

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

`core::session::is_expired(updated_at, now, ttl_days)`。`session.maxSessions` 超過時の削除は `usecases::session::create` の末尾で行う。

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

`adapters::state::paths::resolve_state_dir(opts, env, settings) -> Result<PathBuf>`。優先順は `--state-dir` → `LLMSEARCH_STATE_DIR` → `settings.state.dir` → `dirs::state_dir()` → `E5002`。

### 3.2 ディレクトリ構成

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

`StatePaths` が `session_file`, `document_dir`, `history_file`, `counters_file`, `robots_cache_file`, `config_dir`, `engines_dir` を返す。パス文字列の連結はこの型に集約する。

### 3.3 権限

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

`std::fs::DirBuilder::mode(0o700)` でディレクトリを作成し、ファイルは `OpenOptions::mode(0o600)` で作成する（`cfg(unix)` の `PermissionsExt` を使用）。既存ファイルの権限は変更しない。

### 3.4 原子的な書き込み

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

`adapters::state::atomic::write(path, bytes)` が `path.tmp-<pid>-<rand>` へ書き、`fs::rename` で置換する。`rand` の代わりに `IdGen` を使い、決定論テストでは固定値を注入する。楽観的排他は `SessionState.revision: u64` の比較で行い、不一致は `E5003`。

---

## 4. 復元と破損時の扱い

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

### 4.1 復元

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

`--session` 再開時は `SessionState` をそのまま復元する。`documents` の各要素は `meta.json` を読み、`is_fresh` が偽なら `stale: true` を付けて返す（読み込みは遅延して行い、`read` 時に再取得する）。

### 4.2 破損

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

`fs::rename` で `.corrupt-<timestamp>` へ退避し、新規セッションを作成して `W5001` を返す。失われた項目は「検出できた範囲」のみ列挙する。

### 4.3 不整合

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

`documents` の参照先が存在しない場合は `body_ref: None` として扱う。識別子の欠番は詰めない（`next_result_id` は最大番号 +1）。

### 4.4 容量管理

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

`usecases::state::cleanup` が `documents/*/meta.json` の `fetched_at` 昇順で削除し、合計サイズが `cache.maxSizeMb` を下回るまで続ける。ディレクトリサイズは `walkdir` を追加せず、`fs::read_dir` の再帰で合計する。

---

## 5. 削除

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

### 5.1 セッションの削除

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

`usecases::session::cleanup` が対象一覧（ID・更新日時・サイズ）を返し、`ui` が確認を求める。`--yes` がある場合は確認を省略する。削除は `fs::remove_file` / `fs::remove_dir_all` をセッション単位で行う。

### 5.2 文書キャッシュの削除

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

`usecases::state::refcount` が他セッションと履歴の参照を数え、0 のものだけを削除する。走査は `BTreeSet<PathBuf>` に参照を集めてから差分を取る。

### 5.3 秘匿情報

状態: 実装完了（REF:src/adapters/state/mod.rs, REF:src/core/session.rs）

削除操作の履歴レコードは `delete` 種別で識別子と件数のみを記録する。URL を含める場合は `redact::url_with_secrets` を通す。
