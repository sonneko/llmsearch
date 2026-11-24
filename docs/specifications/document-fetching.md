# 仕様書: 文書取得と変換

本書は選択された検索結果から HTML を取得し、本文を抽出して Markdown へ変換するまでの仕様を定義する。

---

## 1. 取得

### 1.1 対象

取得対象は、検索結果として選択された URL、`outline` / `read` に URL として与えられたもの、履歴から再出力する際にキャッシュが失効している URL である。検索結果に含まれない URL への巡回は行わない。

### 1.2 プロトコルと制限

- 対応スキームは `http` と `https` のみとする。それ以外（`file:`, `ftp:`, `data:` 等）は拒否する（`E2005`）。
- HTTP の 3xx はネットワークプロファイルの `redirectPolicy` に従って HTTP 層が扱う（`follow` / `manual` / `never`。追いきれない場合は `E2004`）。`follow` の上限は `maxRedirects`（既定 5）で、最終 URL を文書の正規 URL とする。加えて、HTML 中継ページ（`<meta http-equiv="refresh">` と JS の `location`）によるリダイレクトを `maxRedirects` の範囲で追う。`maxRedirects = 0` のときは 1 ホップも追わない。
- Content-Type が `text/html` / `application/xhtml+xml` / `text/plain` のいずれかを含むことを確認する。`text/plain` も HTML として変換する（プレーンテキスト専用の経路は持たない）。それ以外（`application/pdf`・画像・バイナリ）は `E3002` を返し、Content-Type を詳細に表示する。Content-Type が無い場合は本文の先頭 1 KiB に `<html` / `<!doctype html` / `<body` / `<meta` が含まれるかで判定する。
- 応答本文の最大サイズは `maxBytes`（既定 5 MiB）。超過した場合は打ち切って変換を続け、`W3003`（`truncated at the response size limit`）を出す。
- タイムアウトは `timeoutConnectMs`（既定 5000）と `timeoutTotalMs`（既定 20000）。
- 取得の前に `network.robots` の設定に従って robots.txt を判定し、拒否された場合は取得しない（`E2006`）。判定は HTML 中継ページによるリダイレクトの追従先についても行う。

### 1.3 文字コード

`Content-Type` の charset、HTML 内の `meta charset` の順に判定する（プロファイルの `charset` 指定があればそれを使う）。判定できない場合は UTF-8 とみなす。文字コードを推測した場合と、復号で置換文字が生じた場合は `W3003` を出す（`assumed charset <name> (replacement characters: <bool>)`）。

### 1.4 圧縮とキャッシュ制御

`gzip` / `deflate` / `br` を受理する（プロファイルの `compression`）。`Accept` ヘッダはプロファイルの `headers` で与える（既定は HTML を優先し JSON を許容する）。`Cache-Control: no-store` を返した文書はキャッシュへ保存しない。

---

## 2. 本文抽出

### 2.1 目的

ナビゲーション、広告、フッタ、コメント欄などの定型ノイズを除去し、記事本文を主成分とする HTML 断片を得る。抽出方式は `extractMode` で選ぶ（`density`（既定）/ `readability` / `whole`）。`density` は候補要素のリンク密度から本文領域を選び、`readability` は段落の文字数を親へ伝播させる簡易スコアリングを行う。

### 2.2 手順

1. `script`, `style`, `noscript`, `iframe`, `svg`, `canvas`, `form`, `nav`, `aside`, `footer`, `header`, `dialog` に加え、フィルタ集合の `content.stripSelectors` を除去する。`whole` では既定の除去を行わない。
2. 候補要素（`article` / `main` / `div` / `section` / `td`）のリンク密度から本文領域を推定する（`density`）。リンク密度が 0.5 を超える塊は本文から除く。
3. 見出し・段落・リスト・表・引用・コードブロック・画像の参照を保持する。
4. 本文推定に失敗した場合は `<body>` 全体を対象に変換し、`W3001` を出す。見出しが 1 つも無い文書は、タイトルを見出し（`h1`）として補ってから見出しを抽出する。

### 2.3 メタデータの取得

