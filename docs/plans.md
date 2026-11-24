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
| 配布互換ゲート（M11） | 完了 | CI の Build & Distribution Compatibility の `E1003`（ステージした `assets/` の入れ子）を直し、同梱既定の解決を配布配置に依存させない | `REF:docs/plans.md#m11-distribution-assets` |
| ドキュメントとソースの同期（M12） | 完了 | docs 全 31 本の総点検・未実装仕様の切り分け・`GUIDE.md` の統合・未使用コードの削除 | `REF:docs/plans.md#m12-docs-source` |
<!-- SYNC:END project-status -->

---

## 2. マイルストーン一覧

完了したマイルストーンの作業内容・判断理由・計測結果は `REF:docs/history.md` の `MILESTONE` / `DECISION` / `SPEC-FIX` エントリに移した。ここには現在の進捗を判断するための要約だけを残す（アンカーは各設計書の `状態:` 行が参照するため変更しない）。

<a id="m0-setup"></a>
### M0: プロジェクトセットアップ

- [x] リポジトリ・ドキュメント管理規則・CI・検証スクリプト・Rust ツールチェーン退避・スキャフォールド（詳細は `REF:docs/history.md#hist-milestone-m0`）

<a id="m1-spec"></a>
### M1: 仕様書・設計書の策定（v2: Rust / 全 JSON）

- [x] 仕様書・設計書を Rust 前提へ改訂（現在は 15 本ずつ。`REF:docs/specifications/engine-definitions.md` ほか）
- [ ] ユーザーによる仕様レビューと承認（指摘は `REF:docs/history.md` の `SPEC-FIX` として記録する）

<a id="m2-implement"></a>
### M2: コア実装（Rust）

- [x] ポート・`core`・アダプタ・ユースケース・UI・同梱アセットの実装（依存確定と vendor 退避を含む。詳細は `REF:docs/history.md#hist-milestone-m002`）
- [x] サブコマンド 12 種、JSON イベント、エラーカタログ、対話モードの状態機械

<a id="m3-hardening"></a>
### M3: 品質強化

- [x] アーキテクチャ検査・契約テスト・結合テスト・E2E と、容量管理・履歴の間引きの検証（詳細は `REF:docs/history.md#hist-milestone-m003`）

<a id="m4-release"></a>
### M4: リリース準備

- [x] README・開発者ガイド・CI・リリースワークフロー・`v0.1.0` のタグ付け（詳細は `REF:docs/history.md#hist-milestone-m003`）

<a id="m5-usability-safety"></a>
### M5: 使いやすさと安全性

- [x] Issue の洗い出し（#4〜#25, #27）と、JSON 共通フィールド・CLI オプション・検証・プロキシ・配布互換性の修正（PR #26。詳細は `REF:docs/history.md#hist-milestone-m005`）

<a id="m6-dogfood-sync"></a>
### M6: 実運用と仕様同期

- [x] システム CA ルート、抽出 0 件時の再試行、`--json` の共通枠（#4/#5）、`refine` と対象解決（#7/#9）、秘匿情報の伏字化（#12）、ドキュメント同期（#21/#23）（PR #28。詳細は `REF:docs/history.md#hist-milestone-m006`）

<a id="m7-fetch-discipline"></a>
### M7: 取得の作法と計測

- [x] robots.txt の強制（#13）、`Retry-After`（#15）、`redirectPolicy` の 3xx（#17）、クォータ消費のずれ（#18）（PR #30。詳細は `REF:docs/history.md#hist-milestone-m007`）

<a id="m8-config-quota-merge"></a>
### M8: 設定・監査・統合の適合（M5 残課題）

- [x] `config show/path/init`（#11）、`quota reset` の履歴（#19）、設定探索順と環境変数（#20）、`search.merge` の統合（#22）（PR #31。詳細は `REF:docs/history.md#hist-milestone-m008`）

<a id="m9-json-and-docs"></a>
### M9: JSON・秘匿情報・ドキュメントの総点検（M5 残課題）

- [x] `--json` の統一（#4/#5）、`refine` と対象解決（#7/#9）、表示 URL の伏字化（#12）、ドキュメント整理（#21/#23）（PR #32。詳細は `REF:docs/history.md#hist-milestone-m009`）

<a id="m10-english-and-language"></a>
### M10: CLI の全英語化とエンジン言語の一本化

- [x] CLI メッセージの全英語化、嘘をつく `ui.locale` の削除、`search.language` と `--language` の追加（PR #34。詳細は `REF:docs/history.md#hist-decision-d033`）

<a id="m11-distribution-assets"></a>
### M11: 配布互換ゲートの `E1003` を直す

- [x] CI のステージ手順を冪等化し、同梱既定の解決を配布配置に依存させない（埋め込み既定・`bundled_root` の 2 段探索。PR #35。詳細は `REF:docs/history.md#hist-spec-fix-f017`）

<a id="m12-docs-source"></a>
### M12: ドキュメントとソースのリファクタリング（同期の網羅チェック）

