# 開発計画（Plans）

本書は現在の開発状況サマリ、実装計画、マイルストーンを記述する。完了済みの経緯・決定ログは `REF:docs/history.md`、固定の開発者情報は `REF:docs/developers.md` を参照すること。

---

## 1. 全体進捗サマリ

<!-- SYNC:START project-status -->
| 区分 | 状態 | 対象範囲 | 参照 |
|---|---|---|---|
| 初期マイルストーン | 完了 | プロジェクトセットアップ・ドキュメント管理規則の策定 | `REF:docs/history.md` |
| 仕様策定（M1） | 改訂中 | Rust・全 JSON 設定・エンジン非依存への仕様改訂（v2） | `REF:docs/specifications/engine-definitions.md` |
| 実装（M2） | 完了 | ポート・`core`・アダプタ・ユースケース・UI・同梱アセット | `REF:docs/plans.md#m2-implement` |
| 品質強化（M3） | 完了 | アーキテクチャ検査・契約テスト・結合テスト・容量管理 | `REF:docs/plans.md#m3-hardening` |
| リリース準備（M4） | 完了 | README・開発者ガイド・CI・v0.1.0 のタグ付けとリリース | `REF:docs/plans.md#m4-release` |
| 使いやすさ・安全性（M5） | 完了 | Issue 洗い出し・CLI/JSON/信頼境界・プロキシ・配布互換性 | `REF:docs/plans.md#m5-usability-safety` |
| 実運用と仕様同期（M6） | 完了 | システム CA ルート・E4004 耐性・M5 残 Issue の解消・ドキュメント同期（PR #28） | `REF:docs/plans.md#m6-dogfood-sync` |
| 取得の作法と計測（M7） | 完了 | robots.txt の強制（#13）・`Retry-After`（#15）・`redirectPolicy` の 3xx（#17）・クォータ消費のずれ（#18） | `REF:docs/plans.md#m7-fetch-discipline` |
| 設定・監査・統合の適合（M8） | 完了 | `config show/path/init`（#11）・`quota reset` の履歴（#19）・設定探索順と環境変数（#20）・`search.merge` の統合（#22） | `REF:docs/plans.md#m8-config-quota-merge` |
| JSON・秘匿情報・ドキュメント（M9） | 完了 | `--json` の統一（#4/#5）・`refine` と対象解決（#7/#9）・表示 URL の伏字化（#12）・ドキュメント整理（#21/#23）（PR #32） | `REF:docs/plans.md#m9-json-and-docs` |
| 表示言語とエンジン言語（M10） | 完了 | CLI メッセージの全英語化・嘘をつく `ui.locale` の削除・`search.language` の追加 | `REF:docs/plans.md#m10-english-and-language` |
<!-- SYNC:END project-status -->

---

## 2. マイルストーン一覧

<a id="m0-setup"></a>
### M0: プロジェクトセットアップ

- [x] リポジトリの作成とドキュメント管理規則の策定
- [x] GitHub Actions（CI）の設定
- [x] `REF:docs/script/verify-docs.py` と `REF:scripts/verify.sh` の作成
- [x] Rust ツールチェーン退避（`REF:scripts/bootstrap-rust.sh`, `REF:.github/workflows/toolchain-export.yml`）
- [x] `REF:Cargo.toml` / `REF:rust-toolchain.toml` / `REF:src/main.rs` のスキャフォールド（`cargo check` まで通過）

<a id="m1-spec"></a>
### M1: 仕様書・設計書の策定（v2: Rust / 全 JSON）

- [x] 仕様書 13 本・設計書 13 本を Rust 前提へ改訂
- [x] `REF:docs/specifications/engine-definitions.md`（エンジン定義 JSON）
- [x] `REF:docs/specifications/network.md`（ネットワークプロファイル JSON）
- [x] `REF:docs/specifications/filters.md`（フィルタ JSON）
- [x] `REF:docs/specifications/quotas.md`（クォータ JSON）
- [ ] ユーザーによる仕様レビューと承認（指摘は `REF:docs/history.md` の `SPEC-FIX` として記録する）

