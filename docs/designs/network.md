# 設計書: ネットワーク設定（JSON）

本書は `REF:docs/specifications/network.md` に対応する実装設計を記述する。

---

## 1. 基本方針

状態: 実装完了（REF:src/core/config/network.rs, REF:src/adapters/http.rs）

### 1.1 プロファイル方式

状態: 実装完了（REF:src/core/config/network.rs, REF:src/adapters/http.rs）

`core::config::network::NetworkProfile` が仕様の全キーを持つ。`adapters::http::reqwest_client` は `NetworkProfile` から `reqwest::Client` を組み立てる関数 `build_client(&NetworkProfile, &Secrets) -> Result<Client>` を提供し、プロファイル以外の設定（環境変数等）を読まない。

### 1.2 プロファイルの選択順

状態: 実装完了（REF:src/core/config/network.rs, REF:src/adapters/http.rs）

`usecases::config::network::select(opts, settings, store) -> Result<NetworkProfile>`。エンジン定義が `network` を持つ場合は、そのプロファイルを用いる（設定の既定より優先）。`--verbose` では「どのプロファイルがどの経路で選ばれたか」を表示する。

### 1.3 プロファイルの探索

状態: 実装完了（REF:src/core/config/network.rs, REF:src/adapters/http.rs）

`adapters::config::layout::network_dirs(config_path) -> Vec<PathBuf>` が 4 経路を返す。同梱既定は `assets/network/*.json` を `include_str!` で埋め込み、`&'static str` として保持する（実行時にファイルシステム上のリポジトリを参照しない）。

`usecases::config::bundled_root(env) -> PathBuf` は同梱アセットの根を次の順で決める（M11）。

1. `env.executable_dir()/assets` がディレクトリであればそこ（配布物を展開した配置）。
2. 無ければ `env.current_dir()` から上方へたどり、`assets/config/settings.json` を含む最初の `assets/`。完全なアセットだけを拾うため、`config/settings.json` の実在を条件にする（無関係な `assets/` を影として拾わない）。
3. どちらも無ければ空の `PathBuf` を返し、資源の解決は埋め込み既定へ落ちる。

隣の `assets/` は常に上方探索より強いので、配布物の中に `assets/` があれば祖先ディレクトリを探しに行かない（影のリスクは「隣に `assets/` が無く、かつ祖先に完全な `assets/` がある」場合に限られる）。`bundled_root` の結果は `config show` の `engines.dir` にそのまま現れるため、どの `assets/` を使ったかは実行で確認できる。

資源の解決（`adapters::config::resolve_network` / `resolve_filters` / `resolve_quota`）は、ディレクトリ探索で見つからなかった ID について `crate::assets::embedded_as::<T>(Resource::Network | Filters | Quota, id, ...)` へ落ちる。エンジン定義は `adapters::engine::loader::load_registry` が登録 0 件のときだけ `crate::assets::Resource::Engines::embedded()` を読む（ファイルが 1 件でも見つかればそれだけを使う＝利用者の定義を埋め込みで薄めない）。

`config show` の `source` は、埋め込みで解決した資源について `embedded` と表示する（`Origin::Default` かつ `source == "embedded"`）。ディレクトリ探索で見つかった場合は従来どおり実パスを表示する。

以前は候補 3（`env!("CARGO_MANIFEST_DIR")`）が最後の砦だった。ビルドした機械のソースパスがバイナリへ埋め込まれるため、CI のランナーでは偶然そのパスが存在して検査をすり抜け、配布物だけが壊れる状態を作っていた（M11 の原因）。埋め込み既定はこの経路を置き換える。

---

## 2. プロファイルの構造

状態: 実装完了（REF:src/core/config/network.rs, REF:src/adapters/http.rs）

### 2.1 トップレベル

状態: 実装完了（REF:src/core/config/network.rs, REF:src/adapters/http.rs）

`NetworkProfile` は `Default` 実装で仕様の既定値を返し、`#[serde(default)]` を各フィールドに付す。`headers` は `BTreeMap<String, String>`。`retry`, `proxy`, `tls`, `robots` はサブ構造体。

