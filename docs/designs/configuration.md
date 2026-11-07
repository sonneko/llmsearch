# 設計書: 設定と構成（JSON）

本書は `REF:docs/specifications/configuration.md` に対応する実装設計を記述する。

---

## 1. 設定の全体像

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 1.1 構成要素

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/core/config/` に種別ごとの型を置く（`settings.rs`, `network.rs`, `filter.rs`, `quota.rs`）。読み込みは `src/adapters/config/loader.rs` が担当し、`serde_json` で `Value` → 型へ変換する。種別ごとのファイル名と探索ディレクトリは `src/adapters/config/layout.rs` に定数として集約する。

### 1.2 `$ref` による分割

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`resolve_refs(value, base_dir, visited)` が再帰的に `{"$ref": "..."}` を展開する。`visited` は正規化パスの `HashSet` で、再訪した場合は `E1003`（循環参照）。`http://` で始まる `$ref` は `E1003`（リモート参照の禁止）。展開後の `Value` を各型へデシリアライズする。

### 1.3 スキーマ版

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`SUPPORTED_SCHEMA_VERSION: u32 = 2`。`serde` の `#[serde(deny_unknown_fields)]` は使わず、`extra: Map<String, Value>` で未知キーを受ける（§6.2）。

---

## 2. 優先順位とマージ

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 2.1 優先順位

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`resolve_settings(layers: [Value; 5]) -> ResolvedSettings` が強い順に畳み込む。畳み込みは `Value` レベルで先に行い、最後に 1 回だけ型へ変換する（型変換の失敗箇所を 1 か所にまとめるため）。

### 2.2 マージ規則

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`merge(base: &mut Value, over: Value)` はオブジェクトを再帰的にマージし、配列とスカラーは置換する。`filter.*.rules` の追記は `mode: "append"` の場合に `merge_rules_append` で別扱いにする。`null` はそのままマージし、後段の検証で `E1004` とする。

### 2.3 探索順

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`locate_config(cli: Option<&Path>, env: &Env) -> Vec<PathBuf>` が候補を返す。存在しない候補も `config path` 用に保持する。`--config` 不在時は即座に `E1003`。

---

## 3. 全体設定（`settings.json`）

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 3.1 トップレベル

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`Settings` 構造体は仕様の表のキーを持つ。`network`, `filters`, `quota` は `RefOr<T>`（`Id(String)` / `Inline(T)`）の列挙で受け、`RefOr::Inline` の場合は追加ファイルを要求しない。

### 3.2 `search`

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

既定値は `impl Default for Settings` に集約し、`search.engine` のみ `Option<String>`（未設定時は同梱の `duckduckgo` を解決）。`limit` は 1〜30、`maxResults` は 1〜200 の検証属性を `validate()` で行う。

### 3.3 `context`

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`maxChars` は 500〜100000、`outlineDepth` は 2〜4。`minSectionChars` は描画時に `hidden_count` として件数のみ注記する（節そのものは保持し、`read` では指定可能とする）。

### 3.4 `cache`

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`ttlHours` から `expires_at` を計算するのは `core::cache::expiry(now, ttl_hours)`。`maxSizeMb` は `usecases::state::cleanup` が参照する。

### 3.5 `history`

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`enabled=false` のとき `HistoryStore` は `NullHistory` 実装に差し替える（呼び出し側の分岐を増やさない）。

### 3.6 `session`

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`maxSessions` 超過時の削除は `usecases::session::create` の末尾で行い、`updated_at` 昇順で削除する。

### 3.7 `ui`

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`color` の解決は `ui/render/color.rs` の `resolve_color(setting, env)` に集約し、`NO_COLOR` を最優先する。

### 3.8 `state`

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`state.dir` の解決は `adapters/state/paths.rs` の `resolve_state_dir`。優先順は `--state-dir` → `LLMSEARCH_STATE_DIR` → `state.dir` → `dirs::state_dir()/llmsearch`。

---

## 4. 環境変数

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/usecases/config/from_env.rs` が写像表を持ち、`Value` を組み立ててマージ層に渡す。真偽値は `1`/`true`/`0`/`false` のみ受理し、他は `E1004`。`LLMSEARCH_ENGINES_DIR` は `std::env::split_paths` で分解する。

---

## 5. キャッシュ

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 5.1 キャッシュ対象

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

検索結果キャッシュのキーは `format!("{engine}/{hash}/{query_hash}/{position}/{size}")` を SHA-256 で短縮したディレクトリ名。文書キャッシュは `adapters/state/paths.rs` の `document_dir(url)`（URL の SHA-256 先頭 16 文字）。

### 5.2 有効期限と無効化

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

再利用判定は `core::cache::is_fresh(meta, now)` の純粋関数。`--refresh` は `is_fresh` を呼ばずに取得へ回す。

### 5.3 検索結果をキャッシュする範囲

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`cache.searchResults`（仕様の既定は `false`）を `Settings` に `#[serde(default)]` で追加し、`usecases::search::run` が真の場合のみ読み書きする。

---

## 6. 検証と未知キー

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 6.1 値の検証

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`Settings::validate() -> Vec<Diagnostic>` が全項目を検査し、`fail` が 1 件でもあれば起動を中止する。キーのパスは手書きの文字列リテラルではなく、検証関数の引数として渡す（`("context.maxChars", value, range)`）。

### 6.2 未知キーの温存

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

各構造体は `#[serde(flatten)] extra: serde_json::Map<String, Value>` を持ち、`extra` が空でない場合に `W1002` を出す。書き戻し（`config init --force`）では `extra` を元の階層へ戻す。

### 6.3 参照の解決

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`resolve_refs_for_id`（ネットワーク・フィルタ・クォータ共通）は探索ディレクトリを順に走査し、`<id>.json` を探す。見つからない場合は候補一覧を `details` に付して `E1003`。

### 6.4 破損した JSON

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`serde_json::from_str` のエラーから `line` と `column` を取り出し、`E1003` のメッセージへ含める。コメント付き JSON は受理しない（`serde_json` の既定動作のまま）。

---

## 7. 表示と初期化

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 7.1 `config show`

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

由来は `Origin`（`Default` / `Profile` / `File` / `Env` / `Argv`）として保持し、`src/ui/render/config_text.rs` が「キー / 値 / 由来」を 3 列で描画する。`secrets` に解決済みの値は表示しない。

### 7.2 `config path`

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`ConfigPaths { settings, network_dir, filters_dir, quotas_dir, engines_dirs, state_dir }` を `src/ui/render/paths_text.rs` が 1 行 1 件で表示する。

### 7.3 `config init`

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

雛形は `src/adapters/config/templates.rs` に `&'static str` として置き、`include_str!` で `assets/config/*.json` を埋め込む。既存ファイルは `--force` が無ければ `E1004`。

### 7.4 `config validate`

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

設定 4 種とエンジン定義の検証を順に実行し、`Vec<Diagnostic>` を `src/ui/render/diagnostic_text.rs` で `[ok]/[warn]/[fail]` に整形する。

### 7.5 `config explain <key>`

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`ResolvedSettings` が保持する `origins: BTreeMap<String, Origin>` からキーを引き、`src/ui/render/explain_text.rs` が 1 行で表示する。キーが存在しない場合は `E1004` と近いキー名の候補を返す。
