# 設計書: 設定と構成（JSON）

本書は `REF:docs/specifications/configuration.md` に対応する実装設計を記述する。

---

## 1. 設定の全体像

状態: 実装完了（REF:src/core/config/settings.rs, REF:src/adapters/config/mod.rs）

### 1.1 構成要素

状態: 実装完了（REF:src/adapters/config/mod.rs）

`src/core/config/` に種別ごとの型を置く（`settings.rs`, `network.rs`, `filter.rs`, `quota.rs`）。読み込みは `src/adapters/config/mod.rs` が担当し、`serde_json` で `Value` → 型へ変換する。種別ごとのファイル名と探索ディレクトリは `src/adapters/config/mod.rs` に定数として集約する。

同梱アセットの既定値は `src/assets.rs` の `include_str!` 群として 1 か所に置く。`assets::Resource`（`Network` / `Filters` / `Quota` / `Engines`）が `dir()` と `embedded()` を持ち、`embedded()` は `(id, JSON 本文)` を返す。エンジン定義だけはファイル名ではなく定義の `id@variant` をキーにする。ファイル探索（バイナリの隣の `assets/` → 作業ディレクトリから上方探索）で見つからない ID についてだけ埋め込みへ落とす。優先順位は「利用者ファイル > 同梱ファイル > 埋め込み」であり、埋め込みはファイルが 1 つも無い配布形態でも起動できるようにするための最後の砦である。

同梱アセットの既定値は `src/assets.rs` の `include_str!` 群として 1 か所に置く。`assets::Resource`（`Network` / `Filters` / `Quota` / `Engines`）が `dir()` と `embedded()` を持ち、`embedded()` は `(id, JSON 本文)` を返す。エンジン定義だけはファイル名ではなく定義の `id@variant` をキーにする。ファイル探索（バイナリの隣の `assets/` → 作業ディレクトリから上方探索）で見つからない ID についてだけ埋め込みへ落とす。優先順位は「利用者ファイル > 同梱ファイル > 埋め込み」であり、埋め込みはファイルが 1 つも無い配布形態でも起動できるようにするための最後の砦である。

### 1.2 `$ref` による分割

状態: 実装完了（REF:src/core/config/settings.rs, REF:src/adapters/config/mod.rs）

`resolve_refs(value, base_dir, visited)` が再帰的に `{"$ref": "..."}` を展開する。`visited` は正規化パスの `HashSet` で、再訪した場合は `E1003`（循環参照）。`http://` で始まる `$ref` は `E1003`（リモート参照の禁止）。展開後の `Value` を各型へデシリアライズする。

### 1.3 スキーマ版

状態: 実装完了（REF:src/core/config/settings.rs, REF:src/adapters/config/mod.rs）

`SUPPORTED_SCHEMA_VERSION: u32 = 2`。`serde` の `#[serde(deny_unknown_fields)]` は使わず、`extra: Map<String, Value>` で未知キーを受ける（§6.2）。

---

## 2. 優先順位とマージ

状態: 実装完了（REF:src/core/config/settings.rs, REF:src/adapters/config/mod.rs）

### 2.1 優先順位

状態: 実装完了（REF:src/core/config/settings.rs, REF:src/adapters/config/mod.rs）

`resolve_settings(layers: [Value; 5]) -> ResolvedSettings` が強い順に畳み込む。畳み込みは `Value` レベルで先に行い、最後に 1 回だけ型へ変換する（型変換の失敗箇所を 1 か所にまとめるため）。

### 2.2 マージ規則

状態: 実装完了（REF:src/core/config/settings.rs, REF:src/adapters/config/mod.rs）

`merge(base: &mut Value, over: Value)` はオブジェクトを再帰的にマージし、配列とスカラーは置換する。`filter.*.rules` の追記は `mode: "append"` の場合に `merge_rules_append` で別扱いにする。`null` はそのままマージし、後段の検証で `E1004` とする。

### 2.3 探索順

状態: 実装完了（REF:src/adapters/config/mod.rs）

`candidate_paths(env, options) -> Vec<PathBuf>` が弱い順（後ろほど優先）に候補を返す。順序は「利用者設定ディレクトリの `settings.json`」→「状態ディレクトリの `settings.json`」で、`LLMSEARCH_CONFIG` があればその 1 件だけ、`--config` があればその 1 件だけを返す。`--config` / `LLMSEARCH_CONFIG` の指定先が存在しない場合は既存の候補へ落とさず `E1003` を返す。存在しない候補も `config path` 用に保持する。

