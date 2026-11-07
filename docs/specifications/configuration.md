# 仕様書: 設定と構成

本書は設定ファイルの形式、探索順、環境変数、既定値、検証と未知キーの扱いを定義する。

---

## 1. 設定の優先順位

### 1.1 優先順位

同じ項目に複数の指定がある場合は、次の順で優先する（上が強い）。

1. コマンドライン引数
2. 環境変数
3. 設定ファイル
4. 既定値

### 1.2 設定ファイルの探索順

1. `--config <path>` で明示指定されたファイル（存在しない場合はエラー `E1003`）
2. 環境変数 `LLMSEARCH_CONFIG` が指すファイル
3. 状態ディレクトリ直下の `config.toml`
4. `~/.config/llmsearch/config.toml`

複数が見つかった場合、上位の 1 つのみを読み込む。読み込んだファイルのパスは `config show` と `--verbose` の出力に含める。

### 1.3 形式

設定ファイルは TOML とする。ファイルが存在しない場合は既定値のみで動作し、初回起動時に状態ディレクトリへ既定値を書き出す。書き出しは `config init` でも行える。

---

## 2. 設定項目

### 2.1 全項目と既定値

| キー | 型 | 既定値 | 説明 |
|---|---|---|---|
| `search.provider` | 文字列 | `duckduckgo` | 既定プロバイダ |
| `search.limit` | 整数 | `7` | 1 ページの件数（1〜30） |
| `search.max_results` | 整数 | `30` | 1 クエリでの累積上限 |
| `search.min_interval_ms` | 整数 | `1000` | 同一ホストへの最小間隔 |
| `search.snippet_chars` | 整数 | `120` | スニペットの切り詰め文字数 |
| `search.searxng.base_url` | 文字列 | 空 | SearXNG インスタンスの URL |
| `search.searxng.engines` | 文字列 | 空 | カンマ区切りのエンジン指定 |
| `search.brave.api_key_env` | 文字列 | `BRAVE_API_KEY` | API キーを読む環境変数名 |
| `network.timeout_connect_ms` | 整数 | `5000` | 接続タイムアウト |
| `network.timeout_total_ms` | 整数 | `20000` | 全体タイムアウト |
| `network.max_bytes` | 整数 | `5242880` | 応答本文の上限 |
| `network.max_redirects` | 整数 | `5` | リダイレクト追従の上限 |
| `network.user_agent` | 文字列 | 既定の識別文字列 | User-Agent |
| `network.respect_robots` | 真偽値 | `true` | robots.txt を尊重する |
| `network.cache` | 真偽値 | `true` | 文書キャッシュを使う |
| `extract.engine` | 文字列 | `readability` | 本文抽出手法 |
| `markdown.heading_style` | 文字列 | `atx` | 見出しの形式（`atx` のみ） |
| `context.max_chars` | 整数 | `8000` | 1 回の出力上限 |
| `context.outline_depth` | 整数 | `3` | アウトラインの既定深度（2〜4） |
| `context.include_children` | 真偽値 | `true` | 節に子見出しを含める |
| `cache.ttl_hours` | 整数 | `24` | 文書キャッシュの有効期限 |
| `cache.max_size_mb` | 整数 | `512` | 状態ディレクトリの容量上限 |
| `history.retention_days` | 整数 | `90` | 履歴の保持日数（0 は無期限） |
| `history.max_entries` | 整数 | `50000` | 履歴の上限件数 |
| `session.ttl_days` | 整数 | `7` | セッションの保持日数 |
| `ui.color` | 文字列 | `auto` | `auto` / `always` / `never` |
| `ui.title_chars` | 整数 | `100` | タイトルの切り詰め |
| `ui.locale` | 文字列 | `ja` | メッセージの言語（`ja` / `en`） |
| `state.dir` | 文字列 | 空 | 状態ディレクトリ（空は既定の探索順に従う） |

### 2.2 環境変数

| 変数 | 対応する設定 |
|---|---|
| `LLMSEARCH_PROVIDER` | `search.provider` |
| `LLMSEARCH_STATE_DIR` | `state.dir` |
| `LLMSEARCH_CONFIG` | 設定ファイルのパス |
| `LLMSEARCH_TIMEOUT_MS` | `network.timeout_total_ms` |
| `LLMSEARCH_USER_AGENT` | `network.user_agent` |
| `LLMSEARCH_NO_CACHE` | `network.cache`（`1` で無効） |
| `BRAVE_API_KEY` | Brave の API キー（`search.brave.api_key_env` で変更可） |
| `NO_COLOR` | `ui.color`（設定時は `never`） |

### 2.3 秘匿情報

API キーを設定ファイルへ直接記述する項目は設けない。設定ファイルには環境変数名のみを記述する。`config show` は環境変数の内容を表示せず、設定済みかどうかのみを表示する。

---

## 3. 検証と未知キー

### 3.1 値の検証

範囲外の値、型の不一致、列挙にない値は、起動時にエラー `E1004` として検出し、項目名・指定値・許容範囲を表示する。検証に失敗した場合は既定値を用いず、処理を開始しない。

### 3.2 未知キーの温存

設定ファイルに未知のキーが含まれる場合、警告 `W1002` を表示するが処理は継続する。未知キーは設定ファイルへ書き戻す際に保持し、利用者が記述した内容を失わない。

### 3.3 プロバイダ固有設定の検証

選択されたプロバイダに必要な設定が不足している場合（SearXNG の `base_url`、Brave の API キー）は、そのプロバイダを選択した時点で `E4002` を返し、必要な設定項目を表示する。

### 3.4 破損した設定ファイル

TOML の解析に失敗した場合、行番号と理由を表示して終了する（`E1003`）。既定値での起動は行わない。

---

## 4. 表示と初期化

### 4.1 `config show`

有効な設定を、由来（既定値・設定ファイル・環境変数・引数）とともに表示する。秘匿情報は伏せる。

### 4.2 `config path`

読み込んだ設定ファイルのパスと状態ディレクトリのパスを表示する。設定ファイルが存在しない場合は、探索した候補パスを表示する。

### 4.3 `config init`

既定値の設定ファイルを書き出す。既存ファイルがある場合は上書きせず、`--force` 指定時のみ上書きする。書き出し後は、編集すべき主要項目をコメント付きで示す。
