# 仕様書: 設定と構成（JSON）

本書は設定ファイルの形式・探索順・マージ規則・環境変数・既定値を定義する。設定はすべて JSON で記述する。検索エンジンの定義は別冊に従う。

---

## 1. 設定の全体像

### 1.1 構成要素

設定は次の 5 種類の JSON で構成する。いずれも単一ファイルに同居させても、`$ref` で分割してもよい。

| 種別 | 内容 | 既定の置き場所 |
|---|---|---|
| `settings.json` | 全体設定（検索・表示・コンテキスト・履歴・クォータ） | 状態ディレクトリ |
| `network.json` | ネットワークプロファイル（タイムアウト・リトライ・プロキシ・UA・レート） | `network/` 配下 |
| `filters.json` | フィルタ集合（結果フィルタ・本文フィルタ） | `filters/` 配下 |
| `quotas.json` | クォータポリシー（回数・量の上限） | `quotas/` 配下 |
| `engines/*.json` | エンジン定義 | `engines/` 配下 |

同梱アセット（`network/` `filters/` `quotas/` `engines/` の既定値）は、バイナリの隣の `assets/` を第一候補とし、無ければ実行時の作業ディレクトリから上方へたどって `assets/` を探し、それも無ければバイナリへ埋め込んだ既定値を使う。埋め込みは「配布物のステージ漏れ・部分コピー・入れ子コピーで既定値が消える」事故への保険であり、通常は隣の `assets/` が使われる（利用者が `assets/` を編集した場合にその編集が効くようにするため）。

### 1.2 `$ref` による分割

任意の JSON オブジェクト位置で `{"$ref": "./network/default.json"}` を記述すると、参照先ファイルの内容で置き換える。`$ref` は相対パス解決のみを許可し、リモート URL は許可しない（サンドボックスの通信制約と再現性のため）。循環参照は `E1003` として検出する。

### 1.3 スキーマ版

各設定ファイルは `schemaVersion`（整数）を持つ。初期リリースは `2`（JSON 化に伴う版）とし、未知の版は `E1004` を返す。

---

## 2. 優先順位とマージ

### 2.1 優先順位

同じ項目に複数の指定がある場合は次の順で優先する（上が強い）。

1. コマンドライン引数
2. 環境変数
3. 利用者設定（`settings.json`）
4. 既定値

### 2.2 マージ規則

- オブジェクトは再帰的にマージする（深いマージ）。
- 配列は置き換えとする（追記しない）。ただし `filter.*.rules` は `mode` が `append` のときに追記する。
- `null` を指定した場合は「未設定に戻す」ではなく「明示的な null」として扱い、検証で失敗させる。

### 2.3 探索順

1. `--config <path>` で明示指定（不在は `E1003`）
2. 環境変数 `LLMSEARCH_CONFIG`（不在は `E1003`）
3. 状態ディレクトリの `settings.json`（`<state.dir>/settings.json`）
4. 利用者設定ディレクトリの `settings.json`（`~/.config/llmsearch/settings.json` 等）

3 と 4 は下に行くほど弱く、1 と 2 は「そのファイルだけを使う」指定である。3・4 が両方存在する場合は 3 を先に読み、4 を深いマージで上書きする（結果として 3 が強い）。3・4 が存在しない場合は読み飛ばす。

読み込んだパスは `config show` と `--verbose` に表示する。ファイルが 1 つも無い場合は既定値で動作し、`config init` で雛形を生成できる。読み込みの対象は `settings.json` だけでなく、`settings.json` が `$ref` で参照するファイル、`network` / `filters` / `quota` の ID 参照で解決したファイル、エンジン定義を含む。

---

## 3. 全体設定（`settings.json`）

### 3.1 トップレベル

| キー | 型 | 既定値 | 内容 |
|---|---|---|---|
| `schemaVersion` | 整数 | `2` | スキーマ版 |
| `search` | オブジェクト | — | 検索の既定挙動（§3.2） |
| `network` | 文字列/オブジェクト | `"default"` | 使用するネットワークプロファイルの ID、またはインライン定義 |
| `filters` | 文字列/オブジェクト | `"default"` | 使用するフィルタ集合の ID、またはインライン定義 |
| `quota` | 文字列/オブジェクト | `"default"` | 使用するクォータポリシーの ID、またはインライン定義 |
| `context` | オブジェクト | — | 出力の予算とアウトライン（§3.3） |
| `cache` | オブジェクト | — | 文書キャッシュ（§3.4） |
| `history` | オブジェクト | — | 履歴（§3.5） |
| `session` | オブジェクト | — | セッション（§3.6） |
| `ui` | オブジェクト | — | 表示（§3.7） |
| `state` | オブジェクト | — | 状態ディレクトリ（§3.8） |

