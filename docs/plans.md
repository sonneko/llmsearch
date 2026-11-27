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
| 実機監査と CLI・状態整合修正（M13〜M14） | 完了（PR #45 マージ済み） | Issue #37〜#44（ページング識別子・セッション再開・クォータ・doctor・表示整合）の洗い出しと修正 | `REF:docs/plans.md#m14-audit-fixes` |
| M5 棚卸し残課題の再監査（M15） | 完了（PR #46 マージ済み） | Issue #4/#5/#7/#9/#12/#21/#23 の仕様・設計・実装・回帰検証 | `REF:docs/plans.md#m15-m5-residuals` |
| 実環境検証のフォローアップ（M16） | 完了（PR #49 マージ済み） | 同一 refine の W4001 抑制（#47）・JSON 非信頼属性（#48）・Muse 検証記録 | `REF:docs/plans.md#m16-real-provider-followup` |
| 秘匿機能のスタブ検証と修正（M17） | 完了（PR #52 マージ済み） | G1 URL userinfo 成功系・埋込/不正 URL・robots 拒否（#50）と G2 応答ヘッダ・JSON/form body 秘匿・過剰変換修正（#51） | `REF:docs/plans.md#m17-secret-redaction-validation` |
| ゼロ前提の全量同期監査（M18） | 完了（レビュー待ち） | 全仕様・設計・補助文書と `develop`（`0326c1a`）の再照合、Issue #53〜#64 起票、文書側の誤記・設計乖離修正 | `REF:docs/plans.md#m18-zero-assumption-sync-audit` |
| 監査指摘の実装修正（M19） | 計画済み（次工程） | M18 で洗い出した実装側バグ・未接続機能・仕様確認事項（Issue #55〜#64）の解消 | `REF:docs/plans.md#m19-audit-remediation` |
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

<a id="m14-audit-fixes"></a>
### M13〜M14: 実機監査と CLI・状態整合修正

- [x] 実機監査（`127.0.0.1` ループバックスタブ。外部ネットワーク不使用）で Issue #37〜#44 を特定し一括修正した（PR #45。`REF:tests/m14.rs` で回帰固定）
- [x] 詳細な作業内容・判断理由・計測結果は `REF:docs/history.md` の `MILESTONE:M014` エントリへ移管済み
- 状態: 完了。

<a id="m15-m5-residuals"></a>
### M15: M5 棚卸し残課題の再監査

#### 目的と対象

M5 棚卸しの Issue #4、#5、#7、#9、#12、#21、#23 を、M9 / M12 の修正済み記録だけに依存せず、現在の仕様・設計・実装・実バイナリで再照合する。Issue #37〜#44 は PR #45 で修正済みなので対象外とし、`develop` が既定ブランチでないため open のままでも再対応しない。

#### 初期照合で確認する範囲

| Issue | 再監査する残項目 |
|---|---|
| #4 | 共通イベント封筒・`at`・`refine` / `document` / `warnings[]` に加え、設定解決時と `Reporter::warn` の警告を JSON に失わず反映すること |
| #5 | 全管理系サブコマンドを `--json` で実行し、標準出力がイベント封筒付き JSON Lines だけで終端が正しいこと |
| #7 | セッション直前クエリ・`previousQuery`・`refine` 履歴を照合し、現行仕様どおり `results` を置き換える（別クエリ結果スロットは本 Issue の受入対象にしない） |
| #9 | セッション無しの `R` / `D` 識別子と `next`（`E5001` + hint）・既存セッション内の不明 ID（`E1006`）・URL 対象を分け、状態不変条件も照合する |
| #12 | 検索・文書 URL、エンジン定義と要約 URL、dry-run の URL / headers / body、HTTP エラー、履歴と警告の各出力境界で既知の値が漏れないこと |
| #21 | README / ガイド / 仕様 / 設計の誤記、実装と合わない説明、存在しないソース参照、テンプレート残骸を検索して修正する |
| #23 | `E2007` の未発行方針、個別 `AppError.hint` の優先、全英語の `message` / `hint` と CLI / JSON / stderr の一致を実行確認する |

#### 受け入れ条件