### 2.2 値の範囲

状態: 実装完了（REF:src/core/config/network.rs, REF:src/adapters/http.rs）

`NetworkProfile::validate() -> Vec<Diagnostic>` が範囲を検査する。範囲はマクロ `check_range!(name, value, min, max)` で記述し、キー名と範囲を 1 か所にまとめる。

---

## 3. 再試行（`retry`）

状態: 実装完了（REF:src/adapters/http.rs）

再試行は `src/adapters/http.rs` の `retry_with(policy, clock, f)` を汎用関数として実装する。

- 判定: `should_retry(policy, outcome) -> bool`（`Outcome` は `Status(u16)` か `Err(io_kind)`）。
- 待機: `backoff(policy, attempt) -> Duration`（`backoffMs * factor^(attempt-1)` を `maxBackoffMs` で頭打ちにし、`jitterMs` の範囲で疑似乱数を加える）。
- `Retry-After` は秒数と HTTP-date の両方を解釈する（`chrono` 使用）。解釈は `REF:src/core/retry.rs` の `parse_retry_after_ms(value, now)`（純粋関数）に閉じ、日付形式は `RFC 1123`（`%a, %d %b %Y %H:%M:%S GMT`）のみを受理する。解釈できない値は `None` とし、指数バックオフへ戻す。
- 待機時間は `REF:src/adapters/http.rs` の `send_blocking` が決める。`respectRetryAfter` が真で解釈できた場合はその値をそのまま待ち（揺らぎは加えない）、`maxBackoffMs` を超える場合は再試行を打ち切って `E4003` を返す。値の受け渡しはエラー詳細の文字列解析ではなく `AppError::retry_after_ms`（`REF:src/core/errors.rs`）で行う。
- `Retry-After` が付くのは `4xx` / `5xx` 応答だけなので、抽出結果 0 件の再送（`REF:src/adapters/engine/eval.rs`）には適用しない。
- 待機は `Clock::sleep` を呼ぶ。テストでは仮想時計で待機時間を検証する。
- 抽出結果が 0 件の応答に対する再試行は `REF:src/adapters/engine/eval.rs` の `evaluate` が行う（HTTP 層の再試行とは別系統）。`retry.enabled` が偽なら 1 回で打ち切り、再試行ごとに `W4004` を記録する。最終的に失敗した場合は応答の先頭 200 文字を `E4004` の `detail` に含める（本文全体は出さない）。
- `E4004` は `REF:src/ports/mod.rs` の `classify` で `FallbackClass::RetryOtherEngine` に分類し、フォールバック先が設定されていればそこでも試す。

---

## 4. プロキシ（`proxy`）

状態: 実装完了（REF:src/core/config/network.rs, REF:src/adapters/http.rs）

`REF:src/adapters/http.rs` の `build_client` は reqwest の暗黙プロキシ探索を `no_proxy()` で停止し、`Proxy::custom` を明示適用する。`REF:src/ports/system.rs` が HTTP/HTTPS 別の環境変数を解決し、プロファイル指定時は環境変数（NO_PROXY を含む）より優先する。判定は送信先ごとに行い、リダイレクトでも noProxy を再評価する。noProxy は完全一致・先頭ドットの接尾辞一致・`*` を扱う。

プロファイルに `proxy` がある場合は `in_no_proxy` が環境変数 `NO_PROXY` を読まない（`REF:src/ports/system.rs`）。文字列形式（`ProxySpec::Url`）は除外項目を持たないため空として扱い、除外はオブジェクト形式の `noProxy` だけで指定できる。これは「プロファイルの指定が環境変数より優先される」という仕様 §4 の帰結であり、意図した挙動である。

詳細設定の `auth` は既存の `proxy_password` で環境変数または直接値を解決し、プロキシ URL の password に設定する。ユーザー名は URL 側の値を用いる。環境変数欠落・不正 URL は認証なし接続や直接接続へフォールバックせず E1004 にする。エラーには認証値や生のプロキシ URL を載せない。`with_env` による環境注入と実ローカル擬似プロキシの回帰テストは `REF:tests/proxy.rs`。

