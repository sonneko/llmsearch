# 仕様書: フィルタリング（JSON）

本書は検索結果と本文のノイズ除去を JSON で宣言するための仕様を定義する。フィルタはコードに埋め込まず、宣言の評価のみで動作する。

---

## 1. 基本方針

### 1.1 対象

| 種別 | 対象 | 適用時点 |
|---|---|---|
| 結果フィルタ（`results`） | 検索結果 1 件（タイトル・URL・スニペット・サイト・日付） | 正規化・重複除去の直後、識別子の付与前 |
| 本文フィルタ（`content`） | 変換後の Markdown、および見出し・節 | 変換直後、およびアウトライン生成時 |

### 1.2 フィルタ集合

フィルタは「フィルタ集合」という名前付きの JSON（`filters/<id>.json`）にまとめる。設定 `filters` で ID を参照するか、インライン定義を直接記述する。camelCase を正規形として出力し、互換性のため `schema_version`, `max_results`, `max_per_host`, `host_regex`, `url_regex`, `path_prefix`, `title_regex`, `snippet_regex`, `title_contains`, `older_than_days`, `newer_than_days`, `strip_selectors`, `strip_patterns`, `strip_lines`, `keep_only`, `min_section_chars`, `drop_empty_sections`, `max_heading_depth`, `shorter_than`, `repeated_at_least`, `collapse_blank_lines`, `trim_line_ends`, `ensure_trailing_newline`, `decode_entities` などの定義済み snake_case 別名も入力として受け付ける。`filters show <id>` は読み込んだ定義を camelCase に正規化し、`filters show <id> --raw` はファイル内の JSON を変更せず表示する。

### 1.3 評価の原則

- フィルタは宣言順に評価する。`deny` → `allow` → `require` → `maxPerHost` → `diversity` → `maxResults` の順で適用する（§3.3）。
- 評価は副作用を持たない。フィルタの評価結果は「残す／除く」と「理由」のみである。
- 除いた件数と理由の内訳を必ず集計し、表示・履歴に残す。黙って捨てない。

---

## 2. フィルタ集合の構造

### 2.1 トップレベル

| キー | 必須 | 型 | 内容 |
|---|---|---|---|
| `schemaVersion` | 必須 | 整数 | スキーマ版（初期リリースは `2`） |
| `id` | 必須 | 文字列 | フィルタ集合の ID |
| `description` | 任意 | 文字列 | 説明 |
| `mode` | 任意 | 文字列 | 予約（読むだけで評価に影響しない。既定は空文字） |
| `results` | 任意 | オブジェクト | 結果フィルタ（§3） |
| `content` | 任意 | オブジェクト | 本文フィルタ（§4） |

### 2.2 `mode`

`mode` は予約項目である。パース時に保持するだけで、`replace` / `append` のような意味付けは行わず、評価結果にも影響しない。利用者設定で既定のフィルタ集合に規則を追加したい場合は、`filters` にインライン定義を書く（`results` を指定すると既定の `results` を丸ごと置き換える）。

---

## 3. 結果フィルタ（`results`）

### 3.1 構造

| キー | 型 | 既定値 | 内容 |
|---|---|---|---|
| `deny` | 配列 | `[]` | 除外する規則 |
| `allow` | 配列 | `[]` | 除外規則に該当しても残す規則（例外） |
| `require` | 配列 | `[]` | すべて満たす必要がある条件 |
| `maxResults` | 整数 | 未設定 | フィルタ後の最大件数 |
| `maxPerHost` | 整数 | 未設定 | 同一ホストあたりの最大件数 |
| `diversity` | 真偽値 | `false` | ホストの分散を優先して並べ替える |

### 3.2 規則（`deny` / `allow` / `require`）

1 つの規則は次のキーの組み合わせで表す。複数キーを指定した場合は AND で評価する。キーを 1 つも指定しない規則は、どの結果にも一致しない（全件除外を避けるため）。