<a id="m2-implement"></a>
### M2: コア実装（Rust）

- [x] 依存クレートの確定（`htmd 0.5` / `scraper 0.29` / `reqwest 0.12` 等）と vendor 退避・サンドボックスへの搬入
- [x] `src/ports/` のトレイト定義（`HttpClient`, `Fetcher`, `HtmlToMarkdown`, `StateStore`, `HistoryStore`, `CounterStore`, `Clock`, `IdGen`, `Env`, `Reporter`）
- [x] `src/core/` の純粋ロジック（設定型・エンジン定義型・抽出仕様・URL・テキスト・見出し・節・予算・フィルタ・クォータ・履歴イベント）
- [x] 設定ローダ（`$ref` 展開・5 層マージ・由来記録・未知キー温存・検証）
- [x] 状態ディレクトリ（原子的書き込み・セッション・文書キャッシュ・カウンタ）
- [x] HTTP アダプタ（reqwest・リダイレクト・タイムアウト・サイズ上限・文字コード・再試行）
- [x] エンジン評価器（HTML 抽出・JSON 抽出・変換パイプライン・ページング）
- [x] 抽出と変換（`scraper` + 密度推定 + `htmd`、後処理、JavaScript 依存の検出）
- [x] 見出し抽出・節分割・アウトライン・コンテキスト予算
- [x] フィルタ評価（結果・本文）と集計
- [x] ガード（レート制限・クォータ・消費記録）
- [x] サブコマンド（`search` / `next` / `refine` / `outline` / `read` / `history` / `engines` / `network` / `filters` / `quota` / `config` / `doctor`）
- [x] JSON イベント出力とエラーカタログ
- [x] 対話モードの状態機械
- [x] 同梱アセット（`assets/engines/*.json`, `assets/network/*.json`, `assets/filters/*.json`, `assets/quotas/*.json`）

<a id="m3-hardening"></a>
### M3: 品質強化

- [x] アーキテクチャ検査（層構造・エンジン名分岐の禁止・警告コードの発行元・仕様節との対応）
- [x] 契約テスト（JSON スキーマ・設定駆動・変換・無害化）
- [x] 結合テスト（検索→選択→取得→アウトライン→節読み）
- [x] 追加の同梱エンジン（SearXNG・Brave 等）を JSON のみで追加し、コード変更が不要であることを検証
- [x] `unwrap` 禁止・非決定性の禁止の自動検査
- [x] 保持期間・上限件数（履歴の間引き）の検証
- [x] 容量管理（文書キャッシュの古い順の削除）の検証
- [x] E2E（`REF:tests/e2e.rs`。実ネットワークは `LLMSEARCH_E2E_LIVE=1` のときだけ）

<a id="m4-release"></a>
### M4: リリース準備

- [x] `REF:README.md` の利用手順（導入・初期設定・典型的な使い方・エンジン追加方法）
- [x] `REF:docs/developers.md` §2・§3・§7 を Rust の実態に合わせて更新
- [x] CI（lint / test / build）を実際のコマンドへ差し替え
- [x] リリース用ワークフロー（`REF:.github/workflows/release.yml`。Linux x86_64 の 1 バイナリをタグで添付）
- [x] 設計書の `状態:` 行を実装完了へ更新し、`REF:src/...` 参照へ切り替え（エージェント連携は未実装のまま）
- [x] `v0.1.0` のタグ付けとリリースノート作成（`REF:.github/workflows/release.yml` が Linux x86_64 の 1 バイナリを添付）

<a id="m5-usability-safety"></a>
### M5: 使いやすさと安全性

- [x] Rust 搬入、build/test、doctor、エンジン dry-run による現状確認（実検索なし）
- [x] 仕様・設計・コードの照合を GitHub Issues #4〜#25 と #27 に記録
- [x] 主要 JSON の共通フィールド、document、warnings、quota.metrics を修正（#4 一部）
- [x] CLI の limit/depth 設定値・子節除外・refine 履歴・--session を修正（#6/#7 一部/#8/#24）
- [x] 未知エンジン・method/encoding 検証と本文マーカー無害化（#10/#16）
- [x] プロキシ適用・認証・noProxy のローカル回帰試験（#14）
- [x] Ubuntu 22.04 基準ビルドと glibc 互換性ゲートの実装（#25。実起動は PR CI で確認）
- [x] 全ローカル検証、develop 向け Ready PR #26 を作成（マージしない。最新 CI とマージ可否は PR を参照）
- [x] 隔離試験で見つけた assets 梱包・ビルド元パス依存を修正（#27）