---

## 5. TLS（`tls`）

状態: 実装完了（REF:src/core/config/network.rs, REF:src/adapters/http.rs）

`reqwest` の `rustls-tls` フィーチャを使用する。`verify=false` は `danger_accept_invalid_certs(true)` とし、その場合はルート証明書を一切読み込まない。

ルート証明書の読み込みは `REF:src/adapters/tls.rs` に閉じる。

- `caFile` は最優先。`Certificate::from_pem_bundle` で全証明書を取り出し、`add_root_certificate` で追加する。読み込み・解析の失敗は黙って無視せず `E2008` として返す。
- システムルートストアは `candidate_bundles()`（環境変数 `SSL_CERT_FILE` / `CURL_CA_BUNDLE` / `REQUESTS_CA_BUNDLE`、次に OS 既定の証明書束）から先勝ちで 1 か所、得られなければ `candidate_dirs()`（環境変数 `SSL_CERT_DIR`、次に OS 既定のディレクトリ）から読み込む。
- 環境変数はポート `Env` 経由で読み（テストで差し替え可能）、`std::env` を直接読まない。
- 解析できない証明書は 1 件ずつ切り捨て、束全体を失敗にしない。
- 読み込んだ証明書は `ReqwestClient` の `roots` に（由来・設定値をキーにして）保持し、`build_client` の呼び出しごとの再読み込みを避ける。

`verify=false` の警告 `W1005` は `build_client` の戻り値に `Diagnostic` を含める形で返し、呼び出し側が 1 回だけ表示する。

---

## 6. robots.txt（`robots`）

状態: 実装完了（REF:src/core/robots.rs）

解析は `src/core/robots.rs` の `Robots::parse(text, max_bytes) -> Robots` と `Robots::verdict(token, path) -> Verdict`（純粋ロジック）。

- `User-agent` の一致判定は大文字小文字を区別せず、トークンの部分一致（`llmsearch` は `llmsearch/0.1.0` に一致）とする。
- `*` のグループと自トークンのグループが両方ある場合は、より具体的な自トークンのグループを優先する（RFC 9309 に準拠）。
- `Allow` と `Disallow` が同一長で競合する場合は `Allow` を優先する。
- `onError=allow` は取得失敗時に全許可、`deny` は全拒否。

強制は `REF:src/adapters/robots.rs` の `RobotsGate` が行う（従来は `core::robots` に呼び出し元が無かった）。

- 呼び出しは `REF:src/adapters/extract/mod.rs` の `fetch_fresh` から、実際に HTTP を送る直前に行う。HTML 中継ページでリダイレクトする場合は、追従先についても判定する。本文キャッシュが有効な場合は `fetch_fresh` に到達しないため判定しない。
- `HttpFetcher` は `robots: Option<RobotsGate>` を持ち、`REF:src/usecases/document.rs` の `fetch` が（状態ディレクトリのパス・HTTP クライアント・時計・通知先を渡して）組み立てる。`None` のときは判定しない（テストと「判定不要な経路」のため）。
- robots.txt の URL は `<scheme>://<host>[:<port>]/robots.txt`。取得は `HttpClient::send_blocking` をそのまま使うため、プロファイルのプロキシ・TLS・レート制限・再試行が効く。取得自体はクォータを消費しない（消費はユースケース層しかしないため、アダプタからの取得は自動的に消費 0 となる）。
- 判定結果は `<state-dir>/cache/robots.json` に「オリジン → 取得時刻 + 解析結果」で保持し、`REF:src/adapters/state/mod.rs` の `write_atomic` で書く。読み込み時に `robots.ttlMinutes` を過ぎていれば破棄して取り直す。ファイルが壊れていれば空として扱う（取得を止めない）。
- 取得に失敗した場合、`onError=deny` は即座に `E2006`。`allow` は「規則なし」を同じ保持時間だけ保持し、同じホストの文書を取得するたびに取り直さないようにする。
- 判定に使うパスは `url.path()` に `?` とクエリを足したもの（RFC 9309）。
- `Crawl-delay` は解析するが強制しない（レート制限は `minIntervalMs` に一本化する）。

