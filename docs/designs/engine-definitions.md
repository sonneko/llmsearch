# 設計書: エンジン定義（JSON）

本書は `REF:docs/specifications/engine-definitions.md` に対応する実装設計を記述する。

---

## 1. 基本方針

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

### 1.1 宣言と評価の分離

状態: 実装完了（REF:src/adapters/engine/eval.rs, REF:src/core/engine.rs）

定義の型は `src/core/engine.rs` に `serde::Deserialize` 付きの構造体として置く。評価器は次の 2 つに分ける。

- `src/core/template.rs`: `{{var}}` の展開（`expand_template(spec, ctx) -> Result<String>`）。URL エンコードは `encoding_rs` で `query` を符号化する関数 `encode_query` に閉じる。
- `src/adapters/engine/eval.rs`: HTTP 応答 → `Vec<SearchHit>` への写像。`response.kind` による分岐はここだけに存在し、`Extractor` トレイト（`extract_items`）の実装を `registry` から引く。

`core` 側は「定義の妥当性」と「変換の意味」を知るが、HTTP や HTML の具体 API には依存しない。セレクタ構文の検証は `scraper::Selector::parse` を `adapters` 側で行い、`core` では `SelectorSpec`（文字列）として保持する。

### 1.2 対象とする応答

状態: 実装完了（REF:src/core/extract.rs）

`Extractor` の実装は `HtmlExtractor`（`src/core/extract.rs`）と `JsonExtractor`（`src/core/extract.rs`）。`xml` を追加する場合は同トレイトの実装を 1 つ足し、`registry.rs` の `match kind` に 1 行追加する。初期リリースでは `xml` を登録しない。

### 1.3 定義の同一性とキャッシュ

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

`engine_hash(def)` は正規化 JSON（キー順をソートした `serde_json::to_string`）の SHA-256 先頭 16 文字を返す。キャッシュキーは `(engine_id, engine_hash, query, page, limit)` とし、`DefaultHasher` ではなく SHA-256 を用いて実行間で安定させる。

---

## 2. エンジン定義の構造

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

### 2.1 トップレベル

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

`EngineDef` は仕様の表のキーをそのまま持つ。`schemaVersion` は `u32` で受け、`SUPPORTED_SCHEMA_VERSION: u32 = 1` 以外は `E1004`。`id` は `Regex`（`^[a-z0-9][a-z0-9._-]*$`）で検証する。`variant` は省略時 `"default"`。

### 2.2 未知キーの扱い

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

各構造体に `#[serde(flatten)] pub extra: serde_json::Map<String, Value>` を持たせ、`engines show --raw` と書き戻し時に再出力する。警告は `validate` 時にキー名を列挙して `W1002` を 1 回だけ返す。

---

## 3. リクエスト定義（`request`）

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

### 3.1 構造

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

`RequestSpec` は `method: Method`, `url: String`, `query: BTreeMap<String, Tmpl>`, `headers: BTreeMap<String, Tmpl>`, `body: Option<String>`, `content_type: Option<String>`, `encoding: Encoding`, `timeout_ms: Option<u64>` を持つ。`query` を `BTreeMap` にして順序を固定し、リクエスト文字列を決定論的にする。

### 3.2 テンプレート変数

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

`core/template.rs` の `expand(tmpl: &str, ctx: &TmplCtx) -> Result<String, AppError>` が 1 パスで `{{name}}` を置換する。未知の変数は `E1004`。`encoding` が `utf-8` 以外の場合は `encode_query` を通してから URL エンコードする（`url::form_urlencoded` は UTF-8 前提のため、独自に `%XX` を組み立てる）。

### 3.3 認証情報の参照

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

`Tmpl` は `Value(String)` / `Env(String)` の列挙。`Env(name)` は `usecases::config::secrets` が起動時に解決し、`SecretString`（`Debug` 実装で `***` を返す newtype）として保持する。`serde::Serialize` を通さない型にすることで、出力経路への混入を型で防ぐ。

### 3.4 送信前の検証

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

