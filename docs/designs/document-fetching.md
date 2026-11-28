# 設計書: 文書取得と変換

本書は `REF:docs/specifications/document-fetching.md` に対応する実装設計を記述する。

---

## 1. 取得

状態: 実装完了（REF:src/adapters/extract/mod.rs, REF:src/core/convert.rs）

### 1.1 対象

状態: 実装完了（REF:src/adapters/extract/mod.rs, REF:src/core/convert.rs）

`trait Fetcher { fn fetch(&self, url: &Url, options: &FetchOptions) -> Result<FetchedDocument> }`（`REF:src/ports/fetch.rs`）。`FetchedDocument { meta, markdown, warnings, cached }` と、その下位の `RawDocument` / `HttpResponse` を返す。実装は `REF:src/adapters/extract/mod.rs` の `HttpFetcher` で、`HttpClient`（`REF:src/adapters/http.rs`）・`DocumentStore`・`Clock`・`Reporter`・`RobotsGate` を保持する。

### 1.2 プロトコルと制限

状態: 実装完了（REF:src/adapters/extract/mod.rs, REF:src/core/convert.rs）

スキーム検証は `core::url::validate_scheme`（`E2005`）。HTTP の 3xx は `REF:src/adapters/http.rs` の `send_blocking` が `redirectPolicy` ごとに処理する（`follow` は `reqwest` に委ね、上限超過は `E2004`。`manual` は `Location` を解決して 1 hop だけ返し、解決できない場合は `E2004`。`never` は `E2003`）。HTML 中継ページは `REF:src/adapters/extract/mod.rs` の `fetch_fresh` が `core::url::html_redirect_target` で `maxRedirects` まで追う。サイズ上限は `REF:src/adapters/http.rs` の `chunk()` ループで累積し、超過時は `HttpResponse.truncated` を立てる。

robots.txt の判定（`REF:src/adapters/robots.rs`）は `send_blocking` の直前、追従先を含めた各 hop で行う。拒否された時点で `E2006` を返し、取得もクォータ消費も行わない。

### 1.3 文字コード

状態: 実装完了（REF:src/adapters/extract/mod.rs, REF:src/core/convert.rs）

`REF:src/ports/fetch.rs` の `detect_charset` がヘッダ → `meta` の順に判定し、`decode_body` が復号する。プロファイルの `charset` 指定が最優先。推測した場合と置換文字が出た場合は `W3003` を出す。

### 1.4 圧縮とキャッシュ制御

状態: 実装完了（REF:src/adapters/extract/mod.rs, REF:src/core/convert.rs）

`reqwest` の `gzip` / `deflate` / `brotli` フィーチャに委ね、`Accept-Encoding` はプロファイルの `compression` から組み立てる。`REF:src/adapters/extract/mod.rs` の `cacheable` が `Cache-Control: no-store` を検出したら保存を見送り、`meta.cacheable` を false にする。

---

## 2. 本文抽出

状態: 実装完了（REF:src/adapters/extract/mod.rs, REF:src/core/convert.rs）

### 2.1 目的

状態: 実装完了（REF:src/adapters/extract/mod.rs, REF:src/core/convert.rs）

`REF:src/core/convert.rs` の `convert(html, final_url, mode, content_filters) -> Result<Converted>` が唯一の変換経路で、ポートのトレイトは置かない。抽出方式は `ExtractMode`（`Density` / `Readability` / `Whole`）で選び、`usecases::document` が `Density` を渡す。

### 2.2 手順

状態: 実装完了（REF:src/adapters/extract/mod.rs, REF:src/core/convert.rs）

1. `scraper::Html::parse_document` で DOM を構築する。
2. 本文領域を `pick_by_density`（`article` / `main` / `div` / `section` / `td` をリンク密度で評価）または `pick_by_readability`（段落の文字数を親へ伝播）で選ぶ。`whole` では選ばない。
3. 選んだ要素の HTML 断片に対して `remove_selectors(&html, &strip)` を適用する。`scraper` の DOM は可変操作を持たないため、`ElementRef::html()` で得た文字列を `replacen` で置換する方式にしており、`ego-tree` の `NodeRef` は使わない。既定の除去対象は `DEFAULT_STRIP`（`script` / `style` / `noscript` / `iframe` / `svg` / `canvas` / `form` / `nav` / `aside` / `footer` / `header` / `dialog`）とフィルタ集合の `content.stripSelectors`。
4. 断片を `htmd::HtmlToMarkdown` で変換する。
5. `core::markdown` の `normalize` → `absolutize` の順に適用する。抽出結果が 200 文字未満なら `W3002` を足す。

### 2.3 メタデータの取得

状態: 実装完了（REF:src/adapters/extract/mod.rs, REF:src/core/convert.rs）

