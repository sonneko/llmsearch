# 仕様書: ネットワーク設定（JSON）

本書は HTTP の挙動を JSON で宣言するための仕様を定義する。ネットワークの挙動はコードへ埋め込まず、すべてプロファイルとして外部化する。

---

## 1. 基本方針

### 1.1 プロファイル方式

ネットワークの挙動は「プロファイル」と呼ぶ名前付きの JSON にまとめる。エンジン定義は `network` キーでプロファイル ID を参照し、文書取得は設定 `network` のプロファイルを用いる。プロファイルは検索と取得で共用できる。

### 1.2 プロファイルの選択順

1. コマンドラインの `--network <id>`
2. 環境変数 `LLMSEARCH_NETWORK`
3. 設定 `network`（ID またはインライン定義）
4. 既定プロファイル `default`

### 1.3 プロファイルの探索

`network/<id>.json` を次の順に探索する。同一 ID は先勝ち。

1. `--config` が指すファイルの兄弟ディレクトリ `network/`
2. 状態ディレクトリの `network/`
3. 利用者設定ディレクトリの `network/`
4. 同梱の既定プロファイル

---

## 2. プロファイルの構造

### 2.1 トップレベル

| キー | 必須 | 型 | 内容 |
|---|---|---|---|
| `schemaVersion` | 必須 | 整数 | 定義スキーマの版（初期リリースは `2`） |
| `id` | 必須 | 文字列 | プロファイル ID |
| `description` | 任意 | 文字列 | 説明 |
| `timeoutConnectMs` | 任意 | 整数 | 接続タイムアウト（既定 `5000`） |
| `timeoutTotalMs` | 任意 | 整数 | 全体タイムアウト（既定 `20000`） |
| `maxBytes` | 任意 | 整数 | 応答本文の上限（既定 `5242880`） |
| `maxRedirects` | 任意 | 整数 | リダイレクト追従の上限（既定 `5`） |
| `redirectPolicy` | 任意 | 文字列 | `follow`（既定） / `manual` / `never` |
| `userAgent` | 任意 | 文字列 | User-Agent。空は不可 |
| `headers` | 任意 | オブジェクト | 全リクエストに付す追加ヘッダ |
| `compression` | 任意 | 文字列配列 | 受理する圧縮方式（既定 `["gzip","deflate","br"]`） |
| `httpVersion` | 任意 | 文字列 | `auto`（既定） / `1.1` / `2` |
| `minIntervalMs` | 任意 | 整数 | 同一ホストへの最小間隔（既定 `1000`） |
| `maxConcurrency` | 任意 | 整数 | 同時接続の上限（既定 `2`、1〜8） |
| `retry` | 任意 | オブジェクト | 再試行の方針（§3） |
| `proxy` | 任意 | 文字列/オブジェクト | プロキシ設定（§4） |
| `tls` | 任意 | オブジェクト | TLS の検証方針（§5） |
| `robots` | 任意 | オブジェクト | robots.txt の扱い（§6） |
| `charset` | 任意 | 文字列 | 応答の既定文字符号化。省略時は自動判定 |
| `acceptLanguage` | 任意 | 文字列 | `Accept-Language` の値 |

### 2.2 値の範囲

`timeoutConnectMs` は 100〜60000、`timeoutTotalMs` は 500〜120000、`maxBytes` は 1024〜52428800、`maxRedirects` は 0〜10、`minIntervalMs` は 0〜60000。範囲外は `E1004`。

---

## 3. 再試行（`retry`）

| キー | 型 | 既定値 | 内容 |
|---|---|---|---|
| `enabled` | 真偽値 | `true` | 再試行を行う |
| `maxAttempts` | 整数 | `2` | 総試行回数（1〜5）。初回を含む |
| `backoffMs` | 整数 | `500` | 初回待機。以降は指数で増やす |
| `backoffFactor` | 数値 | `2.0` | 待機の倍率（1.0〜5.0） |
| `maxBackoffMs` | 整数 | `8000` | 待機の上限 |
| `jitterMs` | 整数 | `250` | 待機に加える揺らぎの上限 |
| `onStatus` | 整数配列 | `[429, 502, 503, 504]` | 再試行する HTTP ステータス |
| `onError` | 文字列配列 | `["timeout","connect"]` | 再試行するエラー種別 |
| `respectRetryAfter` | 真偽値 | `true` | `Retry-After` ヘッダを尊重する |