1. 仕様書 → 設計書 → 本計画の順で方針を同期し、実装・テストの追加後も挙動と記述が一致する。
2. 管理系を含む `--json` 全コマンドが共通封筒と終端イベントを持ち、`Reporter` 警告を `--quiet` でも構造化出力する（`W1001` は独立イベントで `--quiet` 時に抑制）。
3. `refine` と対象解決の回帰を実プロセスで固定し、意図した既存挙動以外は変更しない。
4. 結果 URL、URL ユーザー情報、エンジン定義、dry-run の URL / headers / body、履歴、警告、エラーの出力・保存に既知の秘匿値を含めない。安全境界 `[untrusted]`、クォータ、フィルタ、定義の送信前検証を維持する。
5. README・ガイド・仕様・設計の誤記・残骸・参照切れを修正し、`E2007` / `AppError.hint` / 英語メッセージの方針を検証で固定する。
6. 自作エンジン定義と OS 割り当てポートの `127.0.0.1` ローカル HTTP スタブだけを使い、外部ネットワークなしで回帰を通す。固定ポート競合を避け、未検証の実サイト挙動は最終報告に明記する。
7. Issue #4/#5/#7/#9/#12/#21/#23 をそれぞれ `Closes` 参照する `develop` 向け PR を 1 件作成し、マージしない。PR 外に残る未コミット / 未 push 変更は有無と扱いを明示する。

#### 実施順序と進捗

- [x] 仕様を更新: `agent-integration.md` / `errors.md` / `history.md` / `session-state.md` / `testing.md` / `engine-definitions.md`
- [x] 設計を更新: `agent-integration.md` / `cli-interface.md` / `engine-definitions.md` / `errors.md` / `history.md` / `session-state.md` / `testing.md`
- [x] 本計画に再監査範囲・受入条件・実施順序を記載
- [x] 警告伝播・履歴 payload / URL・dry-run 秘匿情報の実装を補い、標準出力・標準エラー・履歴の漏洩回帰を追加
- [x] 全 `--json` 管理系コマンド、`refine`、`R` / `D` 対象解決、エラーと hint のローカル実プロセス回帰を実行
- [x] README・ガイド・仕様・設計全体を再検索し、実装との不一致を修正
- [x] OS 割り当てポートの `127.0.0.1` 自作スタブで実バイナリを検証。外部サイトには接続しない
- [x] `bash scripts/verify.sh --quick`（docs / release-gate / fmt-check / clippy / all-target tests）と `git diff --check` を実行
- [x] 7 Issue を `Closes` する `develop` 向け PR #46 を作成（マージ済み）

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
| `overview` | 目的・原則・用語・スコープ | `src/lib.rs`, `src/main.rs`, `src/ports/` |
| `cli-interface` | 対話メニュー・サブコマンド・入出力 | `src/ui/` と `src/usecases/` |
| `engine-definitions` | エンジン定義 JSON・抽出ルール・検証コマンド | `src/core/engine.rs`, `src/core/template.rs`, `src/adapters/engine/` |
| `network` | ネットワークプロファイル JSON・再試行・プロキシ・robots | `src/core/config/network.rs`, `src/adapters/http.rs`, `src/adapters/tls.rs`, `src/adapters/robots.rs` |
| `filters` | 結果・本文フィルタ JSON | `src/core/config/filter.rs` |
| `quotas` | クォータ・レート制限・カウンタ | `src/core/config/quota.rs`, `src/usecases/guard.rs` |
| `search` | 検索実行・正規化・ページング | `src/usecases/search.rs`, `src/core/search.rs` |
| `document-fetching` | 取得・抽出・変換・キャッシュ | `src/adapters/extract/mod.rs`, `src/core/convert.rs`, `src/core/markdown.rs` |
| `context-assembly` | 見出し・節・予算・出所表示 | `src/core/outline.rs`, `src/core/context.rs` |
| `session-state` | 状態ディレクトリ・セッション・破損対応 | `src/core/session.rs`, `src/adapters/state/` |
| `history` | 履歴の記録と参照 | `src/core/history.rs`, `src/adapters/state/history_jsonl.rs` |
| `configuration` | 設定 JSON とマージ | `src/core/config/`, `src/adapters/config/mod.rs`, `src/usecases/config.rs` |
| `agent-integration` | JSON イベント | `src/ui/render.rs`, `src/ui/commands.rs` |
| `errors` | エラー・警告・`doctor` | `src/core/errors.rs`, `src/usecases/doctor.rs` |
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
| 外部コンテンツ経由のプロンプトインジェクション | エージェントの誤動作 | 出所ヘッダと信頼境界マーカーを必須化し、JSON の content/document でも `event`・`url`・`untrusted: true` を併記する |

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
- フィルタ集合の `mode`（`append`）による既定集合への規則の合成（M12 で `FilterSet.mode` が文字列として保持されるだけと確認。仕様では「予約」に変更）
- 検索結果への言語（`SearchHit.language`）の付与（現在は常に未設定のため、フィルタ規則 `language` はどの結果にも一致しない）
- `quota` の `resetsAt` を機械可読な RFC 3339（`Z` 終端または明示オフセット）にする（現在は `YYYY-MM-DDT00:00 local` の文字列）
- `read --heading <正規表現>` による見出し名での節指定（同上）
- `history export --with-body`（履歴書き出しへの本文同梱。同上）
- `--no-filter=all`（既定の除去要素まで無効化する指定）は採用しない。既定の除去要素は安全性のための不変条件とし、`--no-filter` は値なしのフラグに限る
- 認証・Cookie を伴うページの取得
- サイト内クローリングとサイトマップ探索
- XML（RSS / Atom）応答の抽出器
- エンジン定義の共有レジストリ
- 複数利用者での履歴共有、サーバー常駐 API