- [x] 実バイナリ（ローカル HTTP スタブ。外部ネットワークは使わない）で Issue #4 / #5 / #7 / #9 / #12 / #21 / #23 の現状を再測した
- [x] 食い違い 20 件を一覧化し、仕様書・設計書の変更案の承認を得た（2026-11-23）
- [x] 未実装の仕様を実装に合わせ、未実装機能を `REF:docs/plans.md#unplanned` へ移した
- [x] `GUIDE.md`（旧テンプレート由来の開発プロセス文書）を削除し、固有情報を `REF:docs/developers.md` へ統合した（README の参照も差し替え）
- [x] 未使用コード `HistoryStore::skipped_lines` / `JsonlHistory::skipped` を削除した（`W5004` は発行しない）
- [x] `REF:docs/plans.md` §2 の完了マイルストーン詳細を `REF:docs/history.md` へ移して要約だけにした（判断理由は `REF:docs/history.md#hist-decision-d034`）
- [x] 検証: `REF:docs/script/verify-docs.py`、`REF:scripts/verify.sh --quick`、`cargo test --locked --all-targets`、`cargo clippy --all-targets -- -D warnings`、実バイナリでの再測

#### M12 の Issue 有効性確認（2026-11-23 実測）

| Issue | 実測 | 判定 |
|---|---|---|
| #4 `--json` の仕様乖離 | `error` / `done` / `search` に `session`・`at`（`Z` 終端）・`warnings[]`、`refine` に `previousQuery`、`engines list` の `policy` は `first-wins`。`outline` / `read` は `document` を先行出力 | 解消済み |
| #5 `--json` 非対応のサブコマンド | `engines show/test`・`config show/path/explain`・`quota show/check/reset`・`network show`・`filters show` が封筒＋`done` | 解消済み |
| #7 `refine` | `--previous` は提供しない（セッションの直前クエリを使う）。履歴に `refine` を残し `previousQuery` を返す。結果は `results` を上書きし別スロット保存は無い（`unplanned` に記載） | 部分残（仕様どおり） |
| #9 対象解決 | セッション無しで `read R1` → `E5001`＋hint、`next` → `E5001`、存在しない識別子 → `E1006` | 解消済み |
| #12 秘匿情報 | `TmplValue::Explicit` と `value_env` を含む定義で `engines show` / `show --raw` / `test` / `test --dry-run` のすべてが 0 件漏洩。エラー `details` は `without_url()`＋`redact_text`。検索結果 URL は `token=***` | 解消済み |
| #21 docs の誤記・残骸 | README のクォータ記述・`--profile`・`--no-filter=all`・`with-body`・`read --heading`・`context.max_chars`（仕様側）・`text/plain`（仕様側）は修正済み。残りは `GUIDE.md` で、M12 で削除・統合した | 解消済み |
| #23 `hint` / `E2007` / locale | `AppError.hint` を設定（4 箇所）・表示（`Renderer::error`）し、実測でも hint が出る。`E2007` は発行箇所ゼロで仕様も「発行しない」と明記。メッセージは全英語・locale 設定なし | 解消済み |

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
- `session.ttlDays` による期限切れセッションの自動削除と、`session.maxSessions` による件数上限の削除（M12 で値は設定・表示のみと確認。自動削除は行わない）
- セッションを削除するコマンド（保持期間を過ぎたものの一括削除。`--yes` を伴う確認付きの操作。M12 で仕様から削除）
- アウトラインへの `minSectionChars` / `dropEmptySections` / `maxHeadingDepth` の適用（`filter.content` と `context.minSectionChars` の両方。M12 で「定義と検証のみ」と確認）
- 残量が 20% を切ったときの注記（`quota` の対話モード表示。M12 で未実装と確認し、仕様から削除）
- `Ctrl+C` の捕捉と終了コード `130`（M12 で未実装と確認。割り込みは OS と端末の既定動作に任せる）
- `!<n>` による同一セッション内の検索語の再利用（M12 で未実装と確認し、仕様から削除）
- 検索結果の別クエリ・スロット保存（`refine` は現在 `results` を上書きする。Issue #7 の残項目）
- 履歴の不正行スキップを警告 `W5004` として通知する（M12 では未使用コードを削除し、黙って読み飛ばす仕様にした）
- `history.enabled = false` で履歴の記録を停止する（M12 で値の読み込みと表示のみと確認。停止は未実装）
- `quota` の `resetsAt` を機械可読な RFC 3339（`Z` 終端または明示オフセット）にする（現在は `YYYY-MM-DDT00:00 local` の文字列）
- `read --heading <正規表現>` による見出し名での節指定（同上）
- `history export --with-body`（履歴書き出しへの本文同梱。同上）
- `--no-filter=all`（既定の除去要素まで無効化する指定）は採用しない。既定の除去要素は安全性のための不変条件とし、`--no-filter` は値なしのフラグに限る
- 認証・Cookie を伴うページの取得
- サイト内クローリングとサイトマップ探索
- XML（RSS / Atom）応答の抽出器
- エンジン定義の共有レジストリ
- 複数利用者での履歴共有、サーバー常駐 API
