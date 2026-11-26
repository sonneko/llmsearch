# 設計書: エンジン定義（JSON）

本書は `REF:docs/specifications/engine-definitions.md` に対応する実装設計を記述する。

---

## 1. 基本方針

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

### 1.1 宣言と評価の分離

状態: 実装完了（REF:src/adapters/engine/eval.rs, REF:src/core/engine.rs）

定義の型は `src/core/engine.rs` に `serde::Deserialize` 付きの構造体として置く。評価器は次の 2 つに分ける。

- `src/core/template.rs`: `{{var}}` の展開（`expand(tmpl, ctx) -> Result<String, AppError>`）。URL エンコードは `encoding_rs` で `query` を符号化する関数 `encode_query` に閉じる。
- `src/adapters/engine/eval.rs`: HTTP 応答 → `Vec<SearchHit>` への写像。`response.kind` による分岐はここだけに存在し、`Extractor` トレイト（`extract_items`）の実装を `registry` から引く。

`core` 側は「定義の妥当性」と「変換の意味」を知るが、HTTP や HTML の具体 API には依存しない。セレクタ構文の検証は `scraper::Selector::parse` を `adapters` 側で行い、`core` では `SelectorSpec`（文字列）として保持する。

### 1.2 対象とする応答

状態: 実装完了（REF:src/core/extract.rs）

`Extractor` の実装は `HtmlExtractor`（`src/core/extract.rs`）と `JsonExtractor`（`src/core/extract.rs`）。`xml` を追加する場合は同トレイトの実装を 1 つ足し、`registry.rs` の `match kind` に 1 行追加する。初期リリースでは `xml` を登録しない。

### 1.3 定義の同一性とキャッシュ

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

`EngineDef::hash()` は `serde_json::to_string(self)` の SHA-256 の先頭 8 バイトを 16 進 16 文字で返す。キャッシュキーは `(engine_id, engine_hash, query, page, limit)` とし、`DefaultHasher` ではなく SHA-256 を用いて実行間で安定させる。

---

## 2. エンジン定義の構造

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

### 2.1 トップレベル

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

`EngineDef` は仕様の表のキーをそのまま持つ。`schemaVersion` は `u32` で受け、`SUPPORTED_SCHEMA_VERSION: u32 = 1` 以外は `E1004`。`id` は `Regex`（`^[a-z0-9][a-z0-9._-]*$`）で検証する。`variant` は省略時 `"default"`。

`network` はエンジン定義が優先する。検索の実行時に `REF:src/usecases/search.rs` が「エンジン定義の `network` → 起動引数 `--network` → 設定 `network`」の順で解決するため、`--network` はエンジン固有の指定を持たないエンジンにだけ効く（仕様 engine-definitions.md §2.1 と cli-interface.md §1 の優先順を実装で確定させたもの。挙動の変更ではない）。

### 2.2 未知キーの扱い

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

各構造体に `#[serde(flatten)] pub extra: serde_json::Map<String, Value>` を持たせ、`engines show --raw` と書き戻し時に再出力する。警告は `validate` 時にキー名を列挙して `W1002` を 1 回だけ返す。

---

## 3. リクエスト定義（`request`）

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

### 3.1 構造

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

`RequestSpec` は `method: String`, `url: String`, `query: BTreeMap<String, TmplValue>`, `headers: BTreeMap<String, TmplValue>`, `body: Option<String>`, `content_type: Option<String>`, `encoding: String`, `timeout_ms: Option<u64>` を持つ。`query` を `BTreeMap` にして順序を固定し、リクエスト文字列を決定論的にする。`body` は送信時のテンプレート展開後も実体を保持するが、`engines test --dry-run` の表示では環境変数値・明示値・認証パラメータを伏字化する。

### 3.2 テンプレート変数

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

