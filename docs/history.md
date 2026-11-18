# 変更履歴・意思決定アーカイブ（History — 追記専用）

本書は過去の仕様改訂履歴、アーキテクチャ・仕様の意思決定ログ（「最初は X だったが Y に変更した」等の経緯）、および達成済みマイルストーンの記録を保持する**追記専用（append-only）**ファイルである。

---

## 0. `grep` による機械検索ガイド

本書を先頭から全文読まずに目的の経緯を特定するため、すべての記録エントリは次の固定ヘッダとタグ行を持つ。

- 見出し形式: `### [HIST:<CATEGORY>:<ID>] <YYYY-MM-DD> <要約タイトル>`
- タグ行形式: `[HIST-TAG: category=<CATEGORY> id=<ID> topics=<カンマ区切りキーワード>]`
- `<CATEGORY>` の種別:
  - `SPEC-REV`: 仕様書の版改訂履歴
  - `DECISION`: アーキテクチャ・個別設計判断ログ
  - `MILESTONE`: 完了した開発マイルストーンと PR・計測結果
  - `SPEC-FIX`: 仕様の要補正点と裁定アーカイブ
  - `FUTURE`: 将来機能提案の確認アーカイブ

よく使う検索コマンド例:
```bash
# 全エントリの目次と行番号を一覧する
grep -n '^### \[HIST:' docs/history.md

# 特定のカテゴリのみを抽出する
grep -n '\[HIST-TAG: category=DECISION' docs/history.md

# キーワードで検索する
grep -n -i '\[HIST-TAG:.*topics=.*<keyword>' docs/history.md
```

---

## 1. 初期セットアップ

### [HIST:MILESTONE:M0] 2026-10-19 プロジェクト初期セットアップ完了
[HIST-TAG: category=MILESTONE id=M0 topics=setup,template,docs]
- リポジトリテンプレートの作成
- ドキュメント管理規則の策定
- GitHub Actions CI の設定
- 検証スクリプトの作成

---

## 2. 仕様策定

### [HIST:DECISION:D001] 2026-11-07 技術スタックを TypeScript / Node.js に決定
[HIST-TAG: category=DECISION id=D001 topics=stack,typescript,node,npm,sandbox]
- 候補は TypeScript / Rust / Python を想定したが、Arena サンドボックスの通信ホワイトリストに `registry.npmjs.org` が含まれ、Rust はツールチェーン退避（`REF:dependencies/rust/README.md`）を要するため初期導入コストが高い。
- Node.js 22 が既に導入済みであり、`fetch` / `AbortSignal.timeout` により HTTP 依存を減らせる点を重視した。
- 代替案: 単一バイナリ配布を優先して Rust を採用する案は、本フェーズでは却下（導入待ち時間が仕様検討の妨げになるため）。

### [HIST:DECISION:D002] 2026-11-07 HTML 変換パイプラインを linkedom + readability + turndown に決定
[HIST-TAG: category=DECISION id=D002 topics=html,markdown,readability,turndown,linkedom]
- jsdom は依存が重くサンドボックスでの導入が遅いため、DOM 構築は `linkedom` を採用した。
- 本文抽出は `@mozilla/readability`、Markdown 化は `turndown` + `turndown-plugin-gfm`（表・取り消し線）とし、抽出失敗時は `<body>` 全体へフォールバックする方針とした。
- 画像・リンク記法（`[text](url)` 形式）は仕様書 `REF:docs/specifications/document-fetching.md` の禁止パターン検査（Markdown 相対リンク禁止）に抵触するため、仕様書の表からは除外し、文章で表現する形に修正した。

### [HIST:DECISION:D003] 2026-11-07 仕様書を 9 本に分割
[HIST-TAG: category=DECISION id=D003 topics=spec,docs,structure]
- 単一ファイルでは肥大化するため、CLI・検索・取得変換・コンテキスト生成・状態・履歴・設定・エージェント連携・エラー・テストの単位で分割した。
- `REF:docs/designs/` は同名ファイルで 1 対 1 に対応させ、見出しを完全一致させた。

