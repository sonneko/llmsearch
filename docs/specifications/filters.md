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

フィルタは「フィルタ集合」という名前付きの JSON（`filters/<id>.json`）にまとめる。設定 `filters` で ID を参照するか、インライン定義を直接記述する。

### 1.3 評価の原則

- フィルタは宣言順に評価する。複数の規則に該当する場合の最終判定は、種別ごとの `mode` に従う。
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
| `mode` | 任意 | 文字列 | `replace`（既定） / `append`（利用者設定側でのみ有効） |
| `results` | 任意 | オブジェクト | 結果フィルタ（§3） |
| `content` | 任意 | オブジェクト | 本文フィルタ（§4） |

### 2.2 `mode`

`append` は利用者設定で「参照先のフィルタ集合に規則を追加する」場合に用いる。追加した規則は参照先の規則より後ろで評価される。`append` で `replace` の動作を上書きすることはできない。

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

1 つの規則は次のキーの組み合わせで表す。複数キーを指定した場合は AND で評価する。

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
| `language` | 文字列/配列 | `defaults.language` が一致する |

### 3.3 評価順

1. `deny` を順に評価し、最初に該当した規則で除外候補とする。
2. 除外候補に対し `allow` を評価し、該当すれば残す。
3. 残った結果に `require` を評価し、満たさないものを除外する。
4. `maxPerHost` を適用し、超過分を除外する（ホスト内の順位が上のものを残す）。
5. `diversity=true` の場合はホストを分散して並べ替える。
6. `maxResults` を適用し、末尾を切り捨てる。

### 3.4 既定のフィルタ集合

同梱の `default` には次を含める。いずれも利用者設定で上書き・追加できる。

- `deny`: `extension` が `pdf`, `zip`, `exe`, `dmg`, `apk` の結果（本文を読めないため）
- `deny`: `host` が `localhost`, `127.0.0.1`, `*.local` の結果
- `deny`: `urlRegex` が `[?&](utm_|fbclid|gclid)` を含む結果（追跡付きの重複を避ける）
- `require`: なし
- `maxPerHost`: `3`

---

## 4. 本文フィルタ（`content`）

### 4.1 構造

| キー | 型 | 既定値 | 内容 |
|---|---|---|---|
| `stripSelectors` | 文字列配列 | `[]` | 変換前に除去する要素（追加分） |
| `stripPatterns` | 文字列配列 | `[]` | 変換後に除去する行の正規表現 |
| `stripLines` | 配列 | `[]` | 行単位の除去条件（§4.3） |
| `keepOnly` | 配列 | `[]` | 残す行の条件（指定時は他を除去） |
| `normalize` | オブジェクト | — | 空白・空行の正規化（§4.4） |
| `minSectionChars` | 整数 | 未設定 | この文字数未満の節をアウトラインで隠す |
| `dropEmptySections` | 真偽値 | `true` | 本文が空の節をアウトラインから除く |
| `maxHeadingDepth` | 整数 | 未設定 | この深さを超える見出しをアウトラインで隠す |

### 4.2 既定の除去要素

`stripSelectors` の既定（仕様として固定）は `script`, `style`, `noscript`, `iframe`, `svg`, `canvas`, `form`, `nav`, `aside`, `footer`, `header`, `[role=navigation]`, `[aria-hidden=true]` である。`stripSelectors` を指定した場合はこの既定に**追加**する（置き換えない）。

### 4.3 `stripLines` の条件

| キー | 型 | 内容 |
|---|---|---|
| `regex` | 文字列 | 行全体の正規表現 |
| `equals` | 文字列 | 行が完全一致する |
| `shorterThan` | 整数 | 行の文字数が指定未満 |
| `repeatedAtLeast` | 整数 | 同一行が文書内で指定回数以上出現する（ナビゲーションの反復を除去） |
| `boilerplate` | 文字列配列 | 定型文の一覧（`シェアする`, `Cookie`, `プライバシーポリシー` 等）に含まれる |

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
3. アウトライン生成時: `minSectionChars` / `dropEmptySections` / `maxHeadingDepth` を表示に適用（本文は保持する）

### 4.6 既定のフィルタ集合（本文）

同梱の `default` には次を含める。

- `stripLines.repeatedAtLeast`: `3`（同一行の反復を除去）
- `stripLines.boilerplate`: `シェアする`, `この記事を共有`, `Cookie`, `プライバシーポリシー`, `関連記事`, `広告`
- `normalize`: 既定どおり
- `minSectionChars`: `1`
- `dropEmptySections`: `true`

---

## 5. 集計と可視化

### 5.1 集計

フィルタ適用後の結果には次を含める。

| 項目 | 内容 |
|---|---|
| `kept` | 残した件数 |
| `removed` | 除いた件数の合計 |
| `reasons` | 理由ごとの件数（規則のキー名と値の組） |
| `truncatedBy` | `maxResults` / `maxPerHost` による切り捨て件数 |

### 5.2 表示

- 対話モードでは一覧の末尾に `（フィルタ: 12 件中 7 件を表示、除外 5 件）` の形で 1 行表示する。
- `--verbose` では理由の内訳を標準エラーへ表示する。
- JSON 出力では `search` イベントの `filtered` オブジェクトに含める。

### 5.3 無効化

`--no-filter` 指定時は、結果・本文の双方でフィルタを評価しない。この場合も集計は行い、`filtered.disabled: true` として記録する。既定の除去要素（§4.2）は安全性のため、`--no-filter` でも維持する（`--no-filter=all` を明示した場合のみ無効化する）。