`validate_request(spec, secrets)` がスキーム・未知変数・`apiKey` 使用時の `value_env` 有無を検査する。`engines test --dry-run` はこの関数の結果を表示し、送信は `usecases::search::run` 内で行う。

---

## 4. 応答と抽出（`response`）

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

### 4.1 構造

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

`ResponseSpec { kind: ResponseKind, charset: Option<String>, items: ItemSpec, fields: FieldMap, error_when: Vec<ErrorCond> }`。`fields` は `BTreeMap<FieldName, FieldSpec>` とし、`title` と `url` の存在を `validate` で必須化する。

### 4.2 `items` の指定

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

`ItemSpec` は `Selector { selector: String }` または `Path { path: String }`。`kind` と `items` の組み合わせが矛盾する場合（`html` に `path` 等）は `E1004`。

### 4.3 `fields` の指定

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

`FieldSpec` は `Selector`, `Path`, `From`, `Const`, `Join` の列挙。`HtmlExtractor` は `Selector` と `Const` のみ、`JsonExtractor` は `Path`・`Const` を直接扱い、`From`・`Join` は共通層（`adapters/engine/derive.rs`）で解決する。これにより変換ロジックの重複を避ける。

### 4.4 変換（`transform`）

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

`Transforms` は `Vec<Transform>` を順に適用する純粋関数 `apply_transforms(value: &str, ts: &[Transform], ctx: &TransformCtx) -> String`。`TransformCtx` は `base_url`, `tracking_params`, `date_formats` を持つ。`absolutize` は `url::Url::join`、`to_date` は `chrono::NaiveDate::parse_from_str` を候補形式の順に試す。

### 4.5 結果なしの判定（`errorWhen`）

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

`ErrorCond` は `FieldEmpty`, `Contains`, `PathExists`, `Status`。評価順は定義順とし、最初に該当した条件で「0 件」を返す。`Contains` の対象は応答本文（`charset` で復号後）であり、比較は部分一致（大文字小文字を区別する）。

### 4.6 抽出できなかった場合

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

`title` か `url` が空の要素は破棄し、破棄件数を数える。全件破棄時は `W4004` とともに応答先頭 2048 文字のテキストを標準エラーへ出す（`Debug` 出力は使わない）。応答本文の全文出力は行わない。

### 4.7 リダイレクト URL の展開

状態: 実装完了（REF:src/core/url.rs, REF:src/core/extract.rs, REF:src/core/search.rs, REF:src/usecases/search.rs）

- 抽出時: `unwrap_redirect` が `REF:src/core/extract.rs` の `unwrap_redirect` を呼ぶ。基準 URL（`base_url`）で絶対化してから解釈するため、`//duckduckgo.com/l/?uddg=...` のようなプロトコル相対・相対パスでも解ける。値のパーセントエンコードと 2 段までの入れ子を解く。
- 正規化時: 定義側の書き漏らしに備え、`REF:src/core/search.rs` の `unwrap_redirects` が全結果 URL をもう一度検査する。変換した件数を `W4006` として `REF:src/usecases/search.rs` から報告する。判定は `REF:src/core/url.rs` の `target_of_redirect`（設定に依存しない既定キー）。
- 取得時: HTTP の 3xx は HTTP 層が追う。加えて `REF:src/adapters/extract/mod.rs` が `REF:src/core/url.rs` の `html_redirect_target` で `<meta http-equiv="refresh">` とスクリプトの location 遷移を検出し、`maxRedirects` を上限にたどり直す。中継ページの本文を「記事」として読んでしまうことを防ぐ。

---

## 5. ページング定義（`paging`）

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

### 5.1 構造

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

`PagingSpec { style: Style, offset_param: String, page_param: String, page_base: u32, page_size: Option<u32>, max_results: u32, offset_step: Option<u32> }`。`Style` は `Offset` / `Page`。`style=offset` のとき `offsetParam`、`style=page` のとき `pageParam` のみを `TmplCtx` に渡す（不要な変数を定義に混ぜない）。

### 5.2 ページング非対応

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