### 3.2 `search`

| キー | 型 | 既定値 | 内容 |
|---|---|---|---|
| `engine` | 文字列 | 未設定 | 既定エンジン（`id[@variant]`）。未設定時は同梱の `duckduckgo` |
| `limit` | 整数 | `7` | 1 ページの件数（1〜30） |
| `maxResults` | 整数 | `30` | 1 クエリの累積上限（1〜200） |
| `fallback` | 文字列配列 | `[]` | フォールバックするエンジン ID の列 |
| `merge` | 真偽値 | `false` | 複数エンジンの結果を統合する |
| `engines` | 文字列配列 | `[]` | `merge=true` のとき統合するエンジン ID |
| `snippetChars` | 整数 | `120` | スニペットの切り詰め |
| `titleChars` | 整数 | `100` | タイトルの切り詰め |
| `language` | 文字列 | `en` | 検索エンジンへ送る言語（§3.2.1） |

#### 3.2.1 `search.language`

検索エンジンへ送る言語を 1 か所で決める。表示言語（常に英語）とは独立である。

- 値は BCP 47 の言語タグ（`en`, `ja`, `de`, `pt-BR` など）を基本とする。大文字小文字は区別しない。
- 先頭の主言語タグは ISO 639 の 2〜3 文字の英字、続くサブタグは英数字 1〜8 文字の `-` 区切りとする。空文字・空白・上記に合わない値は `E1004`（許容範囲の提示）。
- 既定値は `en`。同梱のエンジン定義にもネットワークプロファイルにも言語の直書きを残さない。

反映先は次の 3 経路で、いずれも同じ値を使う。

1. **テンプレート変数 `{{language}}`**: エンジン定義のクエリ・ヘッダ・本文で使える。主言語タグ（`en`, `ja`）を渡す。
2. **`{{locale}}`**: 後方互換のため残し、`search.language` の値を渡す。旧 `defaults.locale` は廃止する（`engine-definitions.md` §2.1）。
3. **`Accept-Language` ヘッダ**: エンジン定義が `Accept-Language` を指定していない場合、`Accept-Language: <language>` を付す（`q` 値は付けない）。

ネットワークプロファイルの `acceptLanguage` は廃止する（`network.md` §2.1）。エンジンごとに異なる言語パラメータは定義側で表現する（Brave の `search_lang`、SearXNG の `language` など）。DuckDuckGo の `kl` は「地域と言語の固定対」（`jp-jp`, `us-en`）しか取れず言語単独では指定できないため、同梱定義では `kl` を書かず `Accept-Language` で言語を届ける（地域を寄せたい利用者は自前の定義で `kl` を書ける）。地域（`region`）は言語と独立で、従来どおり `defaults.region` で決める。

### 3.3 `context`

| キー | 型 | 既定値 | 内容 |
|---|---|---|---|
| `maxChars` | 整数 | `8000` | 1 回の出力上限（500〜100000） |
| `outlineDepth` | 整数 | `3` | アウトラインの既定深度（2〜4） |
| `includeChildren` | 真偽値 | `true` | 節に子見出しの本文を含める |
| `minSectionChars` | 整数 | `0` | 予約（現在は表示に影響しない） |

### 3.4 `cache`

| キー | 型 | 既定値 | 内容 |
|---|---|---|---|
| `enabled` | 真偽値 | `true` | 文書キャッシュを使う |
| `ttlHours` | 整数 | `24` | 文書キャッシュの有効期限 |
| `maxSizeMb` | 整数 | `512` | 状態ディレクトリの容量上限 |

### 3.5 `history`

| キー | 型 | 既定値 | 内容 |
|---|---|---|---|
| `enabled` | 真偽値 | `true` | 履歴を記録する |
| `retentionDays` | 整数 | `90` | 保持日数（0 は無期限） |
| `maxEntries` | 整数 | `50000` | 上限件数 |

`enabled` は値の読み込みと `config show` / `config explain` への表示だけを行い、履歴の記録停止には反映していない（未実装。`plans.md` §7）。

