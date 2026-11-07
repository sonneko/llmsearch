# 設計書: 文書取得と変換

本書は `REF:docs/specifications/document-fetching.md` に対応する実装設計を記述する。パスは実装予定の配置である。

---

## 1. 取得

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 1.1 対象

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

取得は `DocumentFetcher` ポートの実装 `src/adapters/http/fetcher.ts` が担う。入力は URL と取得条件（タイムアウト、最大サイズ、キャッシュ可否）であり、出力は `RawDocument`（本文バイト列、Content-Type、charset、最終 URL、ステータス、ヘッダ）とする。取得ポートは HTML の解析を行わない。

### 1.2 プロトコルと制限

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

スキーム検証は `src/core/url/validate.ts` で行い、`http` / `https` 以外は `E2005`。リダイレクトは `fetch` の `redirect: 'manual'` で自前追従し、回数上限と訪問済み URL の集合でループを検出して `E2004` を返す。サイズ上限はストリームを読みながら累積バイト数を数え、超過時に打ち切って `W3003` を記録する。

### 1.3 文字コード

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

判定は「Content-Type の charset → `<meta charset>` の先頭 2048 バイト走査 → UTF-8」。デコードは `TextDecoder` に `fatal: false` で行い、`U+FFFD` の出現数を数えて 1 件以上なら `W3003` 系の警告（文字化けの可能性）を付す。

### 1.4 圧縮とキャッシュ制御

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`Accept-Encoding` は Node の `undici` に委ねず明示指定し、応答の `Cache-Control` を確認して `no-store` なら `cacheable=false` を返す。判定結果は `DocumentMeta` に保存する。

---

## 2. 本文抽出

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 2.1 目的

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`HtmlToMarkdown` ポート（`src/ports/html-to-markdown.ts`）を設け、既定実装は `linkedom` で DOM を構築し `@mozilla/readability` で本文を抽出、`turndown` で Markdown 化する。`linkedom` は jsdom より軽量でサンドボックスでの導入が容易である。

### 2.2 手順

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

1. `linkedom` の `parseHTML` で文書を構築する。
2. `Readability` に渡し、`parse()` の戻り値（`content`, `title`, `byline`, `siteName`, `publishedTime`）を得る。
3. `parse()` が `null` を返した場合は `<body>` 全体を対象とし、`W3001` を記録する。
4. 抽出 HTML を `turndown`（`turndown-plugin-gfm` 併用）で Markdown 化する。
5. `src/core/markdown/postprocess.ts` で空行圧縮・行末空白除去・エンティティ復号・相対 URL 絶対化を行う。

### 2.3 メタデータの取得

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/adapters/extract/meta.ts` で `title`, `og:title`, `og:site_name`, `canonical`, `html[lang]`, `time[datetime]` を読む。取得できない項目は `null` とし、`undefined` と `null` を区別しないよう型を `string | null` に統一する。

### 2.4 JavaScript 依存ページ

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/core/quality/js-hint.ts` に判定関数を置く。判定条件は「本文 200 文字未満」かつ「`#root`/`#app` のみ」「`__NEXT_DATA__` のみ」「`noscript` 内に JavaScript 必須の定型文」のいずれか。判定結果は `W3002` として `DocumentMeta.warnings` に記録する。

---

## 3. Markdown 変換

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 3.1 変換規則

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`turndown` の既定規則に加え、次のカスタム規則を `src/adapters/extract/turndown-rules.ts` に定義する。

- `pre code` の言語クラス（`language-*`, `lang-*`）を info string へ反映する。
- `table` は GFM プラグインに委ね、セル内の `|` をエスケープする規則を追加する。
- `img` は `alt` が空でも `![](...)` を出力する。
- `a` の `href` は `baseUrl` を基準に絶対化し、`javascript:` / `data:` はテキストのみ残す。

### 3.2 見出しの同一性

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

見出しテキストの生成は `src/core/markdown/heading-text.ts` の `flattenHeadingText()` で行い、連続空白の圧縮と前後トリムを適用する。レベルの補正は行わない（`h1` 不在時に `h2` を `h1` へ格上げしない）。

### 3.3 正規化

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/core/markdown/postprocess.ts` に段階ごとの関数（`collapseBlankLines`, `trimLineEnds`, `absolutizeLinks`, `decodeEntities`）を置き、パイプラインの順序を配列で固定する。順序はテストで固定する。

### 3.4 変換結果の保存

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`DocumentMeta`（`src/core/model/document.ts`）に取得日時・元 URL・最終 URL・タイトル・サイト名・文字数・見出し数・変換方式・警告一覧を持たせる。本文は `body.md`、メタデータは `meta.json` に分けて保存する（`REF:docs/designs/session-state.md` §3.2）。

---

## 4. キャッシュ

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 4.1 保存単位

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

URL の SHA-256 先頭 16 文字をディレクトリ名とする（`src/adapters/state/paths.ts` の `documentDir(url)`）。同一 URL の再取得判定は `meta.json` の `expiresAt` と現在時刻の比較で行う。

### 4.2 有効期限

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`cache.ttl_hours` から `expiresAt` を計算して保存する。`--refresh` は `expiresAt` を無視するフラグとして取得ユースケースに渡す。

### 4.3 無効化

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`meta.warnings` に `W3001` / `W3002` が含まれる場合、アウトラインのヘッダに注記を追加し、「再取得する」選択肢を表示する。再取得は同一 URL で `--refresh` 相当を実行する。

### 4.4 保存しない場合

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`cacheable=false`、`network.cache=false`、`--no-cache` のいずれかで保存をスキップする。この場合、本文はメモリ上でのみ扱い、セッションの `documents` には `bodyRef: null` として記録する。