### [HIST:MILESTONE:M1] 2026-11-07 仕様書・設計書の初版完成
[HIST-TAG: category=MILESTONE id=M1 topics=spec,design,review]
- `REF:docs/specifications/` に 9 本、`REF:docs/designs/` に 9 本を作成した。
- 設計書の `状態:` 行は実装前のため `未実装・実装計画済み（REF:docs/plans.md#m2-implement）` とし、`REF:src/...` 参照は実装後に切り替える。
- ユーザーレビューで承認が得られた時点で `REF:docs/plans.md#m1-spec` を完了とする。

---

## 3. 仕様改訂 v2（Rust / 全 JSON / エンジン非依存）

### [HIST:SPEC-REV:S001] 2026-11-07 技術スタックを TypeScript から Rust へ変更
[HIST-TAG: category=SPEC-REV id=S001 topics=stack,rust,typescript,decision]
- ユーザー指示により実装言語を Rust に変更した。理由は単一バイナリ配布・型安全性・決定論的な文字列処理を重視するため。
- 影響: `REF:docs/designs/` の全 15 本を Rust 前提（`serde` / `clap` / `reqwest` / `tokio` / `scraper` / `htmd`）へ書き換えた。
- 影響: 依存導入は `crates.io` へ到達できないため、`cargo vendor` によるツールチェーン退避（`REF:scripts/bootstrap-rust.sh`, `REF:.github/workflows/toolchain-export.yml`）を前提とする。
- 却下案: TypeScript 継続（npm レジストリへ直接到達でき導入は容易）は、ユーザー要件により取り下げた。

### [HIST:SPEC-REV:S002] 2026-11-07 検索エンジンを JSON 定義で宣言する方式へ変更
[HIST-TAG: category=SPEC-REV id=S002 topics=engine,json,declarative,independence]
- 当初はプロバイダごとに実装を持つ想定（`duckduckgo` / `searxng` / `brave` を `adapters/providers/` に実装）だったが、ユーザー指示により「検索エンジンおよびネットワークをすべて JSON で切り替え可能にし、各エンジンの形式に全く依存しない」方針へ変更した。
- 変更点: 廃止した旧仕様書（プロバイダ別の実装前提だった検索仕様）を、`REF:docs/specifications/engine-definitions.md`（リクエスト定義・抽出ルール・ページング・検証コマンド）と `REF:docs/specifications/search.md`（実行・正規化）の 2 本に分割した。
- 変更点: ネットワーク挙動を `REF:docs/specifications/network.md`（プロファイル JSON）へ外部化した。
- 帰結: コアはエンジン名による分岐を持たない。新しいエンジンは JSON の追加のみで利用でき、再ビルドを要しない。この不変条件を `tests/architecture.rs` で機械検査する。

### [HIST:DECISION:D004] 2026-11-07 フィルタとクォータを JSON で宣言する方針を追加
[HIST-TAG: category=DECISION id=D004 topics=filter,quota,config,json]
- ユーザー指示「フィルタリング機能・検索回数制限なども設定できるようにする」に対し、フィルタは「結果フィルタ」「本文フィルタ」の 2 種を `REF:docs/specifications/filters.md` に、回数制限は「レート制限」と「クォータ」を分離して `REF:docs/specifications/quotas.md` に定義した。
- 設計判断: レート制限（瞬間的な頻度）とクォータ（総量）は目的が異なるため、判定を独立させた。片方が超過していれば送信しない。
- 設計判断: クォータの消費は「送信成功後」に行い、失敗時は消費しない（再試行の余地を残すため）。再試行の各試行は 1 回として数える。
- 設計判断: 日次クォータの区切りは端末のローカルタイムゾーンの 0 時とした（利用者の体感と一致させるため）。

### [HIST:DECISION:D005] 2026-11-07 設定形式を TOML から JSON へ統一
[HIST-TAG: category=DECISION id=D005 topics=config,json,toml,merge,ref]
- 当初は TOML を予定していたが、ユーザー指示によりすべて JSON とした。エンジン定義・ネットワーク・フィルタ・クォータ・全体設定の 5 種を同一の構文・マージ規則で扱える利点を重視した。
- `$ref` による分割参照を追加（相対パスのみ許可、リモート URL と循環参照は禁止）。設定の再利用と差分管理を容易にするため。
- 未知キーの温存は `serde(flatten)` で実装する方針とし、後方互換性を確保する。