JSON 全コマンド統一・robots・クォータ計測変更・リダイレクト・設定探索順など、未対応や仕様間矛盾は Issues に残す。仕様の機能削除・安全方針の変更は行わない。#9 は対象不存在時の E5001 とエラーカタログの E1006 が矛盾するため未修正。#7 の検索結果別スロット保存は未対応。#22 の merge 統合は未対応であることをコードで再確認済み。

<a id="m6-dogfood-sync"></a>
### M6: 実運用と仕様同期

- [x] PR #26（M5）で `Closes` した Issue #6 / #8 / #10 / #14 / #16 / #24 / #25 / #27 を develop 上で実機確認し、手動でクローズ
- [x] システム CA ルートの参照（`REF:src/adapters/tls.rs`。`tls.caFile` が最優先。TLS を検証するときだけ読む）
- [x] 抽出結果 0 件の応答に対する再試行と `W4004` / `E4004` の整備（`REF:src/adapters/engine/eval.rs`）
- [x] プロファイルに `proxy` を文字列で指定したときに環境変数 `NO_PROXY` を参照しない件を仕様・設計に明記（挙動は変更しない）
- [x] #4 / #5: `--json` イベントの共通枠と全サブコマンドの JSON 化（`REF:src/ui/render.rs` の `envelope`）
- [x] #7: `refine` の `previousQuery` と履歴記録（M5 で対応済み。仕様へ `--previous` を追記）
- [x] #9: セッション不在時の識別子解決を `E5001` に統一（`REF:src/ui/commands.rs` の `resolve_target`）
- [x] #12: 秘匿情報の伏字化（送信エラー文字列・`engines show` の定義値・検索結果の表示 URL）
- [x] #21: README のテンプレート残骸の除去、クォータ既定値の仕様同期、設計書の `状態:` 更新、未実装機能の plans.md への移動
- [x] #23: `AppError::hint` の活用、`message()` のロケール反映、`E2007` の扱いを仕様に明記
- [x] 仕様・設計・README と実装の突き合わせ（M6 時点）

未対応: #11 / #13 / #15 / #17〜#20 / #22（M5 の棚卸しで起票されたまま。M6 の範囲外）。

<a id="m7-fetch-discipline"></a>
### M7: 取得の作法と計測

- [x] #13: robots.txt の強制。`REF:src/core/robots.rs` の純粋ロジックに呼び出し元を付け、本文取得の前にオリジン単位で判定して `E2006` を返す（`REF:src/adapters/robots.rs`）。判定結果は `<state-dir>/cache/robots.json` に `ttlMinutes` で保持し、取得失敗は `onError` に従う
- [x] #15: `Retry-After` の解釈（`REF:src/core/retry.rs`。秒数と HTTP 日付）と再試行待機への反映。`maxBackoffMs` を超える待機は `E4003` で打ち切る
- [x] #17: `redirectPolicy` の 3xx 処理。`never` は `E2003`、`manual` は `Location` を解決して返し、`follow` で追いきれない 3xx は `E2004`。`maxRedirects = 0` は HTML 中継ページも追わない
- [x] #18: クォータ消費の訂正。`fetch` は `fetches` のみ（`chars` は `read` だけ）、`results` はフィルタ前の件数、フォールバックと同一エンジン内の再送は試行ごとに `searches` を消費し、操作全体が失敗した場合は消費しない
- [x] 再試行判定が詳細文の状態番号解析に依存していた不具合を修正（`AppError::status`。`Retry-After` が付く応答が再試行されなかった）
- [x] 回帰試験 `REF:tests/m7.rs`（ローカル TCP サーバと偽 HTTP のみ。外部ネットワークは使わない）