`title`（`og:title` を代替に使う）、サイト名（`og:site_name` / `application-name`）、`html[lang]`、公開日（`article:published_time` / `datePublished` / `time[datetime]`）を取得する。著者と `canonical` は取得しない。取得できない項目は空とし、代替値を創作しない。

### 2.4 JavaScript 依存ページ

変換後の Markdown が 200 文字未満の場合、描画不足の可能性を `W3002` として表示する（DOM の特徴による判定は行わない）。長さだけでは本文抽出の失敗（`W3001`）と区別できないため、両方が出ることがある。初期リリースでは描画を行わない。

---

## 3. Markdown 変換

### 3.1 変換規則

| HTML | Markdown |
|---|---|
| `h1`〜`h6` | `#`〜`######` の ATX 見出し。レベルは保持する |
| `p` | 段落。空行で区切る |
| `strong`, `b` | `**` |
| `em`, `i` | `*` |
| `code`（インライン） | バッククォート。内容にバッククォートを含む場合は区切りを増やす |
| `pre` | フェンス付きコードブロック。言語クラスが分かれば info string に反映する |
| `ul`, `ol`, `li` | `*` および番号。入れ子は 4 スペースでインデントする |
| `blockquote` | `>` |
| `table` | パイプ記法のテーブル。セル内の `\|` はエスケープする |
| `hr` | `* * *` |
| `br` | 行末の 2 スペース改行 |

リンクは「リンクテキスト」と「URL」を角括弧と丸括弧で組にした Markdown のリンク記法で表し、相対 URL は文書の正規 URL を基準に絶対化する。`javascript:` のリンクは記法ごと削除し、`data:` のリンクは URL を空にする。画像は代替テキストを空にした画像記法で出力する（元の `alt` は出力しない）。

### 3.2 見出しの同一性

見出しテキストは、HTML のインライン要素を平坦化し、連続する空白を 1 つにまとめ、前後の空白を除去して生成する。見出しレベルは HTML の `h1`〜`h6` に対応させ、補正（例: h1 が存在しない場合に h2 を h1 へ格上げする）は行わない。

### 3.3 正規化

- 連続する空行は最大 1 行に圧縮する。
- 行末の空白を除去する（コードブロック内の行は対象外。`br` 由来の改行も行末空白として除去される）。
- リンクの URL は `#` 始まり・`data:`・`mailto:` 以外を絶対 URL とし、`javascript:` はリンクごと削除する。
- HTML エンティティを復号する（`decodeEntities`）。
- 変換後の文字コードは UTF-8、改行は LF とする。

### 3.4 変換結果の保存

変換結果は Markdown 本文とメタデータ（取得日時、有効期限、元 URL、最終 URL、タイトル、サイト名、文字数、見出し数、抽出方式、警告一覧、キャッシュ可否）を合わせて保存する。`published` / `language` は取得するが、文書メタデータには保存しない。

---

## 4. キャッシュ

### 4.1 保存単位

文書は URL ごとに 1 件保存する。ディレクトリ名は URL の SHA-256 の先頭 8 バイト（16 進 16 文字）とし、`meta.json` と `body.md` を置く。同一 URL を別セッションで取得した場合、有効期限内であればキャッシュを再利用し、外部へのリクエストを行わない。再利用時の警告は、保存時の文言を `W3003` として再掲する。

### 4.2 有効期限

既定 24 時間（`cache.ttlHours`、既定 24）。有効期限は取得時刻 + `ttlHours` で決まり、過ぎていれば再取得する。`cache.ttlHours = 0` のときはキャッシュを読まない。`--refresh` 指定時も再取得する。

### 4.3 無効化

`--refresh` を指定した場合はキャッシュを使わずに再取得する。キャッシュを再利用した場合も、保存済みの警告（変換時の `W3001` / `W3002` など）はアウトライン表示時にあわせて表示する。

### 4.4 保存しない場合

`no-store` を返した文書、`cache.enabled = false` の設定時、`--no-cache` 指定時はキャッシュへ保存しない。この場合も履歴には記録するが、本文は保存しない。`network.robots.respect = false` のプロファイルでも保存しない（robots 判定を迂回した取得を再利用しないため）。