状態ディレクトリのパスは `state_dir(env, explicit)`（`--state-dir` → `LLMSEARCH_STATE_DIR` → `state.home`）で解決し、`settings.json` はその直下に置く（`config/settings.json` ではない）。`network/` などの資源ディレクトリも同じ基準（`<state.dir>/network/`）とする。`LoadedSettings` は読み込んだ `sources` を優先順（弱い→強い）で保持し、`config show` の由来表示に使う。

---

## 3. 全体設定（`settings.json`）

状態: 実装完了（REF:src/core/config/settings.rs, REF:src/adapters/config/mod.rs）

### 3.1 トップレベル

状態: 実装完了（REF:src/core/config/settings.rs, REF:src/adapters/config/mod.rs）

`Settings` 構造体は仕様の表のキーを持つ。`network`, `filters`, `quota` は `RefOr<T>`（`Id(String)` / `Inline(T)`）の列挙で受け、`RefOr::Inline` の場合は追加ファイルを要求しない。

### 3.2 `search`

状態: 実装完了（REF:src/core/config/settings.rs, REF:src/adapters/config/mod.rs）

既定値は `impl Default for Settings` に集約し、`search.engine` のみ `Option<String>`（未設定時は同梱の `duckduckgo` を解決）。`limit` は 1〜30、`maxResults` は 1〜200 の検証属性を `validate()` で行う。

`search.language`（既定 `en`）は `src/core/config/settings.rs` の `SearchSettings.language` に持ち、`Settings::validate()` が言語タグの形を検査する（空・不正は `E1004`）。`--language <tag>` は `Bootstrap.language` → `settings.search.language` の順で重ね、`LLMSEARCH_SEARCH_LANGUAGE` は `ENV_MAPPINGS` の `Kind::Text` として `search.language` へ写す。由来は `config show` の `origin_of` に `--language` / `search.language` の枝を足して表示する。

正規化は `src/core/lang.rs` の `normalize_language(tag) -> Option<Language>` に集約する。`Language { tag, primary }` を返し、`tag` は小文字化した全体（`pt-BR` → `pt-br`）、`primary` は主言語タグ（`pt`）である。受理する形は `<主言語>[-<サブタグ>...]`、主言語は英字 2〜3 文字、サブタグは英数字 1〜8 文字。空・記号混入・4 文字以上の主言語は `None` とし、`Settings::validate()` と `parse_env` の双方で `E1004` にする。

消費側は 1 か所に集約する。`src/usecases/search.rs` の `call_engine` と `src/usecases/engines.rs` の `dry_run` が `TmplCtx { language: primary, locale: tag, .. }` を組み立てる。`{{language}}` は主言語タグ、`{{locale}}` は正規化後の全体（旧定義との互換のため残す）。`Accept-Language` の補完は `src/adapters/engine/eval.rs` で `prepared.headers` に `Accept-Language` が無い場合だけ `tag` を挿入する（ネットワークプロファイルからの `Accept-Language` 上書きは廃止し、`src/adapters/http.rs` の `profile.accept_language` 分岐と `NetworkProfile.accept_language` フィールドを削除する）。

DuckDuckGo の `kl` は地域と言語の固定対（`jp-jp` / `us-en`）を列挙した値で、言語だけを表す形が無い。同梱定義から `kl` の直書きを削除し、言語は `Accept-Language` で届ける（`REF:docs/specifications/engine-definitions.md` §3.2）。地域を寄せたい利用者は自前の定義で `{{region}}` を使って `kl` を書ける。Brave は `search_lang={{language}}`、SearXNG は `language={{locale}}` を使う。

### 3.3 `context`

状態: 実装完了（REF:src/core/config/settings.rs, REF:src/adapters/config/mod.rs）

`maxChars` は 500〜100000、`outlineDepth` は 2〜4。`minSectionChars` は描画時に `hidden_count` として件数のみ注記する（節そのものは保持し、`read` では指定可能とする）。

### 3.4 `cache`

状態: 実装完了（REF:src/core/config/settings.rs, REF:src/adapters/config/mod.rs）

`ttlHours` から `expires_at` を計算するのは `core::cache::expiry(now, ttl_hours)`。`maxSizeMb` は `usecases::state::cleanup` が参照する。

### 3.5 `history`

状態: 実装完了（REF:src/core/config/settings.rs, REF:src/adapters/config/mod.rs）

`enabled=false` のとき `HistoryStore` は `NullHistory` 実装に差し替える（呼び出し側の分岐を増やさない）。

### 3.6 `session`

状態: 実装完了（REF:src/core/config/settings.rs, REF:src/adapters/config/mod.rs）

`maxSessions` 超過時の削除は `usecases::session::create` の末尾で行い、`updated_at` 昇順で削除する。

### 3.7 `ui`