### 3.6 `session`

| キー | 型 | 既定値 | 内容 |
|---|---|---|---|
| `ttlDays` | 整数 | `7` | 保持日数の指定値。期限切れの自動削除は行わない |
| `maxSessions` | 整数 | `200` | 保持数の指定値。件数超過の自動削除は行わない |

セッションの削除は明示操作によってのみ行う。指定値は `config show` と `config explain` に現れるが、これらによる自動削除は行わない。

### 3.7 `ui`

| キー | 型 | 既定値 | 内容 |
|---|---|---|---|
| `color` | 文字列 | `auto` | `auto` / `always` / `never` |
| `pageSize` | 整数 | `7` | 対話メニューで一度に表示する件数（`search.limit` と独立） |

表示言語の設定は無い。メッセージは常に英語である（`errors.md` §4.2）。旧版の `ui.locale` は廃止した（同 §4.5）。

### 3.8 `state`

| キー | 型 | 既定値 | 内容 |
|---|---|---|---|
| `dir` | 文字列 | 空 | 状態ディレクトリ。空のとき既定の探索順に従う |

---

## 4. 環境変数

| 変数 | 対応する設定 | 備考 |
|---|---|---|
| `LLMSEARCH_CONFIG` | 設定ファイルのパス | §2.3 の探索順 2 番目 |
| `LLMSEARCH_STATE_DIR` | `state.dir` | |
| `LLMSEARCH_ENGINE` | `search.engine` | |
| `LLMSEARCH_NETWORK` | `network` | プロファイル ID |
| `LLMSEARCH_FILTERS` | `filters` | フィルタ集合 ID |
| `LLMSEARCH_QUOTA` | `quota` | ポリシー ID |
| `LLMSEARCH_NO_CACHE` | `cache.enabled` | `1` で無効（`0` で有効） |
| `LLMSEARCH_TIMEOUT_MS` | ネットワークプロファイルの `timeoutTotalMs` | 正の整数（ms） |
| `LLMSEARCH_USER_AGENT` | ネットワークプロファイルの `userAgent` | 空文字は `E1004` |
| `LLMSEARCH_ENGINES_DIR` | エンジン定義の追加探索ディレクトリ | パス区切りで複数指定可 |
| `LLMSEARCH_SEARCH_LIMIT` | `search.limit` | 整数 |
| `LLMSEARCH_SEARCH_MAX_RESULTS` | `search.maxResults` | 整数 |
| `LLMSEARCH_CONTEXT_MAX_CHARS` | `context.maxChars` | 整数 |
| `LLMSEARCH_CONTEXT_OUTLINE_DEPTH` | `context.outlineDepth` | 整数 |
| `LLMSEARCH_HISTORY_ENABLED` | `history.enabled` | `1` / `0` / `true` / `false` |
| `LLMSEARCH_SEARCH_LANGUAGE` | `search.language` | 言語タグ。不正値は `E1004` |
| `LLMSEARCH_UI_COLOR` | `ui.color` | `auto` / `always` / `never` |
| `NO_COLOR` | `ui.color` | 設定時は `never`。`LLMSEARCH_UI_COLOR` より強い |

`LLMSEARCH_UI_LOCALE` は廃止した。設定されていても無視する（`errors.md` §4.5）。

`LLMSEARCH_CONFIG` は「そのファイルだけを使う」指定であり、指定先が存在しない場合は `E1003` で停止する（黙って既定値へ落ちない）。数値・真偽値として解釈できない値は、その変数を無視せず `E1004` として扱い、キーのパス・指定値・許容範囲を示す。

認証情報の環境変数名はエンジン定義側で宣言する（`value_env`）。本ツールはその名前を列挙せず、値も保持しない。

---

## 5. キャッシュ

### 5.1 キャッシュ対象

1. 検索結果（キー: エンジン ID・定義ハッシュ・クエリ・位置・件数）
2. 文書（キー: 正規化 URL）

### 5.2 有効期限と無効化

`schemaVersion`、エンジン定義ハッシュ、クエリ、位置、件数がすべて一致した場合のみ再利用する。`--refresh` 指定時はキャッシュを無視する。`cache.enabled=false` の場合は読み書きを行わない。

### 5.3 検索結果をキャッシュする範囲

