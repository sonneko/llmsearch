# M17 秘匿機能ローカルスタブ検証・修正記録

## 1. 背景とスコープ

Muse のローカル実環境検証（`develop` `5db4708` / `0.1.2`、2026-11-26。記録: `reports/m16-validation.md`）で未確認として残された M5 秘匿機能の下記 2 項目を、OS 割り当てポートの `127.0.0.1` ループバック HTTP スタブと実バイナリ（`target/debug/llmsearch`）で検証し、見つかった秘匿漏れ・過剰変換を修正した。

- **G1**: URL userinfo（`http://user:pass123secret@127.0.0.1:<port>/...` 等）の秘匿（成功系を含む表示 URL・エラー・履歴・JSON 出力）
- **G2**: 送信・応答における秘匿ヘッダ、JSON body、form body の秘匿と、通常値への過剰マスク非発生

サンドボックス制約および指示に従い、外部の実サイトへのアクセスは一切行っていない。

---

## 2. 修正前の実バイナリ検証結果と起票 Issue

### 2.1 修正前から正常だった経路

- 標準的な `http://user:pass@127.0.0.1:<port>/...`（ユーザー名のみ・パスワードのみ・パーセントエンコードを含む）に対する `outline` / `read`（`--json`・テキスト双方）、直接検索ヒット URL、`history`（一覧・`export`・`history.jsonl`）では `http://***:***@...` へマスクされ、通常クエリ引数と本文テキストは維持されていた。
- `engines show`（`--json` / `--raw`）、`engines test --dry-run`、`network show` の定義・dry-run 表示では、`Authorization` / `Cookie` / `X-Api-Key` や JSON/form の `api_key` / `password` が `***` にマスクされていた。

### 2.2 実バイナリ＋ループバックスタブで再現した漏れ・不具合

#### Issue #50（G1: URL userinfo・埋込 URL・不正/非 HTTP URL・robots 拒否・相対リンク）

1. **`W4006` リダイレクト展開警告の埋込 URL 漏れ**:
   `http://127.0.0.1:<port>/l/?uddg=http%3A%2F%2Fwrapuser%3Awrappass321%40127.0.0.1%3A<port>%2Fok-doc%3Ftoken%3DWRAPTOK123%26normal%3Dkeep` において、展開後の `results[].url` はマスクされる一方、`W4006` 警告（JSON `warnings[]` および `stderr`）に `wrapuser`・`wrappass321`・`WRAPTOK123` が平文で出力された。
2. **不正 URL / 未展開テンプレート URL / 非 HTTP スキーム URL の userinfo 漏れ**:
   - `outline "http://badportuser:badportpass999@127.0.0.1:99999/doc?token=BADPORTTOK"`（`E2005`）で `Url::parse` が失敗するため `badportuser:badportpass999` と `BADPORTTOK` が JSON `details`・`stderr` に平文で露出した。
   - 既存セッション下での `outline "ftp://ftpuser:ftppass999@127.0.0.1/doc?token=FTPTOK"`（`E1002`）で `redact_display_text` が `http(s)://` のみ走査していたため `ftpuser:ftppass999` と `FTPTOK` が露出した。
3. **`robots.txt` 拒否エラー `E2006` の相対パス秘匿引数漏れ**:
   `outline "http://robouser:robopass999@127.0.0.1:<port>/disallowed?token=ROBOTTOK999&normal=keeprobo"`（`--network default`）で、`RobotsGate` がスキーム無しの相対パス `/disallowed?token=ROBOTTOK999&normal=keeprobo` を `details` に埋め込み、`ROBOTTOK999` が露出した。
4. **302 リダイレクト先 userinfo URL での相対リンク認証情報伝播と文字数ずれ**:
   `Location: http://rediruser:redirpass456@127.0.0.1:<port>/ok-doc` へリダイレクトされた文書内の `<a href="/rel-path">` に対し、`markdown::absolutize` が基底 URL の `rediruser:redirpass456@` を引き継ぎ、キャッシュ本文 `body.md` への混入と出力境界マスク後の `chars` 差分を生じさせていた。

#### Issue #51（G2: 応答ヘッダ・JSON body・form body 秘匿漏れと form テンプレート過剰変換）

1. **抽出不一致プレビュー（`W4004` / `E4004`）での応答 JSON body・form body・HTTP ヘッダ漏れ**:
   - 応答 JSON（`api_key` / `access_token` / `x_auth_token` / `password` / ネスト `Authorization`・`Cookie`）、応答 form body（`access_token=...&password=...`）、応答 HTTP ヘッダ行（`Authorization: Bearer ...` / `Set-Cookie: ...` / `X-Api-Key: ...`）が `E4004` `details` および `W4004` `warnings[]`（リトライ 2 回目で成功した `rc=0` の検索イベントを含む）に平文で出力された。
   - 応答 JSON の先頭パディングが 160 文字超の場合、`E4004` 用の 200 文字切り詰めが JSON 構造の途中で切断を起こし、部分秘密値が漏れる経路も確認した。
2. **応答ヘッダパラメータの漏れ（`E3002` / `Retry-After`）**:
   `Content-Type: application/pdf; token=CT_RESP_SECRET_930; charset=utf-8` に対する `E3002` `details` に `CT_RESP_SECRET_930` が平文で出力された。