<a id="m16-real-provider-followup"></a>
### M16: M15 実環境検証の修正

- 前提: Muse が develop `5db4708` / 0.1.2 を実環境検証済み。エージェントは実サイトに再アクセスしない。記録は `reports/m16-validation.md`。
- 仕様: 同一 refine の W4001 抑制（#47）、JSON content/document に boolean `untrusted: true` を追加（#48）。既存本文・文字数・v=1 は維持。
- 設計: usecases/search.rs の警告条件、ui/commands.rs の 2 種のイベント生成だけを変更。テキスト境界と秘匿化は維持。
- 実装・検証: `tests/m16.rs` の loopback HTTP スタブと実プロセスで refine の変更あり／なし、quiet・非 quiet 警告伝播、outline/read の非信頼属性、テキスト境界、機微 query 秘匿と通常 query 保持を検証する。既存回帰テスト・fmt・clippy も実行する。
- 状態: 完了（PR #49 として `develop` にマージ済み）。
- CI と実環境記録を分離し、userinfo 成功系の実サイト検証は未検証のまま残す。PR は develop 向け、自己マージしない。

<a id="m17-secret-redaction-validation"></a>
### M17: M5 秘匿機能のローカルスタブ検証と修正

- 前提: Muse のローカル検証（`develop` `5db4708` / 2026-11-26）で未確認だった G1（URL userinfo 成功系を含む秘匿）と G2（ヘッダ・JSON body・form body の秘匿および過剰マスク有無）を `127.0.0.1` ループバック HTTP スタブと実バイナリで検証する。実サイトへは接続しない。
- 起票 Issue:
  - Issue #50（G1）: `W4006` リダイレクト展開警告の埋込 URL userinfo/秘匿引数漏れ、`E2005` / `E1002` 等の不正・テンプレート・非 HTTP URL の `userinfo@` 漏れ、`RobotsGate` `E2006` の相対パス秘匿引数漏れ、リダイレクト先 userinfo URL での相対リンク認証情報伝播。
  - Issue #51（G2）: `eval.rs` の抽出不一致プレビュー（`W4004` / `E4004`、リトライ後成功や 200 文字途中切断を含む）での応答 JSON body・form body・HTTP ヘッダ行の秘匿漏れ、`E3002` 等の応答ヘッダパラメータ（`; token=...`）漏れ、定義・プロファイル内固定秘匿値の収集統一と `filters show` への `redact_json` 適用、`engines show` における form body テンプレート `{{query}}` の `%7B%7Bquery%7D%7D` 過剰変換。
- 仕様・設計: `agent-integration` §4.2 および `document-fetching` §3.1 を同期し、後方互換性（既存フィールド・イベント形式・通常クエリ/通常ヘッダ/通常 JSON・form 値の非破壊）を維持する。
- 実装順序:
  1. `src/core/url.rs`: ネスト URL クエリ値の再帰秘匿化と、構文エラー・任意スキーム URL 様トークンのフォールバック秘匿化関数を追加し、`parse` の `E2005` 詳細にも適用する。
  2. `src/core/secret.rs`: `redact_display_text` の任意スキーム・不正 URL 対応、`redact_header_params` の追加、`redact_body` の JSON / form（非秘匿セグメント無変換）/ HTTP ヘッダ行 / 埋込断片対応、`is_sensitive_json_key` の認証トークン系キー拡充、エンジン定義・ネットワークプロファイルからの固定秘匿値収集統一を行う。
  3. `src/core/markdown.rs`: `absolutize` で基底 URL から `username` / `password` を除去してから相対リンクを解決する。
  4. `src/adapters/robots.rs`・`src/adapters/http.rs`・`src/adapters/extract/mod.rs`・`src/adapters/engine/eval.rs`・`src/usecases/**`・`src/ui/**`: `E2006` 拒否パス秘匿化、ヘッダパラメータ秘匿化、`W4004` / `E4004` 切り詰め前の `redact_body` 適用、`engines::describe` 要約 URL 秘匿化、`filters show` の `redact_json` 適用、統一 redaction list の配線を実装する。
  5. 検証・記録: CI 再現可能な `REF:tests/m17.rs`（`127.0.0.1` OS 割り当てポートのループバックスタブ＋実バイナリ検証）と単体テストを追加し、`REF:reports/m17-validation.md`、`REF:docs/handoff.md`、`REF:docs/history.md` を更新して品質ゲートを通過させたうえで `develop` 向け PR を作成する（自己マージしない）。