未対応: #11 / #19 / #20 / #22（M7 の範囲外）。

<a id="m8-config-quota-merge"></a>
### M8: 設定・監査・統合の適合（M5 残課題）

- [x] #11: `config show` を全キーの由来表示（`argv` / `env` / `file` / `default`）へ拡張し、`config path` に存在しない候補を含む探索先一覧を出し、`config init` で `settings.json` / `network/default.json` / `filters/default.json` / `quotas/default.json` の 4 ファイルを書き出す
- [x] #19: `quota reset` を履歴（`EventType::Quota`）へ記録する。範囲・ポリシー ID・リセット前後の消費量を残し、記録失敗でも解除は巻き戻さない
- [x] #20: 設定探索順を「状態ディレクトリ → 利用者設定ディレクトリ」の順（後勝ちの畳み込みで前者が強い）へ直し、`LLMSEARCH_CONFIG` を実装する。環境変数表を実装に合わせて仕様へ反映し、解釈できない値は `E1004` とする
- [x] #22: `search.merge=true` のとき `search.engines` の全エンジンへ逐次送信し、`merge_round_robin` で交互に統合する。`W4005` の重複除去件数と `engine` 付与、送信回数分のクォータ消費を行う
- [x] 回帰試験 `REF:tests/m8.rs`（4 件の Issue に対応する 9 ケース）

<a id="m9-json-and-docs"></a>
### M9: JSON・秘匿情報・ドキュメントの総点検（M5 残課題）

- [x] #4: `--json` の共通フィールドを全イベントへ揃える。`done` / `error` にも `session` / `at` を付け、`at` は `YYYY-MM-DDTHH:MM:SSZ`、`refine` に `previousQuery`、`outline` の前に `document` イベント、検索・文書系イベントに `warnings[]`、`engines` の `policy` は安定した識別子
- [x] #5: `--json` 指定時に `engines show/test`・`network/filters show`・`config *`・`quota *`・`history export/clear` が生テキストを出さず、イベントと `done` を出す
- [x] #7: `refine` は直前のクエリを `previousQuery` として扱い、`search_uc::refine` で履歴に `refine` を残す。仕様に無い `--previous` を削除する
- [x] #9: セッション無しで識別子を指定した場合は `E5001`（暗黙作成なし）、セッション内に無い識別子は `E1006`、`next` のセッション無しも `E5001` と hint にする
- [x] #12: 検索結果・文書の表示 URL を `display_url` で一元的に伏字化する。エラー `details`（送信・robots）も伏字化する
- [x] #21: README のクォータ記述と `GUIDE.md` のテンプレート残骸を直し、設計書の TypeScript 時代のパスを実装に合わせる。未実装機能は仕様から外し `REF:docs/plans.md#unplanned` に移す
- [x] #23: `E2007` を発行しない理由を仕様に明記したまま、`message` / `hint` を `ui.locale` で選び、`Display` は日本語固定であることを設計に記す
- [x] 回帰試験 `REF:tests/m9.rs`

<a id="m10-english-and-language"></a>
### M10: CLI の全英語化とエンジン言語の一本化

