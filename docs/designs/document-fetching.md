# 設計書: 文書取得と変換

本書は `REF:docs/specifications/document-fetching.md` に対応する実装設計を記述する。

---

## 1. 取得

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 1.1 対象

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`ports::Fetcher::fetch(&self, url: Url, opts: FetchOptions) -> Result<RawDocument>`。`RawDocument { bytes, content_type, charset, final_url, status, headers }`。実装は `adapters::http::fetcher` で、`reqwest::Client` を `NetworkProfile` から組み立てたものを保持する。

### 1.2 プロトコルと制限

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

スキーム検証は `core::url::validate`。リダイレクトは `redirect::Policy::none()` にして自前で追従し、`Vec<Url>` で訪問済みを管理して `E2004`（ループ）を検出する。サイズ上限は `bytes_stream` の累積で判定する。タイムアウトは `Client::timeout` と `Request::timeout` の二段構え。

### 1.3 文字コード

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`decode::decode_body` が仕様の順序で判定する。`encoding_rs::Encoding::for_label` で charset 名を解決し、未知の名前は UTF-8 にフォールバックして警告を出す。

### 1.4 圧縮とキャッシュ制御

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`reqwest` の `gzip` / `deflate` / `brotli` フィーチャに委ね、`Accept-Encoding` はプロファイルの `compression` から組み立てる。`Cache-Control: no-store` を検出したら `RawDocument.cacheable = false` とする。

---

## 2. 本文抽出

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 2.1 目的

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`trait HtmlToMarkdown { fn convert(&self, html: &str, base: &Url, opts: &ConvertOptions) -> Result<Converted> }` を `ports` に置く。既定実装は `adapters::extract::dom_density`。

### 2.2 手順

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

1. `scraper::Html::parse_document` で DOM を構築する。
2. `adapters::extract::strip::remove_selectors(&doc, &strip_selectors)` で不要要素を除去する（`scraper` は可変操作を直接持たないため、`html5ever` の `TreeSink` を用いるか、`ego-tree` の `NodeRef::detach` で切り離す）。
3. 本文領域の推定は `core::extract::density::score(paragraphs) -> Vec<Scored>`（純粋関数）で行い、最上位の祖先を本文とする。
4. 本文のみを `htmd`（HTML→Markdown）で変換する。
5. `core::markdown::postprocess::apply` で正規化する。

`@mozilla/readability` 相当の移植として `readability` クレートを試し、`scraper` と型が噛み合わない場合は 3 の自前密度推定で代替する（いずれも `Cargo.toml` の依存解決後に確定する）。

### 2.3 メタデータの取得

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`adapters::extract::meta::read(&doc, base) -> Meta`。`title`, `og:title`, `og:site_name`, `canonical`, `html[lang]`, `time[datetime]` を読み、取得できない項目は `None`。

### 2.4 JavaScript 依存ページ

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`core::quality::js_hint::detect(markdown_chars, dom_features) -> Option<Diagnostic>`。DOM の特徴（`#root`/`#app` のみ、`__NEXT_DATA__`、`noscript` 内の定型文）を `adapters::extract::features` が抽出し、判定は `core` の純粋関数で行う。

---

## 3. Markdown 変換

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 3.1 変換規則

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`htmd` の既定変換を用い、不足する規則（表・コードブロックの info string・画像の空 alt）は `adapters::extract::post_rules` で後処理する。リンクの絶対化は `core::markdown::link::absolutize(md, base)` で Markdown 中のリンク記法と画像記法を正規表現で書き換える（`<` 自動リンク記法にも対応する）。

### 3.2 見出しの同一性

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`core::markdown::heading::flatten(text)` が連続空白の圧縮と前後トリムを行う。レベルの補正は行わない。

### 3.3 正規化

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`normalize::apply` は 4 つの処理を個別関数として持つ。`trim_line_ends` はフェンス状態を追跡し、コードブロック内の行末空白を保護する。処理順は `NormalizeOptions` の適用順配列で固定する。

### 3.4 変換結果の保存

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`DocumentMeta` を `meta.json`、本文を `body.md` に保存する。`DocumentMeta` は `fetched_at`, `expires_at`, `url`, `final_url`, `title`, `site`, `chars`, `headings`, `engine`, `warnings` を持つ。

---

## 4. キャッシュ

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 4.1 保存単位

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`adapters::state::paths::document_dir/1` が URL の SHA-256 先頭 16 文字でディレクトリを作る。`state::documents::save/load` が `meta.json` と `body.md` を扱う。

### 4.2 有効期限

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`core::cache::expiry(now, ttl_hours)` で `expires_at` を計算し、`is_fresh` で判定する。`--refresh` はこの判定を飛ばす。

### 4.3 無効化

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`W3001` / `W3002` を含む文書は、アウトラインのヘッダに注記を付け、「再取得する」選択肢（`ui` の選択肢 `4` ではなく、`--refresh` の案内）を表示する。

### 4.4 保存しない場合

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`RawDocument.cacheable == false`、`cache.enabled == false`、`--no-cache` のいずれかで保存をスキップする。この場合 `DocumentMeta` はセッション状態にのみ保持し、`body_ref` を `None` とする。