再試行は検索・取得の双方に適用する。`respectRetryAfter` が真で `Retry-After` がある場合、`maxBackoffMs` を超える待機が必要なら再試行せずに `E4003` を返す（長時間の沈黙を避ける）。

---

## 4. プロキシ（`proxy`）

| 形式 | 内容 |
|---|---|
| 文字列 | URL 形式（`http://host:port` 等） |
| オブジェクト | `{"url": ..., "noProxy": ["localhost", ".internal"], "auth": {"value_env": "PROXY_PASS"}}` |

- プロキシの認証情報は `value_env` で参照する。値を直接記述した場合は検証で警告 `W1004` を出し、履歴・表示には伏字化して扱う。
- `noProxy` の判定は「完全一致」「`.` で始まる場合は接尾辞一致」「`*` は全許可」とする。
- 環境変数 `HTTP_PROXY` / `HTTPS_PROXY` / `NO_PROXY` は、プロファイルに `proxy` が無い場合のみ参照する。

---

## 5. TLS（`tls`）

| キー | 型 | 既定値 | 内容 |
|---|---|---|---|
| `verify` | 真偽値 | `true` | 証明書を検証する |
| `caFile` | 文字列 | 空 | 追加の CA 証明書ファイル（PEM） |

`verify=false` は自己署名環境向けの逃げ道として許可するが、設定した場合は起動時に警告 `W1005` を表示し、`doctor` でも `[warn]` として扱う。ルート CA のストアを変更する機能は提供しない。

---

## 6. robots.txt（`robots`）

| キー | 型 | 既定値 | 内容 |
|---|---|---|---|
| `respect` | 真偽値 | `true` | robots.txt を尊重する |
| `ttlMinutes` | 整数 | `60` | 判定結果のキャッシュ保持時間 |
| `userAgentToken` | 文字列 | `llmsearch` | 判定に用いる User-Agent トークン |
| `onError` | 文字列 | `allow` | 取得失敗時の扱い（`allow` / `deny`） |
| `maxBytes` | 整数 | `524288` | robots.txt の読み込み上限 |

- `respect=false` に設定した場合、`doctor` は `[warn]` を出し、履歴の `fetch` レコードに `robotsSkipped: true` を記録する。
- `onError=deny` の場合、robots.txt が取得できないホストへの本文取得をすべて拒否する（`E2006`）。

---

## 7. ヘッダの合成順

リクエストヘッダは次の順に合成し、同名は後勝ちとする。

1. プロファイルの `headers`
2. プロファイルの `userAgent` / `acceptLanguage` / 圧縮
3. エンジン定義の `headers`
4. 本文取得時の `Accept`（HTML 優先）

禁止ヘッダ（`Host`, `Content-Length`, `Connection`, `Transfer-Encoding`）をプロファイルで指定した場合は `E1004` とする。

---

## 8. 受信の扱い

### 8.1 文字符号化

`Content-Type` の charset、プロファイルの `charset`、HTML の `<meta charset>`、本文先頭の `@charset` の順に判定する。判定できない場合は UTF-8 とみなし、置換文字が生じた場合は警告を出す。

### 8.2 サイズ超過

`maxBytes` を超えた場合、読み取りを打ち切って変換を継続し、`W3003` を記録する。

### 8.3 ステータスの扱い

- `2xx`: 正常。
- `3xx`: `redirectPolicy` に従う。`never` の場合は `E2003` として返す。
- `401` / `403`: `E4003`（制限・認証）として扱い、エンジン定義の `errorWhen` より優先する。
- `404` / `410`: `E2003`（取得不能）。
- `429`: `retry` の対象。最終的に失敗した場合は `E4003`。
- `5xx`: `retry` の対象。最終的に失敗した場合は `E2003`。

### 8.4 ログと秘匿情報

リクエスト URL のクエリのうち、`value_env` 由来の値は `***` に置換して表示する。ヘッダの `Authorization` 等の認証系も同様に伏字化する。