- [x] 仕様の改訂: `REF:docs/specifications/errors.md` §4.2 / §4.5、`REF:docs/specifications/configuration.md` §3.2.1 / §3.7 / §4 / §6.2、`REF:docs/specifications/engine-definitions.md` §2.1 / §3.2 / §8.3、`REF:docs/specifications/network.md` §2.1 / §7、`REF:docs/specifications/cli-interface.md` §1.3
- [x] 設計の改訂: `REF:docs/designs/errors.md` §4、`REF:docs/designs/configuration.md` §3.2 / §3.7 / §4、`REF:docs/designs/engine-definitions.md` §3.2、`REF:docs/designs/network.md` §7、`REF:docs/designs/cli-interface.md` §1.3
- [x] `Locale` の削除とカタログの英語化（`REF:src/core/errors.rs`、`Display` の日本語直書きを含む）
- [x] `ui.locale` / `LLMSEARCH_UI_LOCALE` / `Settings::locale()` / `App::locale()` / `Renderer::set_locale()` / `peek_locale()` の削除と、旧 `ui.locale` への `W1002` 告知
- [x] CLI・対話 UI・`doctor`・検証・警告・進捗の全英語化（`REF:src/ui/`・`REF:src/usecases/`・`REF:src/adapters/`）。同梱アセットの `description` も英語化
- [x] `search.language`（既定 `en`）と `--language` / `LLMSEARCH_SEARCH_LANGUAGE` の追加、`src/core/lang.rs` の正規化、`{{language}}` / `{{locale}}` の配線
- [x] `Accept-Language` のエンジン段での補完、`NetworkProfile.acceptLanguage` の廃止、同梱 3 エンジン（duckduckgo / brave / searxng）の言語パラメータ配線
- [x] `REF:tests/architecture.rs` にユーザー可視日本語リテラルの検出を追加し、`tests/m10.rs` に言語設定の配線と `ui.locale` 廃止の回帰試験を追加
- [x] 実バイナリでの確認（`--help`・エラー・実検索・`config show`・`engines test --dry-run`）と PR（develop 向け）

---

## 3. フェーズ別実行順序

1. **Phase 1（基盤）**: 依存確定と vendor 退避、ポート定義、`core` の型・設定ロード、状態ディレクトリ。
2. **Phase 2（宣言の評価）**: エンジン定義の評価器（HTML/JSON）、テンプレート展開、変換パイプライン、抽出。
3. **Phase 3（抑制と整形）**: フィルタ、クォータ、レート制限、Markdown 後処理。
4. **Phase 4（操作と出力）**: サブコマンド、JSON イベント、対話モード、履歴表示。
5. **Phase 5（品質）**: アーキテクチャ検査、契約テスト、追加エンジンの JSON、E2E、ドキュメント更新。

各フェーズの完了条件は `REF:docs/specifications/testing.md` §3.1 の品質ゲート通過とする。

---

## 4. 技術スタック（決定事項）

| 領域 | 採用 | 決定理由（要約） |
|---|---|---|
| 言語 | Rust（edition 2024、1.90 系） | ユーザー指定。単一バイナリ配布・型による安全性・決定論的な文字列処理に強い |
| HTTP | `reqwest`（rustls, gzip/deflate/brotli） | 非同期・リダイレクト制御・プロキシ・TLS 設定を宣言的に扱える |
| 非同期 | `tokio`（rt-multi-thread） | `reqwest` と同一ランタイムで完結し、対話モードでは `block_on` 1 か所に閉じる |
| CLI | `clap`（derive） | サブコマンド・ヘルプ生成が安定し、依存が小さい |
| JSON | `serde` + `serde_json`（`preserve_order`） | 定義の順序を保ちつつ、未知キーの温存（`flatten`）ができる |
| HTML | `scraper`（html5ever） | CSS セレクタ評価が必須要件（エンジン定義の `selector`） |
| 本文抽出 | 密度推定の自前実装（必要なら `readability` クレート） | 外部の挙動に依存せず、抽出失敗の判定を自分で行える |
| Markdown 化 | `htmd` | HTML→Markdown の変換を自前実装しない |
| 文字コード | `encoding_rs` | EUC-JP / Shift_JIS / GBK 等の検索エンジンに対応する |
| 幅計算 | `unicode-width` | 全角文字を含む端末表示の桁計算を自前実装しない |
| その他 | `url`, `regex`, `chrono`, `sha2`, `dirs`, `thiserror`, `anyhow` | 仕様で必要な最小限 |

バージョンは `Cargo.lock` で固定し、`crates.io` へ到達できないため vendor 退避（`REF:docs/add-dependencies.md`）で導入する。

---

## 5. 仕様書と設計書の対応