`paging: Option<PagingSpec>` が `None` のとき `PageSlice::single(len)` を返し、`has_more = false`。`next` ユースケースは定義を見てこの判別を行う（エンジン ID では分岐しない）。

---

## 6. エンジン固有の上限（`limits`）

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

`Limits` は既定値を `impl Default` で持ち、`usecases::search::effective_limits(def, network)` が `limits.*` とネットワークプロファイル、`search.max_results` の最小値を採る。`requiresApiKey` は `secrets` の解決可否と併せて `engines list` の `available` 列に反映する。

---

## 7. 定義の配置と読み込み

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

### 7.1 配置場所と探索順

状態: 実装完了（REF:src/adapters/engine/loader.rs）

`src/adapters/engine/loader.rs` の `load_engines(dirs: &[PathBuf]) -> Result<EngineRegistry>` がディレクトリを順に走査し、`*.json` を `EngineDef` として読む。`id` の重複は先勝ちとし、後から見つかったものは `W4006`（重複定義の無視）として記録する。同梱既定は `include_dir` ではなく `include_str!` で埋め込まず、ビルド時に `engines/` をそのまま同梱パスとして参照する（`CARGO_MANIFEST_DIR` を実行時に使わないため、`--engines-dir` の既定に同梱ディレクトリを追加する）。

### 7.2 複数定義の束ね（プロファイル）

状態: 実装完了（REF:src/core/engine.rs）

`EngineRegistry` は `HashMap<(String, String), EngineDef>`（キーは `(id, variant)`）で保持し、`get("id")` は `("id", "default")` に解決する。`id@variant` の解析は `src/core/engine.rs`。

### 7.3 フォールバック

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

`usecases::search::run_with_fallback(chain, query)` が `chain` を順に試す。再試行するのは `AppError::Provider { kind: NoResults | Blocked }` のみで、他は即座に返す。成功時は `used_engine` と `W4005` を結果に含める。

### 7.4 複数エンジンのマージ（任意機能）

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

`core/search/merge.rs` の `round_robin_merge(per_engine: Vec<Vec<SearchHit>>) -> Vec<SearchHit>` は純粋関数。URL の正規形をキーに `HashSet` で重複を除去し、`engine` フィールドを付す。識別子の付与はマージ後に 1 回だけ行い、連番を保証する。

---

## 8. 検証コマンド

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

### 8.1 `engines list`

状態: 実装完了（REF:src/ui/render.rs）

`src/ui/render.rs` が表を描画する。`available` は「有効かつ認証情報が揃っている」ことで判定する。`--json` 時は `engines` イベントを 1 つ出力する。

### 8.2 `engines show <id[@variant]>`

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

解決済み定義は `EngineDef::resolved()`（既定値を埋めた複製）を `serde_json::to_value` で整形して表示する。`--raw` は `extra` を含む元の JSON を返す。

### 8.3 `engines validate [<id>...]`

状態: 実装完了（REF:src/usecases/engines.rs）

検証は `src/usecases/engines.rs` に 6 種類のチェック関数を置き、`Vec<Diagnostic>` を返す。セレクタ構文の検査は `scraper::Selector::parse`、JSON パスの検査は `core/extract/jsonpath.rs` のパーサを試す。失敗が 1 件でもあれば終了コード `1`。

明示指定された未知の識別子を黙殺せず、`E4002` を含む失敗チェックとして返す。`EngineDef::validate` は `request.method` の GET/POST と、`REF:src/core/template.rs` の `query_encoding` が対応する文字符号化も検査する。`ensure_valid` 経由のロード時にも同じ検証を行う。回帰テストは `REF:tests/m5.rs` と `REF:src/core/engine.rs`。

### 8.4 `engines test <id[@variant]> --query <q>`

状態: 実装完了（REF:src/ui/render.rs）

`--dry-run` は `validate_request` の結果を `src/ui/render.rs` で表示する（`Authorization` 等のヘッダは `***` へ置換）。実送信時は通常の `search` ユースケースを `limit=3` で呼び、クォータ消費を `Reporter` で事前に通知する。