`core/template.rs` の `expand(tmpl: &str, ctx: &TmplCtx) -> Result<String, AppError>` が 1 パスで `{{name}}` を置換する。未知の変数は `E1004`。`encoding` が `utf-8` 以外の場合は `encode_query` を通してから URL エンコードする（`url::form_urlencoded` は UTF-8 前提のため、独自に `%XX` を組み立てる）。

`TmplCtx` に `language: String`（主言語タグ）を足し、`locale` は同じ `search.language` の正規化後の全体を入れる（M10）。`core/engine.rs` の `ALLOWED_VARS` にも `language` を追加する。`Defaults` は `region` と `#[serde(flatten)] extra` のみとし、`locale` / `language` フィールドを削除する（旧定義では未知キーとして `extra` に落ち、`EngineDef::validate()` が `W1002` を出す。`EngineDef` 側にも `extra` があるため実装はどちらでも温存される）。言語は常に `search.language` が決め、定義からは上書きできない。

`Accept-Language` の補完は `src/adapters/engine/eval.rs` の `evaluate()` で行う。`template::prepare` の結果に `Accept-Language` が無ければ `ctx.locale`（正規化後の全体）を挿入する。`engines test --dry-run` の表示にも同じ補完結果を出す（`usecases::engines::dry_run` が同じ処理を共有する）。

`Defaults` の未知キー（`locale` / `language`）は `Defaults::validate()` で `W1002` とし、`EngineDef::validate()` がそれを取り込む。

### 3.3 認証情報の参照

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

`TmplValue` は `Value(String)` / `Env { value_env }` / `Explicit { value }` の列挙。`Env` は `Secrets` にある `Secret` から評価時に解決し、`Secret` は `Serialize` を実装せず `Debug` も値を出さない。`Explicit` は固定値としてリクエストへ展開できるが、定義表示では値を伏せる。JSON 定義の表示は `core::secret::redact_json()` を通し、`engines test --dry-run` は環境変数値と明示値を redaction list に含め、URL・認証ヘッダ・本文の認証フィールドを伏せる。

### 3.4 送信前の検証

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

`EngineDef::validate()` がスキーム・未知変数・`apiKey` 使用時の `value_env` 有無を検査する。`engines test --dry-run` は `template::prepare` の結果を表示し、実送信は `adapters::engine::evaluate` が行う。

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

`Transforms` は `Vec<Transform>` を順に適用する純粋関数 `apply_transforms(value: &str, ts: &[Transform], ctx: &TransformCtx) -> String`。`TransformCtx` は `base_url` と追跡パラメータの一覧を持つ（`REF:src/core/url.rs` の `DEFAULT_TRACKING` を `usecases::config` が `App.tracking` へ写す。日付形式は `to_date` が実装内の候補形式を用いる）。`absolutize` は `url::Url::join`、`to_date` は `chrono::NaiveDate::parse_from_str` を候補形式の順に試す。

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

`PagingSpec { style: PagingStyle, offset_param: String, page_param: String, page_base: u32, page_size: Option<u32>, max_results: u32, offset_step: Option<u32> }`。`PagingStyle` は `Offset` / `Page`。`style=offset` は `min(search.limit, limits.maxPageSize)` 件を要求し、その実要求数が `pageSize`（未設定時は `offsetStep`、さらに未設定なら要求数）より小さい場合は実要求数を offset 増分に使う。それ以外は `offsetStep`（省略時は `pageSize`）に従う。CLI のページ範囲も同じ実増分で検証する。`style=page` は `pageSize`（省略時は要求件数、`limits.maxPageSize` で上限）をプロバイダ幅として、論理ページを満たす隣接プロバイダページを順次取得して切り出す。`style=offset` のとき `offsetParam`、`style=page` のとき `pageParam` のみを `TmplCtx` に渡す。

### 5.2 ページング非対応

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

`paging: Option<PagingSpec>` が `None` のエンジンは `search_once` で `has_more = false` とし、CLI は `search --page 1` だけを許可する。`next` は保存済み `PageState.exhausted` を確認して、終端後の要求を拒否する。

---

## 6. エンジン固有の上限（`limits`）

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