`respect=false` のときは判定もファイルの読み書きも行わず、履歴の `fetch` レコードに `robotsSkipped: true` を残す（既存の実装どおり）。

---

## 7. ヘッダの合成順

状態: 実装完了（REF:src/core/config/network.rs, REF:src/adapters/http.rs）

`core::net::headers::compose(profile, engine, accept) -> HeaderMap` が段階的な合成を 1 か所で行い、禁止ヘッダの検査（`E1004`）もここで行う。`search.language` の `Accept-Language` 補完はエンジン定義の段で行う（`REF:docs/designs/engine-definitions.md` §3.2）ため、プロファイルは `Accept-Language` を設定しない。`NetworkProfile.accept_language` フィールドと `adapters/http.rs` の `Accept-Language` 上書き分岐は M10 で削除した。旧プロファイルに `acceptLanguage` が残っている場合は未知キーとして `W1002` を出す。`reqwest::Client::builder().default_headers()` にはプロファイル分のみを設定し、エンジン固有ヘッダはリクエスト単位で付す（クライアントの再利用で混線しないようにするため）。

`3xx` の扱いは `REF:src/adapters/http.rs` の `send_once` で `status` を見て分岐する（`reqwest` の `Policy::none()` は `manual` と `never` で同じ動きになるため、分岐は自前で行う）。

- `never`: `E2003`（`reason = リダイレクトを許可しない`）。
- `manual`: `Location` が解決できれば `final_url` にそれを入れ、本文を空にして `Ok` を返す。解決できない場合は `E2004`。
- `follow`: `Policy::limited(maxRedirects)` に委ねる。回数超過は `reqwest` のエラー（`is_redirect`）として `E2004` になる。`Location` が無く追えない `3xx` が返った場合も `E2004` とする。

HTML 中継ページの追従（`<meta http-equiv="refresh">` と JS の location）は `REF:src/adapters/extract/mod.rs` の `fetch_fresh` が行う。試行回数は `profile.max_redirects` をそのまま上限とし、0 のときは 1 hop も追わない（従来の `max(1)` は `maxRedirects=0` を無視していた）。

---

## 8. 受信の扱い

状態: 実装完了（REF:src/core/config/network.rs, REF:src/adapters/http.rs）

### 8.1 文字符号化

状態: 実装完了（REF:src/core/config/network.rs, REF:src/adapters/http.rs）

`adapters::http::decode::decode_body(bytes, content_type, profile_charset, html_meta_charset) -> (String, Option<Diagnostic>)` が `encoding_rs` で復号する。順序は仕様のとおり。`U+FFFD` の出現を数え、1 件以上なら警告を返す。

### 8.2 サイズ超過

状態: 実装完了（REF:src/core/config/network.rs, REF:src/adapters/http.rs）

`bytes_stream()` を読みながら累積し、`maxBytes` を超えた時点で `W3003` を記録して打ち切る。打ち切った本文でも変換を試みる。

### 8.3 ステータスの扱い

状態: 実装完了（REF:src/core/config/network.rs, REF:src/adapters/http.rs）

`adapters::http::status::classify(status, context) -> StatusClass` が仕様の表に従って分類し、`usecases` 側が `AppError` へ写像する。`401/403` をエンジンの `errorWhen` より優先するため、`classify` は抽出より前に呼ぶ。

### 8.4 ログと秘匿情報

状態: 実装完了（REF:src/core/config/network.rs, REF:src/adapters/http.rs）

`redact::url_with_secrets(url, secret_values)` が `value_env` 由来の値を `***` に置換する。`SecretString` の保持する値と照合して置換するため、置換漏れが起きにくい。`Authorization` はヘッダ名で一律に伏字化する。