状態: 実装完了（REF:src/core/config/settings.rs, REF:src/adapters/config/mod.rs）

`color` の解決は `ui/render/color.rs` の `resolve_color(setting, env)` に集約し、`NO_COLOR` を最優先する。

### 3.8 `state`

状態: 実装完了（REF:src/core/config/settings.rs, REF:src/adapters/config/mod.rs）

`state.dir` の解決は `adapters/state/paths.rs` の `resolve_state_dir`。優先順は `--state-dir` → `LLMSEARCH_STATE_DIR` → `state.dir` → `dirs::state_dir()/llmsearch`。

---

## 4. 環境変数

状態: 実装完了（REF:src/usecases/config.rs, REF:src/ui/cli.rs, REF:src/adapters/config/mod.rs）

`src/usecases/config.rs` の `env_overrides(env)` が 1 つの写像表（`(変数名, キー, 種別)`）を持ち、`Value` を組み立ててマージ層へ渡す。由来の表示（`config show` / `config explain`）も同じ表の `env_keys()` を参照し、表の二重管理を避ける。真偽値は `1`/`true`/`0`/`false` のみ受理する。数値・真偽値として解釈できない値は、その変数を黙って捨てずに `E1004`（キー・指定値・許容範囲）として返す。

設定ファイルへ落とせない変数は経路を分ける。

- `LLMSEARCH_CONFIG`: `adapters::config::candidate_paths` が探索順 2 番目として扱う。
- `LLMSEARCH_STATE_DIR`: `adapters::config::state_dir` が解決する。
- `LLMSEARCH_ENGINE` / `LLMSEARCH_NETWORK` / `LLMSEARCH_FILTERS` / `LLMSEARCH_QUOTA`: 参照 ID の解決（`resolve_network` / `resolve_filters` / `resolve_quota` と `usecases::config::build` の既定エンジン解決）で引く。
- `LLMSEARCH_ENGINES_DIR`: `std::env::split_paths`（`Env` 経由）で分解し、`--engines-dir` の後ろへ連結する。
- `LLMSEARCH_NO_CACHE`: `cache.enabled` の逆として `env_overrides` に入れる。`--no-cache` も同じキーを上書きする。
- `LLMSEARCH_TIMEOUT_MS` / `LLMSEARCH_USER_AGENT`: 設定 `network` がインライン定義のときだけ `settings.network` のインライン値へ重ねる（ID 参照のときは解決後の `NetworkProfile` へ上書きする）。空の `LLMSEARCH_USER_AGENT` は `E1004`。
- `NO_COLOR` / `LLMSEARCH_UI_COLOR`: `ui.color` へ写す。`NO_COLOR` が最優先。
- `LLMSEARCH_SEARCH_LANGUAGE`: `search.language` へ写す（`Kind::Text`）。不正な言語タグは `E1004`。
- `LLMSEARCH_UI_LOCALE`: 廃止。表から削除し、設定されていても参照しない。

---

## 5. キャッシュ

状態: 実装完了（REF:src/core/config/settings.rs, REF:src/adapters/config/mod.rs）

### 5.1 キャッシュ対象

状態: 実装完了（REF:src/core/config/settings.rs, REF:src/adapters/config/mod.rs）

検索結果キャッシュのキーは `format!("{engine}/{hash}/{query_hash}/{position}/{size}")` を SHA-256 で短縮したディレクトリ名。文書キャッシュは `adapters/state/paths.rs` の `document_dir(url)`（URL の SHA-256 先頭 16 文字）。

### 5.2 有効期限と無効化

状態: 実装完了（REF:src/core/config/settings.rs, REF:src/adapters/config/mod.rs）

再利用判定は `core::cache::is_fresh(meta, now)` の純粋関数。`--refresh` は `is_fresh` を呼ばずに取得へ回す。

### 5.3 検索結果をキャッシュする範囲

状態: 実装完了（REF:src/core/config/settings.rs, REF:src/adapters/config/mod.rs）

`cache.searchResults`（仕様の既定は `false`）を `Settings` に `#[serde(default)]` で追加し、`usecases::search::run` が真の場合のみ読み書きする。

---

## 6. 検証と未知キー

状態: 実装完了（REF:src/core/config/settings.rs, REF:src/adapters/config/mod.rs）

### 6.1 値の検証

状態: 実装完了（REF:src/core/config/settings.rs, REF:src/adapters/config/mod.rs）

`Settings::validate() -> Vec<Diagnostic>` が全項目を検査し、`fail` が 1 件でもあれば起動を中止する。キーのパスは手書きの文字列リテラルではなく、検証関数の引数として渡す（`("context.maxChars", value, range)`）。

### 6.2 未知キーの温存