### [HIST:DECISION:D006] 2026-11-07 本文抽出は密度推定の自前実装を第一候補とする
[HIST-TAG: category=DECISION id=D006 topics=extract,readability,markdown,htmd]
- `@mozilla/readability` 相当の処理は Rust では `readability` クレートが候補だが、`scraper` との型の相性に不確実性がある。
- 方針: `core/extract/` に密度推定（段落数・テキスト密度・リンク密度）を純粋関数として実装し、外部クレートの挙動に依存しない。Markdown 化は `htmd` を用いる。
- 影響: 抽出失敗の判定（`W3001` / `W3002`）を自前で行えるため、フォールバックの制御が明確になる。

### [HIST:MILESTONE:M002] 2026-11-09 M2（コア実装）完了
[HIST-TAG: category=MILESTONE id=M002 topics=implement,ports,core,adapters,usecases,ui,tests]

- ポート（`REF:src/ports/`）→ `core`（純粋ロジック）→ アダプタ（HTTP・取得・設定・エンジン評価・状態保存）→ ユースケース → CLI・対話 UI の順に実装し、M2 の全項目を完了した。
- 検証: `cargo test --locked --offline` で lib 111 / architecture 9 / assets 4 / contract 8 / integration 13 が通過。`cargo clippy --all-targets -- -D warnings` も通過。
- 設計判断: セッション単位のクォータは CLI 呼び出しをまたぐため、カウンタに `session_key` を持たせ、`Guard` がキーの変化時にセッション枠のみを初期化する方式にした。
- 設計判断: 警告コードの発行元を仕様（`REF:docs/specifications/errors.md` §3）どおりに固定し、`ExtractOutcome` / `Converted` / `FetchedDocument` の警告をコード付き（`Diagnostic`）に変えた。文字列だと受け取る層がコードを推測することになり、`W4001` と `W4003` の取り違えが起きたため。
- 設計判断: 文書の初回登録は警告ではないため `W4001` を出さない。`W4001` は「抽出に失敗した結果を破棄した」場合に限る。

### [HIST:SPEC-FIX:F003] 2026-11-09 警告コード W4002 / W4005 / W4006 の整理
[HIST-TAG: category=SPEC-FIX id=F003 topics=warning,errors,spec,drift]

- 仕様と実装で警告コードの意味が食い違っていたため、仕様書の表を正として実装を合わせた。
- `W4002`: 「エンジン定義の重複を無視した」。発行元は定義読込（`REF:src/adapters/engine/loader.rs`）。
- `W4005`: 「マージ時に重複 URL を除去した」。発行元は検索の正規化（`REF:src/usecases/search.rs`）。
- `W4006`: 仕様書の表に存在しない。実装でも使用していないため、コード表に追加しない。
- あわせて `W1001`（端末でないのに装飾を要求された）と `W2001`（レート制限の待機）を実際に発行するようにし、`W3004`（再取得で見出し数が変化）を追加した。宣言だけされて発行されないコードは、利用者にとって存在しないのと同じであるため。
- この不変条件は `REF:tests/architecture.rs` の `warning_codes_are_emitted_where_spec_says` で機械検査する。

### [HIST:DECISION:D007] 2026-11-09 配布は Linux x86_64 の単一バイナリ 1 種類とする
[HIST-TAG: category=DECISION id=D007 topics=release,ci,dist,binary]

- ユーザー指示により、リリース時にビルドする成果物は Linux x86_64 の実行ファイル 1 種類のみとした。
- 配布用プロファイル `release-dist`（`REF:Cargo.toml`）で LTO と `strip` を有効にし、約 9.4 MiB の単体ファイルにする。`panic = "abort"` も指定し、実行時サイズを抑える。
- リリースは `REF:.github/workflows/release.yml` がタグ push（`v*`）で実行し、`llmsearch-<version>-x86_64-unknown-linux-gnu.tar.gz` と SHA256 を添付する。リリースノートは GitHub が自動生成する。
- 却下案: 複数 OS・複数アーキテクチャのマトリクスビルドは、現時点で必要とされていないため見送った。追加する場合は同じワークフローに `strategy.matrix` を足すだけで済む。
- 削除: 用途のない Pages デプロイ用ワークフロー（旧 `deploy.yml`）は誤って走る余地があるため削除した。