| ファイル | 主な内容 | 主な実装単位 |
|---|---|---|
| `overview` | 目的・原則・用語・スコープ | 全体配線・ポート定義 |
| `cli-interface` | 対話メニュー・サブコマンド・入出力 | `src/ui/` と `src/usecases/` |
| `engine-definitions` | エンジン定義 JSON・抽出ルール・検証コマンド | `src/core/engine.rs`, `src/adapters/engine/` |
| `network` | ネットワークプロファイル JSON・再試行・プロキシ・robots | `src/core/config/network.rs`, `src/adapters/http/` |
| `filters` | 結果・本文フィルタ JSON | `src/core/filter/` |
| `quotas` | クォータ・レート制限・カウンタ | `src/core/quota.rs`, `src/usecases/guard/` |
| `search` | 検索実行・正規化・ページング | `src/usecases/search/`, `src/core/search/` |
| `document-fetching` | 取得・抽出・変換・キャッシュ | `src/adapters/http/`, `src/adapters/extract/` |
| `context-assembly` | 見出し・節・予算・出所表示 | `src/core/outline/`, `src/core/context/` |
| `session-state` | 状態ディレクトリ・セッション・破損対応 | `src/adapters/state/` |
| `history` | 履歴の記録と参照 | `src/core/history/`, `src/adapters/state/history_jsonl.rs` |
| `configuration` | 設定 JSON とマージ | `src/core/config/`, `src/usecases/config/` |
| `agent-integration` | JSON イベント | `src/ui/render/json_events.rs` |
| `errors` | エラー・警告・`doctor` | `src/core/errors/`, `src/usecases/doctor/` |
| `testing` | 受入基準・品質ゲート | `tests/` 全体 |

---

## 6. 既知のリスクと対処

| リスク | 影響 | 対処 |
|---|---|---|
| クレートの vendoring に失敗する（`crates.io` へ到達不可） | Rust 実装が開始できない | `REF:.github/workflows/toolchain-export.yml` で `cargo vendor` を実行し、`cache/rust-toolchain` 経由で搬入する。依存は最小限に保つ |
| エンジン定義の書き方が利用者に難しい | 追加の敷居が高い | 同梱エンジン 3 種を実例として提供し、`engines validate` で失敗理由を 1 行で示す |
| DuckDuckGo の HTML 構造変更 | 同梱定義が抽出できなくなる | 抽出は JSON 側の責務とし、定義の差し替えだけで追従できる。`engines test` で原因を特定する |
| 本文抽出の精度がサイト依存で不安定 | 節の欠落・ノイズ混入 | 抽出失敗を `W3001` として可視化し、`<body>` 全体へのフォールバックを提供する |
| クォータの誤設定で作業が止まる | 調査が中断する | 上限到達メッセージにリセット時刻と解除方法を必ず含め、`quota check` で事前確認できる |
| 状態ディレクトリの肥大化 | サンドボックスのスナップショット上限に影響 | 既定でリポジトリ外（`~/.local/state`）に保存し、容量管理を実装する |
| 外部コンテンツ経由のプロンプトインジェクション | エージェントの誤動作 | 出所ヘッダと信頼境界マーカーを必須化し、JSON でも `event` と `url` を併記する |

---

## 7. 未実装・未計画の機能

大規模な機能追加で当面実装予定がないものはここに記録する。

<a id="unplanned"></a>
- ヘッドレスブラウザ（Playwright 等）による JavaScript 描画
- MCP（Model Context Protocol）サーバーとしての提供
- 節本文の自動要約と回答生成
- 複数文書の横断比較・差分表示
- PDF・Office 文書の取り込み
- 設定プロファイル（`--profile` / `profile` キー）による設定集合の切り替え（M5 の棚卸しで未実装と判明し、仕様から移動）
- `read --heading <正規表現>` による見出し名での節指定（同上）
- `history export --with-body`（履歴書き出しへの本文同梱。同上）
- `--no-filter=all`（既定の除去要素まで無効化する指定）は採用しない。既定の除去要素は安全性のための不変条件とし、`--no-filter` は値なしのフラグに限る
- 認証・Cookie を伴うページの取得
- サイト内クローリングとサイトマップ探索
- XML（RSS / Atom）応答の抽出器
- エンジン定義の共有レジストリ
- 複数利用者での履歴共有、サーバー常駐 API