3. **定義・ネットワークプロファイル内の平文秘匿ヘッダ値収集と `filters show` の `redact_json` 漏れ**:
   `request.headers` に平文文字列で書かれた `"Authorization": "Bearer ..."` 等や `NetworkProfile` の秘匿ヘッダ・`ProxyAuth::Plain` が実行時赤塗りリストに入らず、サーバエコー時に露出した。また `filters show` に `redact_json` が適用されていなかった。
4. **`engines show` における form body テンプレート変数の過剰変換**:
   `q={{query}}&api_key=...&normal=KEEP_FORM_BODY` が `form_urlencoded::Serializer` の再直列化で `q=%7B%7Bquery%7D%7D&api_key=***&normal=KEEP_FORM_BODY` に変質していた。

---

## 3. 仕様・設計・実装の要約（後方互換性維持）

1. **仕様・設計・計画の更新**:
   `docs/specifications/agent-integration.md` §4.2、`docs/specifications/document-fetching.md` §3.1、対応する `docs/designs/**`、および `docs/plans.md#m17-secret-redaction-validation` を順に更新した。JSON イベント構造・`v: 1`・既存 CLI オプション・通常値の出力仕様は変更しない。
2. **URL 秘匿化の強化（`src/core/url.rs`, `src/core/secret.rs`, `src/core/markdown.rs`）**:
   - クエリ値内のネスト URL（パーセントエンコードを含む）を再帰的に秘匿化（通常 URL は無変更）。
   - `redact_url_str` と `redact_display_text` で任意スキーム `scheme://...` および `Url::parse` 失敗トークンの `userinfo@`・秘匿パス/クエリ引数をマスクし、IPv6 `]` やテンプレート `{{...}}` の括弧バランスを保持。
   - `markdown::absolutize` で基底 URL から `username` / `password` を除去してから相対リンクを解決。
3. **ヘッダ・JSON body・form body 秘匿化と過剰マスク防止（`src/core/secret.rs`, `src/adapters/**`, `src/usecases/**`, `src/ui/**`）**:
   - `eval.rs` で 2048 / 200 文字切り詰め前に `redact_body` を適用。
   - `redact_body` で JSON body・form body（非秘匿セグメント `q={{query}}` 等は元文字列のまま無変換保持）・HTTP ヘッダ行・非構造テキスト中の埋込 JSON/key=value 断片を処理。
   - `redact_header_params` で `Content-Type` / `Retry-After` 内の `; token=...` 等の秘匿パラメータのみを `***` 化し、`charset=utf-8` 等を温存。
   - `App::redaction_list`（`redaction_list_for_app`）で環境変数・エンジン定義・ネットワークプロファイルの秘匿値を一元収集し、`engines::describe` 要約 URL の秘匿化と `filters show` の `redact_json` 適用を追加。

---

## 4. 修正後の検証結果

- **実プロセス回帰テスト（`tests/m17.rs`）**: 4 ケース全通過（OS 割り当て `127.0.0.1:0` ループバックスタブのみ使用、CI 外部依存なし）
  - `g1_userinfo_success_paths_outline_read_search_redirect_and_history_do_not_leak`: wire 上で Basic 認証が送信されることを確認したうえで、`outline` / `read`（URL・`D1`・`--json`・テキスト）/ 302・meta リダイレクト（相対リンク非伝播・`chars` 一致・`body.md` 検査）/ `search`（`W4006` 埋込 URL を含む）/ `history`（一覧・`export`・`history.jsonl`）に秘密値が 0 件、通常値 `normal=*` と本文 `NORMALBODYMARKER` が保持されることを確認。
  - `g1_malformed_non_http_and_robots_disallow_paths_redact_credentials_and_tokens`: ポート不正 `E2005`、非 HTTP `ftp://`（セッション無し `E5001`・既存セッション `E1002`）、接続拒否 `E2001`、`robots.txt` 拒否 `E2006` の stdout / stderr で認証情報・秘匿トークン露出 0 件、通常クエリ保持を確認。
  - `g2_request_headers_json_body_and_form_body_redaction_without_over_redaction`: POST JSON / POST form の両定義で wire 上に実秘密値が届くことを確認したうえで、`engines show`（要約・`--json`・`--raw`）/ `engines test --dry-run` / `engines test` / `network show` / `filters show` / `search` に秘密値露出 0 件、`q={{query}}`・`value_env`・`X-Normal`・通常 JSON `"value"`・通常 form 値が変質せず残ることを確認。
  - `g2_response_headers_json_body_and_form_body_leaks_redacted_in_errors_and_warnings`: 応答 JSON・リトライ成功 `W4004`・200 文字境界切断 JSON・応答 form・生 HTTP ヘッダ行・エコーヘッダ・`E3002` `Content-Type` パラメータのすべてで秘密値露出 0 件、通常フィールドと `charset=utf-8` の保持を確認。
- **品質ゲート**: `python3 docs/script/verify-docs.py`、`bash scripts/verify.sh --quick`（docs / release-gate / `cargo fmt --all --check` / `cargo clippy --locked --all-targets -- -D warnings` / `cargo test --locked --all-targets`）、`git diff --check` すべて通過。
- **残る境界**: 実在の外部検索プロバイダ・公開 Web サイトに対する userinfo URL 等の実通信挙動はサンドボックス外の対象であり、本 PR ではループバック HTTP スタブ＋実バイナリで検証している。