状態: 実装完了（REF:src/core/config/settings.rs, REF:src/adapters/config/mod.rs）

各構造体は `#[serde(flatten)] extra: serde_json::Map<String, Value>` を持ち、`extra` が空でない場合に `W1002` を出す。書き戻し（`config init --force`）では `extra` を元の階層へ戻す。

### 6.3 参照の解決

状態: 実装完了（REF:src/core/config/settings.rs, REF:src/adapters/config/mod.rs）

`resolve_refs_for_id`（ネットワーク・フィルタ・クォータ共通）は探索ディレクトリを順に走査し、`<id>.json` を探す。見つからない場合は候補一覧を `details` に付して `E1003`。

### 6.4 破損した JSON

状態: 実装完了（REF:src/core/config/settings.rs, REF:src/adapters/config/mod.rs）

`serde_json::from_str` のエラーから `line` と `column` を取り出し、`E1003` のメッセージへ含める。コメント付き JSON は受理しない（`serde_json` の既定動作のまま）。

---

## 7. 表示と初期化

状態: 実装完了（REF:src/core/config/settings.rs, REF:src/adapters/config/mod.rs）

### 7.1 `config show`

状態: 実装完了（REF:src/usecases/config.rs, REF:src/ui/render.rs）

`usecases::config::describe(app, resolved) -> Vec<ConfigRow>` が全キーを 1 か所で組み立てる。`ConfigRow { key, value, origin, source }` の `origin` は `Origin`（`Argument` / `Env(String)` / `File(PathBuf)` / `Default`）で、`source` は由来の補足（環境変数名・ファイルパス）である。

由来の決定は「引数 → 環境変数 → 読み込んだファイル（`resolved.sources` のうち当該キーを含む最も強いもの）→ 既定」の順に判定する。ファイル由来の判定は、読み込み時にキーごとの由来を記録するのではなく、`settings.json` の生 JSON を再帰的に走査してキーの存在を確かめる（マージ後の型では由来が失われるため）。`--config` / `LLMSEARCH_CONFIG` は全キーに優先する。

`apiKey` 等の秘匿値と `value_env` の解決値は `value` に載せず `***` とする（`core::secret::Secrets` の値をそのまま表示しない）。`--json` では `values` の各要素に `key` / `value` / `origin` / `source` を出す。

### 7.2 `config path`

状態: 実装完了（REF:src/usecases/config.rs, REF:src/ui/render.rs）

`usecases::config::paths(app, resolved) -> Vec<PathRow>` が `PathRow { label, path, used }` を返す。`used` は実際に読み込んだ（または使用する）パスで、`false` は「存在しない候補」を表す。並びは `settings.json` の探索候補 → `network` / `filters` / `quota`（解決ファイルと探索ディレクトリ候補）→ エンジン定義ディレクトリ → 状態ディレクトリと配下の実ファイル（履歴・カウンタ・セッション・文書・robots）。

`src/ui/render.rs` は `[使用]` / `[候補]` の接頭辞を付けて 1 行 1 件で表示する。`--json` では `paths` に `label` / `path` / `used` を持つ配列を出す。

### 7.3 `config init`

状態: 実装完了（REF:src/usecases/config.rs, REF:src/adapters/config/mod.rs）

雛形は `include_str!` で `assets/config/settings.json`、`assets/network/default.json`、`assets/filters/default.json`、`assets/quotas/default.json` を埋め込み、`usecases::config::init(state_dir, force) -> InitReport` が 4 ファイルを書き出す。`InitReport` は「書き出したパス」と「既存のため飛ばしたパス」を持つ。

`--force` が無いときに既存ファイルがあっても、他のファイルの書き出しは続ける（4 ファイルのうち 1 つあるだけで全部を諦めない）。既存を飛ばした場合は `E1004` ではなく通常終了とし、飛ばしたパスを表示して利用者に `--force` を案内する。書き出しは `write_atomic` を使う。

### 7.4 `config validate`

状態: 実装完了（REF:src/ui/render.rs）

設定 4 種とエンジン定義の検証を順に実行し、`Vec<Diagnostic>` を `src/ui/render.rs` で `[ok]/[warn]/[fail]` に整形する。

### 7.5 `config explain <key>`

状態: 実装完了（REF:src/usecases/config.rs, REF:src/ui/render.rs）

`usecases::config::explain(app, resolved, env, key)` が `describe` の該当行から値と由来を返し、`src/ui/render.rs` が 1 行で表示する。キーが存在しない場合は `E1004` と、`describe` のキー一覧から編集距離の近い候補を `details` に付す。`config show` と同じ由来判定を使う（表示の食い違いを作らない）。
