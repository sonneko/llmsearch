# 設計書: セッションと状態管理

本書は `REF:docs/specifications/session-state.md` に対応する実装設計を記述する。パスは実装予定の配置である。

---

## 1. セッション識別子

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 1.1 形式

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

生成は `src/core/session/id.ts` の `newSessionId(clock, ids)` で行う。時刻部分は `Clock` の UTC 時刻を `YYYYMMDD-HHmmss` に整形し、末尾 4 桁は `IdGenerator` の乱数（`[a-z0-9]{4}`）とする。テストでは両ポートを固定値にして文字列を検証する。

### 1.2 生成と継続

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`SessionStore.open(id?)` は、ID 指定かつ存在すれば読み込み、存在しなければ新規作成して `created: true` を返す。作成時の通知は呼び出し側（コマンド／対話画面）が `Reporter.info` で行う。

### 1.3 表示

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

対話開始時に `session <id>（継続する場合: --session <id>）` を 1 行表示する。`--quiet` では出力しない。

### 1.4 有効期間

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`updatedAt` と `session.ttl_days` から期限を判定する。期限切れセッションは `open()` で `E5001` 相当の「期限切れ」を返し、新規作成を提案する。削除は `session.cleanup` 操作で行う。

---

## 2. 状態の単位

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 2.1 セッション状態

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`SessionState`（`src/core/session/state.ts`）は仕様の表の項目をそのままフィールドとして持つ。読み書きは `zod` スキーマ（`src/core/session/schema.ts`）で検証し、未知フィールドは保持する（`passthrough`）。

### 2.2 文書キャッシュ

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`DocumentMeta`（`meta.json`）と `body.md` に分ける。見出し索引は `meta.json` に保存し、`read` 時に Markdown を再解析しない。

### 2.3 履歴

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`HistoryStore` ポートの実装は `src/adapters/state/history-jsonl.ts`。1 行 1 JSON の追記専用ファイルとし、読み込み時に不正行をスキップして件数を返す。上限超過時の削除は、末尾 N 行を残した新ファイルの作成とリネームで行う。

---

## 3. 保存先

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 3.1 探索順

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/adapters/state/paths.ts` の `resolveStateDir(options, env, config)` が優先順位を実装する。`XDG_STATE_HOME` の未設定時は `~/.local/state`、ホームディレクトリも取得できない場合は `E5002` を返す。

### 3.2 ディレクトリ構成

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`StatePaths` が各パスを組み立てる（`sessionFile(id)`, `documentDir(url)`, `historyFile`, `robotsCacheFile`, `configFile`）。パスの文字列連結はこのクラスに集約し、他層でパスを組み立てない。

### 3.3 権限

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

ディレクトリ作成時に `mode: 0o700`、ファイル書き込み時に `mode: 0o600` を指定する。既存ファイルの権限は変更しない（利用者の管理を尊重する）。

### 3.4 原子的な書き込み

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`atomicWrite(path, data)`（`src/adapters/state/atomic.ts`）が `path.tmp-<pid>-<rand>` へ書き、`fs.rename` で置き換える。セッション更新時の衝突検出は、読み込み時の `revision` を書き戻し時に比較する楽観的排他で行い、不一致なら `E5003`。

---

## 4. 復元と破損時の扱い

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 4.1 復元

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`--session` 再開時は `SessionState` をそのまま復元する。文書参照のうち `expiresAt` を過ぎたものは `stale` フラグを立てて返し、取得ユースケースが必要に応じて再取得する。

### 4.2 破損

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`JSON.parse` 失敗またはスキーマ検証失敗時は、`fs.rename` で `<file>.corrupt-<timestamp>` へ退避し、新規セッションを作成して `W5001` を返す。失われた項目は「検出できた範囲」のみ列挙し、推測で補完しない。

### 4.3 不整合

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`documents` の参照先が存在しない場合は `bodyRef: null` として扱い、`read` 時に再取得を試みる。識別子の欠番は詰めない（`nextResultId` は最大番号 +1）。

### 4.4 容量管理

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/usecases/state/cleanup.ts` が `documents/` 配下の `meta.json` の `fetchedAt` 昇順で削除し、合計サイズが `cache.max_size_mb` を下回るまで続ける。削除件数を `W5002` として返す。

---

## 5. 削除

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 5.1 セッションの削除

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

削除対象の一覧（ID、更新日時、サイズ）を先に表示し、確認入力を待つ。`--yes` がある場合は確認を省略する。削除は `fs.rm(dir, { recursive: true })` をセッションファイル単位で行う。

### 5.2 文書キャッシュの削除

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

履歴削除時の連動削除は、他セッションの `documents` と履歴の `fetch` レコードを走査して参照カウント 0 のものだけを削除する。走査は `src/usecases/state/refcount.ts` に集約する。

### 5.3 秘匿情報

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

削除操作の履歴レコードには `delete` 種別で識別子と件数のみを記録する。本文・URL のクエリは記録しない。