既定では検索結果をキャッシュしない（`cache.searchResults` の既定は `false`）。同一クエリの再実行はエンジン側の状態変化を反映したいため、明示的に有効化した場合のみ保存する。

---

## 6. 検証と未知キー

### 6.1 値の検証

範囲外の値・型不一致・列挙にない値は起動時に `E1004` として検出し、キーのパス（例: `context.maxChars`）、指定値、許容範囲を表示する。検証に失敗した場合は既定値で代替せず、処理を開始しない。

### 6.2 未知キーの温存

未知のキーは警告 `W1002` を出して処理を継続し、ファイルへ書き戻す際に保持する。オブジェクトの下位にある未知キー（`ui.locale` など）も同じ扱いとし、キーのパスを警告に含める。

### 6.3 参照の解決

`network`, `filters`, `quota` の値が文字列の場合は ID 参照として解決する。参照先が存在しない場合は `E1003`（解決不能）とし、候補 ID を列挙する。

### 6.4 破損した JSON

JSON の解析に失敗した場合、行・桁と理由を表示して終了する（`E1003`）。既定値での起動は行わない。JSON5 やコメントは受理しない（決定論性と機械検証のため）。

---

## 7. 表示と初期化

### 7.1 `config show`

有効な設定を、由来（`default` / `file` / `env` / `argv`）とともに表示する。対象は次のキーである。

- `settings.json` の全キー（`schemaVersion` を除く。`search.*`, `context.*`, `cache.*`, `history.*`, `session.*`, `ui.*`, `state.*`）
- `network` / `filters` / `quota`（解決後の ID）と、その解決元ファイル
- `search.engine`（解決後のエンジン）、`state.dir`（解決後の状態ディレクトリ）、`engines.dir`（エンジン定義の探索先）

由来は次の 4 種類とする。上位ほど強い（§2.1）。

| 由来 | 意味 |
|---|---|
| `argv` | コマンドライン引数（`--config` / `--engine` / `--network` / `--filters` / `--quota` / `--state-dir` / `--engines-dir`） |
| `env` | 環境変数（変数名を併記する） |
| `file` | 設定ファイル（パスを併記する。`$ref` の解決先も含む） |
| `default` | 同梱の既定値 |

キーごとに 1 行で「キー / 値 / 由来」を表示する。認証情報と `value_env` の参照先の値は表示しない（値の代わりに `***` とする）。`--json` では `values` の各要素に `key` / `value` / `origin` / `source` を持つ。

### 7.2 `config path`

次を 1 行 1 件で表示する。読み込んだファイルと、読み込まなかった候補（存在しない探索先）の双方を含み、状態（`[used]` / `[candidate]`）を付す。行の書式は `[used] <種別>    <パス>`（種別は `settings` / `network` / `filters` / `quotas` / `engines` / `state` / `history` / `counters` / `sessions` / `documents` / `robots`）。

1. `settings.json` の探索候補（`--config` / `LLMSEARCH_CONFIG` / 状態ディレクトリ / 利用者設定ディレクトリの順）
2. `network` / `filters` / `quota` の解決に使ったファイルと、その探索ディレクトリの候補
3. エンジン定義ディレクトリの探索候補（`--engines-dir` / `LLMSEARCH_ENGINES_DIR` / 状態ディレクトリ / 利用者設定ディレクトリ / 同梱）
4. 状態ディレクトリと、履歴・カウンタ・文書キャッシュ・セッション・robots キャッシュの実パス

### 7.3 `config init`

`settings.json`、`network/default.json`、`filters/default.json`、`quotas/default.json` の 4 ファイルの雛形を、状態ディレクトリ配下の対応する位置（`settings.json`, `network/`, `filters/`, `quotas/`）へ書き出す。既存ファイルがある場合はそのファイルだけを上書きせず、残りを書き出したうえで上書きしなかったパスを表示する。`--force` 指定時のみすべて上書きする。書き出す内容は同梱アセット（既定値）と同一とし、`schemaVersion` を含む。

### 7.4 `config validate`

全設定ファイル（エンジン定義を含む）を検証し、結果を `[ok]` / `[warn]` / `[fail]` で 1 行ずつ表示する。失敗が 1 件でもあれば終了コード `1`。

### 7.5 `config explain <key>`

指定したキーの最終的な値と、その由来を 1 行で表示する（例: `context.maxChars = 4000 (env: LLMSEARCH_CONTEXT_MAX_CHARS)`）。