### [HIST:MILESTONE:M003] 2026-11-09 M3・M4 完了と v0.1.0 の公開
[HIST-TAG: category=MILESTONE id=M003 topics=release,m3,m4,merge,tag,v0.1.0]

- PR #1 を `main` へマージし、`v0.1.0` をタグ付けして公開した。成果物は `llmsearch-v0.1.0-x86_64-unknown-linux-gnu.tar.gz`（3.9 MiB）と SHA256。
- リリースワークフローはタグ push で Build → Package → Smoke test（`--version` / `engines list` / `config show`）→ Upload を実行し、すべて成功した。
- M3 の残項目だった容量管理・保持期間・E2E を実装し、`tests/e2e.rs` を追加した（実ネットワークは `LLMSEARCH_E2E_LIVE=1` のときだけ。ユーザー指示により今回は未実施）。
- 教訓: サンドボックス再起動で `/home/user/build` が消えると `cargo` も消える。`git` の参照も巻き戻る場合があるため、作業前に `git fetch` してから `git reset --hard origin/<branch>` で合わせる（今回は退避したコミットを `cherry-pick` で載せ直した）。

### [HIST:SPEC-FIX:F004] 2026-11-09 検索結果 URL がエンジンのリダイレクト URL のままになる問題を修正
[HIST-TAG: category=SPEC-FIX id=F004 topics=redirect,extract,search,fetch,url]

- 症状: 検索結果の URL が `//<engine>/l/?uddg=<実 URL>` のようなエンジン側のリダイレクト URL のまま出て、`read` しても検索結果の中身を読めなかった。
- 原因 1: `unwrap_redirect` が生の `href` を `url::parse` していたため、プロトコル相対（`//host/...`）の URL を解釈できず、変換が黙って無効化されていた。`Transform::UnwrapRedirect` が基準 URL（`base_url`）を受け取るようにし、相対・プロトコル相対を絶対化してから解釈するようにした。値の二重エンコードと、リダイレクトがリダイレクトを包む入れ子（2 段）も解く。
- 原因 2: エンジン定義の書き漏らし（`unwrap_redirect` の指定漏れ）があると、抽出段階では防げなかった。`core/search.rs` の `unwrap_redirects` で全結果 URL を正規化時に再検査し、展開した場合は `W4006` を出す。判定は `core/url.rs` の `target_of_redirect`（既定キー）。
- 原因 3: 取得時、中継ページが HTTP の 3xx ではなく `<meta http-equiv="refresh">` やスクリプトの location 遷移で返ると、`read` が中継ページを「記事」として読んでいた。`adapters/extract` が `html_redirect_target` で検出し、`maxRedirects` を上限にたどり直す。
- 仕様: `REF:docs/specifications/engine-definitions.md` §4.7（リダイレクト URL の展開）を追加し、`W4006` を `REF:docs/specifications/errors.md` の警告一覧へ追加した。
- 検査: プロトコル相対・入れ子・二重エンコードの展開（`src/core/extract.rs`）、`W4006` の発行元（`tests/architecture.rs`）、中継ページの追従（`tests/integration.rs`）、同梱 HTML エンジンが `unwrap_redirect` を持つこと（`tests/contract.rs`）。

### [HIST:DECISION:D008] 2026-11-14 HTTPS の検証にシステムの CA ストアを使う
[HIST-TAG: category=DECISION id=D008 topics=tls,ca,network,https,security]