| キー | 型 | 内容 |
|---|---|---|
| `host` | 文字列/配列 | ホスト名の完全一致、または `*.example.com` 形式の接尾辞一致 |
| `hostRegex` | 文字列 | ホスト名の正規表現 |
| `urlRegex` | 文字列 | URL 全体の正規表現 |
| `pathPrefix` | 文字列 | URL パスの前方一致 |
| `titleRegex` | 文字列 | タイトルの正規表現 |
| `snippetRegex` | 文字列 | スニペットの正規表現 |
| `titleContains` | 文字列/配列 | タイトルの部分一致（大文字小文字を区別しない） |
| `scheme` | 文字列/配列 | `http` / `https` |
| `olderThanDays` | 整数 | 公開日が指定日数より古い（公開日が無い場合は該当しない） |
| `newerThanDays` | 整数 | 公開日が指定日数より新しい |
| `extension` | 文字列/配列 | URL の拡張子（`pdf`, `zip` 等） |
| `language` | 文字列/配列 | 結果の `language` が一致する（現在は常に未設定のため、実質どの結果にも一致しない） |

### 3.3 評価順

1. `deny` を順に評価し、最初に該当した規則で除外候補とする。
2. 除外候補に対し `allow` を評価し、該当すれば残す。
3. 残った結果に `require` を評価し、満たさないものを除外する。
4. `maxPerHost` を適用し、超過分を除外する（同じホストでは**先に現れたもの**を残す）。
5. `diversity=true` の場合はホストを分散して並べ替える（各ホストの元の順序は保つ）。
6. `maxResults` を適用し、末尾を切り捨てる。
7. 残った結果へ `R1` からの識別子と表示順を振り直す（欠番を作らない）。

### 3.4 既定のフィルタ集合

同梱の `default`（`assets/filters/default.json`）は次の 4 種類の `deny` と 1 種類の `allow` を持つ。いずれも利用者設定で上書き・追加できる。

- `deny`: `extension` が `pdf`, `zip`, `exe`, `dmg`, `apk`, `iso` の結果（本文を読めないため）
- `deny`: `host` が `*.pinterest.com` / `*.quora.com` / `*.facebook.com` かつ `urlRegex` がログイン・登録ページの結果（両方を満たす場合のみ）
- `deny`: `titleRegex` がログイン・サインインの定型で始まる結果
- `deny`: `titleRegex` が `404 Not Found` などの結果
- `allow`: `host` が `*.wikipedia.org` / `*.go.jp` / `*.ac.jp` の結果（`deny` の例外）
- `require`: なし
- `maxResults`: `60`、`maxPerHost`: `3`、`diversity`: `true`

---

## 4. 本文フィルタ（`content`）

### 4.1 構造

| キー | 型 | 既定値 | 内容 |
|---|---|---|---|
| `stripSelectors` | 文字列配列 | `[]` | 変換前に除去する要素（既定の除去要素に追加する） |
| `stripPatterns` | 文字列配列 | `[]` | 変換後に除去する行の正規表現 |
| `stripLines` | 配列 | `[]` | 行単位の除去条件（§4.3） |
| `keepOnly` | 配列 | `[]` | 残す行の条件（指定時は他を除去） |
| `normalize` | オブジェクト | — | 空白・空行の正規化（§4.4） |
| `minSectionChars` | 整数 | 未設定 | 予約。現在はアウトラインの表示に影響しない |
| `dropEmptySections` | 真偽値 | `true` | 予約。現在はアウトラインの表示に影響しない |
| `maxHeadingDepth` | 整数 | 未設定 | 予約。現在はアウトラインの表示に影響しない |

### 4.2 既定の除去要素

`stripSelectors` の既定（仕様として固定）は `script`, `style`, `noscript`, `iframe`, `svg`, `canvas`, `form`, `nav`, `aside`, `footer`, `header`, `dialog` である。`stripSelectors` を指定した場合はこの既定に**追加**する（置き換えない）。`[role=navigation]` と `[aria-hidden=true]` は既定に含まない（`nav` の除去で足りるため）。

### 4.3 `stripLines` の条件

| キー | 型 | 内容 |
|---|---|---|
| `regex` | 文字列 | 行全体の正規表現 |
| `equals` | 文字列 | 行が完全一致する |
| `shorterThan` | 整数 | 行の文字数が指定未満 |
| `repeatedAtLeast` | 整数 | 同一行が文書内で指定回数以上出現する（ナビゲーションの反復を除去） |
| `boilerplate` | 文字列配列 | 定型文の一覧（`Cookie`, `クッキー`, `プライバシーポリシー`, `All rights reserved` 等）のいずれかを含む（大文字小文字を区別しない） |