`REF:src/core/convert.rs` の `convert` の中で `title` / `og:title` / `og:site_name` / `application-name` / `html[lang]` / `article:published_time` / `datePublished` / `time[datetime]` を読み、`build_meta` が `DocumentMeta` を組み立てる。`published` / `language` は `Converted` に載るが `DocumentMeta` には保存しない。`canonical` と著者は読まない。

### 2.4 JavaScript 依存ページ

状態: 実装完了（REF:src/adapters/extract/mod.rs, REF:src/core/convert.rs）

`REF:src/core/convert.rs` の `convert` の末尾で、変換後の Markdown が 200 文字未満なら `W3002` を 1 件足す。DOM の特徴（`#root` のみ、`__NEXT_DATA__` など）による判定は行わない。

---

## 3. Markdown 変換

状態: 実装完了（REF:src/adapters/extract/mod.rs, REF:src/core/convert.rs）

### 3.1 変換規則

状態: 実装完了（REF:src/adapters/extract/mod.rs, REF:src/core/convert.rs, REF:src/core/markdown.rs）

`htmd` の既定変換を用い、後処理は行わない。リンクの絶対化は `REF:src/core/markdown.rs` の `absolutize(md, base)` が基底 URL のクローンから `username` / `password` を空にしたうえで `[text](url)` / `![alt](url)` を正規表現で書き換え、リダイレクト先 URL 等に含まれる認証情報が相対リンクやキャッシュ本文（`body.md`）へ伝播しないようにする。`javascript:` のリンクは空文字へ、`data:` のリンクは URL を空にし、`#` / `mailto:` はそのまま残す。`<` 自動リンク記法は対象外。

### 3.2 見出しの同一性

状態: 実装完了（REF:src/adapters/extract/mod.rs, REF:src/core/convert.rs）

`REF:src/core/markdown.rs` の `flatten_heading(text)`（`text::collapse_ws` の薄い包み）が連続空白の圧縮と前後トリムを行う。見出しの抽出は `REF:src/core/outline.rs` の `extract_headings` が行い、レベルの補正はしない。

### 3.3 正規化

状態: 実装完了（REF:src/adapters/extract/mod.rs, REF:src/core/convert.rs）

`REF:src/core/markdown.rs` の `normalize(md, opts)` が 1 パスで、`TrimLineEnds` → `CollapseBlankLines` → 末尾改行 → `DecodeEntities` の順に適用する。フェンス状態は `REF:src/core/outline.rs` の `fence_flags` で判定し、コードブロック内の行末空白と空行は保護する。

### 3.4 変換結果の保存

状態: 実装完了（REF:src/adapters/extract/mod.rs, REF:src/core/convert.rs）

`DocumentMeta` を `meta.json`、本文を `body.md` に保存する。`DocumentMeta` は `url`, `final_url`, `title`, `site`, `fetched_at`, `expires_at`, `chars`, `headings`, `extractor`, `warnings`, `cacheable` を持つ（`REF:src/core/model.rs`）。`engine` は持たない。

---

## 4. キャッシュ

状態: 実装完了（REF:src/adapters/extract/mod.rs, REF:src/core/convert.rs）

### 4.1 保存単位

状態: 実装完了（REF:src/adapters/extract/mod.rs, REF:src/core/convert.rs）

`REF:src/adapters/state/mod.rs` の `StatePaths::document_dir(url)` が `documents/<cache_key>` を返す。`cache_key`（`REF:src/core/session.rs`）は URL の SHA-256 先頭 8 バイトを 16 進 16 文字にしたもの。`REF:src/adapters/state/session_store.rs` の `FsDocumentStore::put` / `get` が `meta.json` と `body.md` を扱う。

### 4.2 有効期限

状態: 実装中（REF:src/core/convert.rs, REF:src/usecases/document.rs）

`expires_at` は `REF:src/core/convert.rs` の `build_meta` が `fetched_at + ttlHours` で作る。なお読込側（`REF:src/usecases/document.rs` の `load`）は `cache_entry_valid`（`cacheable`・`expires_at` 未経過）を通過したキャッシュのみ再利用し、`--no-cache` / `--refresh` 指定時と `cache.enabled=false` 時は再取得する。画像は `strip_image_alt` が代替テキストを空にして出力する（仕様 document-fetching.md §3.1）。

### 4.3 無効化

状態: 実装完了（REF:src/adapters/extract/mod.rs, REF:src/core/convert.rs）

`--refresh` を指定するとキャッシュを使わずに再取得する。キャッシュ再利用時も、保存した警告一覧を `W3003`（`Diagnostic` の文言は保存時のもの）として再掲する。

### 4.4 保存しない場合

状態: 実装完了（REF:src/adapters/extract/mod.rs, REF:src/core/convert.rs）

`meta.cacheable == false`（`no-store` または `network.robots.respect == false`）、`cache.enabled == false`、`--no-cache` のいずれかで保存をスキップする。この場合も `FetchedDocument` は本文を返し、履歴には記録する。