- 症状: 実運用環境（Muse）で `tls.caFile` を指定しない限り、すべての HTTPS 取得が `E2001` になった。原因は実装ではなく環境で、その VM は TLS を MITM しており、curl は `CURL_CA_BUNDLE` / `SSL_CERT_FILE` でその CA を信頼しているのに、llmsearch の reqwest（`rustls-tls`）は同梱の webpki-roots しか見ていなかった。`main` でも同じ挙動で、M5 の不具合ではない。
- 対応: `REF:src/adapters/tls.rs` を追加し、`tls.verify` が真のときだけシステムのルート証明書を読んで reqwest に追加する。参照順は `tls.caFile` → `SSL_CERT_FILE` / `CURL_CA_BUNDLE` / `REQUESTS_CA_BUNDLE` → 代表的なシステムバンドル（`/etc/ssl/certs/ca-certificates.crt` 等）→ システムの証明書ディレクトリ（`SSL_CERT_DIR` / `/etc/ssl/certs`）。
- 設計判断: `rustls-native-certs` は vendored 依存に無く（サンドボックスは crates.io へ到達できない）、追加できないため PEM / DER の読み込みを自前で実装した。読んだ証明書はプロファイルと設定値をキーにキャッシュし、送信のたびに再読込しない。
- 設計判断: `tls.caFile` は従来どおり最優先とし、読めない場合は `E2008` で停止する（黙って検証を緩めない）。システムストアの読み取り失敗は環境差が大きいため警告にとどめ、従来どおり webpki-roots で検証する。
- 設計判断: `tls.verify = false` のときはシステムストアを読まない（検証しない設定でわざわざ信頼点を増やさない）。
- 検査: ローカルのプライベート CA を使う HTTPS サーバに対する `REF:tests/tls.rs`（7 件）と、実バイナリでの 7 ケース（信頼・拒否・`caFile` 優先・`E2008`・キャッシュ・`verify=false`）。

### [HIST:SPEC-FIX:F005] 2026-11-14 抽出結果が 0 件の応答を再試行する
[HIST-TAG: category=SPEC-FIX id=F005 topics=retry,extract,e4004,w4004,spec,drift]

- 症状: `--json search` が一度だけ `E4004`（抽出規則に一致する要素が 1 件も無い）を出し、直後のテキスト再実行では成功した。
- 判断: 送出側の一時的な揺れ（中継ページ・版面の一部違い）として扱い、`REF:src/adapters/engine/eval.rs` の `evaluate` が同じ送信を `retry.maxAttempts` まで繰り返す。HTTP 層の再試行とは別系統で、応答は成功している場合にだけ動く。範囲は仕様 `network.md` §3 の `maxAttempts`（1〜5）に合わせた。
- 再試行のたびに `W4004` を記録し、成功時は結果の警告に、失敗時は `E4004` の `warnings` に含める（警告が最終的なエラーに吸われて消えないようにするため）。
- 失敗時の `detail` には応答の先頭 200 文字だけを入れ、本文全体は出さない。
- 仕様: `REF:docs/specifications/network.md` §3、`REF:docs/specifications/agent-integration.md` §3.3。
- 検査: `REF:tests/m6.rs`（「応答が揺れる」偽 HTTP で試行回数・成功時の警告・失敗時の `E4004`・`enabled=false` で再試行しないことを確認）。

### [HIST:SPEC-FIX:F006] 2026-11-14 `--json` の全コマンド統一と秘匿情報の伏字化
[HIST-TAG: category=SPEC-FIX id=F006 topics=json,cli,secret,errors,e5001,drift]

- `Renderer::envelope()`（`REF:src/ui/render.rs`）を追加し、全イベントの共通枠（`v` / `event` / `session` / `at`）を 1 か所で付与するようにした。従来はハンドラごとに書いており、書き漏らしが起きていた（`REF:docs/specifications/agent-integration.md` §2.1）。
- `engines` / `engine` / `network` / `filters` / `config` / `quota` / `history` の各サブコマンドが `--json` でイベントを出すようにした。生の設定 JSON を出していた `network show` / `filters show` / `history export` も封筒に入れる（`REF:docs/specifications/agent-integration.md` §2.2）。
- `error` イベントに `warnings` を追加。`AppError` が警告を持てるようにし、失敗に至る過程の警告を失わないようにした。
- 秘匿情報: 送信エラーの文字列（`E2001` / `E2002` / `E2004` の `details`）、`engines show` の定義値（`TmplValue::Explicit` と URL の秘匿パラメータ）、検索結果の表示 URL を伏せるようにした（`REF:src/core/secret.rs`）。`--dry-run` のヘッダも認証・Cookie 類を伏せる。取得用の URL はセッション内の実体をそのまま保持し、表示だけを伏せる。
- セッション不在時の識別子解決を `E5001` に統一した（`REF:src/ui/commands.rs` の `resolve_target`）。従来は暗黙にセッションを作ってから `E1006` を返していた。URL を直接指定した場合は従来どおり取得できる。
- `AppError::hint` を実際に表示に使い、`AppError::message()` が `ui.locale` を反映するようにした。
- `E2007` は初期リリースでは発行しないことを `REF:docs/specifications/errors.md` §2.2 に明記した（応答サイズ超過は打ち切り + `W3003`）。
- 検査: `REF:tests/m6.rs`（9 件）。