- 状態: 完了（PR #52 として `develop` `0326c1a` にマージ済み、2026-11-26）。Issue #50 / #51 解消。

<a id="m18-zero-assumption-sync-audit"></a>
### M18: ゼロ前提の全量同期監査

- 前提: 過去の監査結果を前提とさず、`develop`（`0326c1a`）の全仕様書 15 本・全設計書 15 本・`REF:README.md`・`REF:docs/developers.md`・本計画・`REF:docs/handoff.md`・`REF:docs/history.md` を `src/`・`tests/`・`assets/`・`scripts/`・`build.rs` と全行照合した。実証は実バイナリ＋`127.0.0.1` ループバック HTTP スタブ・PTY・`no-assets` 隔離配置のみ（外部サイト接続ゼロ）。
- 起票: GitHub Issue #53〜#64（12 件）。立証ログと対応表は `REF:reports/m18-validation.md`。
- 処分:
  - 本 PR で修正（文書側同期・ソース不変更）: #53（トップレベル文書）、#54（全 15 設計書の架空パス・型名）、#64 の一部（`agent-integration.md` §2.2 のイベント・コマンド表）。
  - M19 バックログ（実装側・`src/` 未修正）: #62（P0）→ #57 / #58 / #59 / #60（P1）→ #55 / #61 / #63（P2）→ #56 と #64 残（`content.omittedChildren`、P3）。
  - 意図的な仕様変更（Issue 対象外）: M10 / M14 / M15 / M16 / M17 由来 5 項目を `REF:reports/m18-validation.md` §4 に記録。
- 状態: 完了（本 PR。レビュー待ち）。

<a id="m19-audit-remediation"></a>
### M19: 監査指摘の実装修正（未着手）

- 範囲: M18 が確定した Issue #55〜#64 の実装側バグ・未接続機能・仕様確認事項の `src/` 修正と、各項目のループバックスタブ／PTY 回帰テスト追加。
- 優先順位（M18 監査で確定）:
  1. P0: #62 — `history clear` の `--before` 無視（全件消去）・削除件数 0 固定、`Refine` 要約、隔離ファイル名。
  2. P1: #60 — 文書キャッシュ無効化無視・`~~~` フェンス閉じ誤り・ATX 見出し誤認・`read` オプション無視。
  3. P1: #57 — `request.encoding` 破棄・`enabled=false` 無視・`request.body` 変数未検証・`{{apiKey}}` 未配線。
  4. P1: #58 — HTTP `401` の `E2003` 化・`403` 無条件再試行・`onError` 混同・相対 URL 自動解決・秘匿部分一致誤爆。
  5. P1: #59 — `network` / `filters` の埋め込み既定・`--state-dir`・起動時検証・`config validate` 範囲。
  6. P2: #61 — `onExceed="warn"` 誤警告・`quota check --kind` 値域・`quota reset` 空セッション ID。
  7. P2: #63 — 対話モードのストリーム違反・ページ送り無視・`4) Edit the query` 挙動。
  8. P2: #55 — 休眠設定フィールド・検索結果キャッシュ等の機能化（仕様維持を確認のうえ）。
  9. P3: #56 — 未発行エラーコード 5 件・`W4002` 引数逆転・`doctor` の HTTP GET と `rustc` 重複表記。
  10. P3: #64 残 — `content.omittedChildren` の出力。
- 規律: 後方互換の出力契約（`v: 1`・イベント形式・CLI オプション）を変更しない。仕様書 → 設計書 → ソース → テストの順で同期し、PR は自己マージしない。
- 状態: 計画済み（次マイルストーン）。