### 4.4 `normalize`

| キー | 型 | 既定値 | 内容 |
|---|---|---|---|
| `collapseBlankLines` | 真偽値 | `true` | 連続する空行を 1 行に圧縮する |
| `trimLineEnds` | 真偽値 | `true` | 行末の空白を除去する（コードブロック内は除く） |
| `ensureTrailingNewline` | 真偽値 | `true` | 末尾に改行を 1 つだけ残す |
| `decodeEntities` | 真偽値 | `true` | HTML エンティティを復号する |

### 4.5 適用順

1. HTML からの変換時: `stripSelectors` → 本文抽出 → Markdown 変換
2. Markdown 後処理: `stripPatterns` → `stripLines` → `keepOnly` → `normalize`

アウトラインの表示深度は `--depth` と `context.outlineDepth` で決める。`minSectionChars` / `dropEmptySections` / `maxHeadingDepth` は定義と検証のみ行い、表示には適用しない（§4.1 の予約項目）。

### 4.6 既定のフィルタ集合（本文）

同梱の `default` には次を含める。

- `stripPatterns`: 広告だけの行、`記事をシェア` / `この記事をシェアする` / `Share this article` だけの行
- `stripLines.boilerplate`: `Cookie`, `クッキー`, `プライバシーポリシー`, `利用規約`, `無断転載を禁じます`, `All rights reserved`, `JavaScript を有効`, `このサイトを続けて閲覧`
- `stripLines.repeatedAtLeast`: `3`（同一行の反復を除去）
- `normalize`: 既定どおり
- `minSectionChars`: `0`、`dropEmptySections`: `true`、`maxHeadingDepth`: `4`（いずれも予約。現在は表示に影響しない）

---

## 5. 集計と可視化

### 5.1 集計

フィルタ適用後の結果には次を含める。

| 項目 | 内容 |
|---|---|
| `kept` | 残した件数 |
| `removed` | 規則除外、重複 URL、および `maxPerHost` 超過の合計。`maxResults` と表示ページの `--limit` による切り捨ては含まない |
| `reasons` | 理由ごとの件数（規則 ID、`maxPerHost`、`duplicateUrl` など） |
| `truncatedByMaxResults` | フィルタ集合の `maxResults` による切り捨て件数 |
| `truncatedByMaxPerHost` | `maxPerHost` 上限を超えた件数（`removed` にも含む） |
| `disabled` | 規則フィルタが `--no-filter` で無効化されたか |
| `truncatedByLimit` | 表示ページの `--limit` を超えているため、今回の一覧に出さなかった件数 |

### 5.2 表示

- 一覧の末尾に、今回の表示件数・全対象件数と除外／切り捨て区分を 1 行表示する。例: `(filters: showing 7 of 10 results; 3 beyond --limit)`。規則除外、重複 URL、`maxPerHost`、`maxResults`、表示 `--limit` は区別する。何も除外・切り捨てが無いときは `(filters: all 7 shown)`、無効で除外・切り捨ても無いときは `(filters: disabled)` とする。
- 理由の内訳は一覧に出さない。`--verbose` でも規則 ID ごとの内訳は出さず、JSON の `filtered.reasons` で確認する。
- JSON 出力では `search` / `next` イベントの `filtered` オブジェクトに `kept` / `removed` / `reasons` / `disabled` / `truncatedByMaxPerHost` / `truncatedByMaxResults` / `truncatedByLimit` を含める。`maxPerHost` による除外は `removed` に含め、`truncatedByMaxResults` と `truncatedByLimit` は個別カウントとして示す。

### 5.3 無効化

`--no-filter` 指定時は、結果の `deny` / `allow` / `require` 規則と、変換後の本文フィルタ（`stripPatterns` / `stripLines` / `keepOnly` / `normalize`）を適用しない。ただし結果セットの上限制御 `maxPerHost` / `maxResults` は常に適用し、`filtered.disabled: true` と各除外件数を記録する。HTML 変換時の `stripSelectors`（既定の除去要素と §4.2）は `--no-filter` でも適用する（安全性のため無効化する手段は提供しない）。