### [HIST:SPEC-FIX:F007] 2026-11-14 robots.txt を実際に強制する
[HIST-TAG: category=SPEC-FIX id=F007 topics=robots,e2006,fetch,cache,quota,issues-13]

- 症状: `REF:src/core/robots.rs` に解析ロジックがあるだけで呼び出し元が無く、`Disallow` のページも取得できていた（Issue #13）。`E2006` も発行されなかった。
- 対応: `REF:src/adapters/robots.rs` の `RobotsGate` を追加し、本文取得の前に「オリジン単位（`scheme://host[:port]`）」で robots.txt を判定する。拒否された場合は `E2006`。取得は `HttpClient::send_blocking` をそのまま使うため、プロファイルのプロキシ・TLS・レート制限・再試行が効く。
- 設計判断: 判定結果は `<state-dir>/cache/robots.json` に「オリジン → 取得時刻 + 解析結果」で保持し、`network.robots.ttlMinutes` の間は再利用する（同じホストの文書ごとに取り直さない）。取得に失敗した場合、`onError=allow` は「規則なし」を同じ保持時間だけ保持する。ファイルが壊れていれば空として扱い、取得を止めない。
- 設計判断: robots.txt の取得はクォータを消費しない。消費はユースケース層しかしないため、アダプタからの取得は自動的に 0 となる。
- 設計判断: `E2006` は `classify()` で `Fatal` のままとし、robots の拒否で別エンジンへフォールバックしない（拒否は取得先の問題であり、検索エンジンの問題ではない）。
- 設計判断: `respect=false` のときは判定もファイルの読み書きも行わず、履歴の `fetch` レコードに `robotsSkipped: true` を残す。`doctor` は `[warn]` を出す（従来どおり）。
- 副次修正: `REF:src/usecases/document.rs` の `load()` は「保存された本文」を読み直していたため、`respect=false` や `--no-cache`（保存しない経路）で必ず `E3001` になっていた。取得した本文を `Fetched.markdown` として持ち、保存の有無に関わらずその場で使うようにした（保存しない方針は変えない）。
- 仕様: `REF:docs/specifications/network.md` §6、`REF:docs/specifications/search.md` §5.3、`REF:docs/specifications/document-fetching.md` §1。
- 検査: `REF:tests/m7.rs`（拒否・取得成功・`onError=deny`・`respect=false`・キャッシュ再利用・クォータ非消費）と実バイナリでの 6 ケース。

### [HIST:SPEC-FIX:F008] 2026-11-14 `Retry-After` を再試行の待機時間に使う
[HIST-TAG: category=SPEC-FIX id=F008 topics=retry,retry-after,e4003,http,spec,issues-15]

- 症状: `Retry-After` はエラー詳細の文字列に混ぜて表示するだけで、待機時間には使われていなかった（Issue #15）。
- 対応: `REF:src/core/retry.rs` の `parse_retry_after_ms(value, now)`（純粋関数）で秒数と HTTP 日付（`RFC 1123`）を解釈し、`AppError::retry_after_ms` として運ぶ。`respectRetryAfter` が真で解釈できた場合はその値をそのまま待つ（揺らぎは加えない）。`maxBackoffMs` を超える場合は再試行せず `E4003` を返す。解釈できない値は指数バックオフへ戻す。
- 副次修正: 再試行の可否判定（`REF:src/adapters/http.rs` の `retryable`）が状態番号をエラー詳細の文字列から解析していたため、`Retry-After` が付いた応答（`HTTP 503（Retry-After: 1）`）が再試行されなかった。`AppError::status` に状態番号を持たせ、詳細文の解析に頼らないようにした。
- 設計判断: `Retry-After` が付くのは `4xx` / `5xx` 応答だけなので、抽出結果 0 件の再送（F005）には適用しない。
- 仕様: `REF:docs/specifications/network.md` §3。
- 検査: `REF:tests/m7.rs`（上限内で待機・上限超過で `E4003` ・`respectRetryAfter=false` ・解釈不能）と実バイナリでの 3 ケース（待機 1.01 秒 / 打ち切り / 無視して 0.11 秒）。