`Limits` は `impl Default` で既定値を持つ。`EngineDef::max_results(global_max)` は `limits.maxResults` / `paging.maxResults` / `search.max_results` の最小値を返し、各 `call_engine` 要求の `limit` は `limits.maxPageSize` で制限する。`requiresApiKey` は `secrets` の解決可否と併せて `engines list` の `available` 列に反映する。

---

## 7. 定義の配置と読み込み

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

### 7.1 配置場所と探索順

状態: 実装完了（REF:src/adapters/engine/loader.rs）

`src/adapters/engine/loader.rs` の `load_registry(dirs: &[PathBuf]) -> Result<LoadedEngines, AppError>` がディレクトリを順に走査し、`*.json`（1 ファイルに配列で複数定義も可）を `EngineDef` として読む。`id@variant` の重複は先勝ちとし、後から見つかったものは `W4002`（既に読み込み済みの定義の無視）として記録する。1 件も見つからない場合は同梱既定（`Resource::Engines::embedded()` の `brave@api` / `duckduckgo@default` / `searxng@json`）へフォールバックする。同梱分の探索ディレクトリは `engine_dirs` が組み立てる。

### 7.2 複数定義の束ね（プロファイル）

状態: 実装完了（REF:src/core/engine.rs）

`EngineRegistry` は `HashMap<(String, String), EngineDef>`（キーは `(id, variant)`）で保持し、`get("id")` は `("id", "default")` に解決する。`id@variant` の解析は `src/core/engine.rs`。

### 7.3 フォールバック

状態: 実装完了（REF:src/core/engine.rs, REF:src/adapters/engine/loader.rs）

`usecases::search::run` は `search.merge` に応じて `run_fallback` または `run_merge` へ分岐する。`run_fallback` は `candidate_engines` の候補を順に試し、`ports::classify` が `FallbackClass::RetryOtherEngine`（`E4001` / `E4003` / `E4004`）を返す場合に限り次へ進む。それ以外（`E5005` を含む）は即座に返す。後続候補で成功した場合は `W4003` を付け、結果には実際に使ったエンジンを記録する。

### 7.4 複数エンジンのマージ（任意機能）

状態: 実装完了（REF:src/core/search.rs, REF:src/usecases/search.rs）

`src/core/search.rs` の `merge_round_robin(lists: Vec<Vec<RawHit>>, engines: &[String]) -> (Vec<RawHit>, Vec<String>, usize)` は純粋関数。各エンジンの結果を 1 件ずつ順に取り出し、URL の正規形（`url::dedup_key`）をキーに `HashSet` で重複を除去して、由来エンジン名を並べて返す。識別子の付与はマージ後に `assign_ids` で 1 回だけ行い、連番を保証する。

`run_merge` は `search.engines` を列挙順に逐次呼び、各エンジンの `search_once` が返す同じ論理ページを `merge_round_robin` に渡す。offset 方式は要求件数を `maxPageSize` で制限して 1 回送り、page 方式は必要な provider page を取得して論理ページを切り出す。失敗したエンジンはエラーコード付きの警告 1 行に変換して除外し、全滅した場合だけ最初のエラーを返す。`pending.searches` は各送信前の callback で成功・失敗にかかわらず積み、統合結果を返すときに `Guard::consume_all` で確定する（`quotas.md` §3.2）。クォータ拒否 `E5005` は途中でもマージ全体を中止し、他エンジンへの送信に進まない。マージ時は `search.fallback` の連鎖を作らない。

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

`--dry-run` は `EngineDef::validate()` と `template::prepare` の結果を `usecases::engines::dry_run()` で組み立て、`commands.rs` がテキストまたは JSON で表示する。URL・ヘッダ名・本文の秘匿値を伏せ、`Authorization` / Cookie 類は出自にかかわらずヘッダ名で `***` にする。実送信時は通常の `search` ユースケースを `limit=3` で呼び、クォータ消費を `Reporter` で事前に通知する。