### [HIST:SPEC-FIX:F009] 2026-11-14 `redirectPolicy` ごとの 3xx の扱い
[HIST-TAG: category=SPEC-FIX id=F009 topics=redirect,3xx,e2003,e2004,http,issues-17]

- 症状: `send_once` が `status >= 400` でしかエラーにしないため、`never` でも 3xx が成功扱いになり（本文は空）、`manual` の `Location` も使われず、`maxRedirects` の `0` も HTML 中継ページの追従で無視されていた（Issue #17）。
- 対応: 3xx を `redirectPolicy` で分岐する。`never` は `E2003`（`reason = リダイレクトを許可しない`）。`manual` は `Location` を解決して `final_url` に入れ、本文を空にして `Ok` を返す（追うかどうかは呼び出し側の判断）。`follow` で追いきれなかった 3xx は `E2004`。`Location` が解決できない `manual` も `E2004`。
- 対応: HTML 中継ページの追従上限を `max_redirects.max(1)` から `max_redirects` に戻し、`0` のときは 1 hop も追わないようにした。
- 設計判断: `manual` は「追わないが行き先は返す」とした。3xx を握りつぶすと `outline` / `read` が空本文に対して「JavaScript 依存」の警告を出すだけになり、利用者が気付けないため。
- 仕様: `REF:docs/specifications/network.md` §8.3。
- 検査: `REF:tests/m7.rs`（`never` で `E2003` / `never` で 2xx は成功 / `manual` で `Location` を返す / `maxRedirects=0`）と実バイナリでの 4 ケース（`follow` で追従・`never` で `E2003` ・`manual` で空本文・`maxRedirects=0` で `E2004`）。

### [HIST:SPEC-FIX:F010] 2026-11-14 クォータの消費を仕様どおりに数える
[HIST-TAG: category=SPEC-FIX id=F010 topics=quota,metrics,chars,results,fallback,issues-18]

- 症状: 消費が仕様とずれていた（Issue #18）。`fetch` が取得した本文の文字数を `chars` に加算していた。検索の `results` がフィルタ後の `page_hits.len()` だった。フォールバックは成功した 1 回分しか `searches` を消費しなかった。
- 対応: `REF:src/usecases/document.rs` の `fetch` は `fetches` のみを消費する（`chars` は `read` が出力した分だけ）。`REF:src/usecases/search.rs` の `run` は `results` にフィルタ前の取得件数を入れ、`searches` に「送信した回数」（フォールバックで試したエンジン数 + 同一エンジン内の再送回数）を積む。
- 設計判断: 消費は `pending` に積み、**操作が成功したときだけ** `Guard::consume_all` で永続化する。全ての候補が失敗した場合は `pending` を捨てるため消費は 0 のままになる（仕様 `quotas.md` §3.2）。次に試すエンジンの判定は `guard.check(Searches, pending.searches + 1)` のように「既に送った分 + これから送る 1 回」で行う。
- 仕様: `REF:docs/specifications/quotas.md` §2.2 / §3.1 / §3.2。`history show` は本文を再出力しない（メタデータと記録のみ）ため消費しないことを明記した（従来の「`chars` のみ加算」は実装と矛盾していた）。
- 検査: `REF:tests/m7.rs`（`outline` で `chars` を消費しない / `read` で消費する / フィルタ前の件数 / フォールバックで 2 回 / 全滅で 0 回）と実バイナリでの 3 ケース（`outline` で `fetches=1, chars=0` → `read` で `chars=46` → 追加の `read` で `chars=91`）。
