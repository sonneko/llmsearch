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
| 品質強化（M3） | 進行中 | アーキテクチャ検査・契約テスト・結合テスト | `REF:docs/plans.md#m3-hardening` |
| リリース準備（M4） | 未着手 | README・CI 差し替え・タグ付け | `REF:docs/plans.md#m4-release` |
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
- [ ] E2E（実端末・`LLMSEARCH_E2E_LIVE=1`）

<a id="m4-release"></a>
### M4: リリース準備

- [x] `REF:README.md` の利用手順（導入・初期設定・典型的な使い方・エンジン追加方法）
- [ ] `REF:docs/developers.md` §2・§3・§7 を Rust の実態に合わせて更新
- [ ] CI（lint / test / build）を実際のコマンドへ差し替え
- [x] 設計書の `状態:` 行を実装完了へ更新し、`REF:src/...` 参照へ切り替え（エージェント連携は未実装のまま）
- [ ] `v0.1.0` のタグ付けとリリースノート作成

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
- 認証・Cookie を伴うページの取得
- サイト内クローリングとサイトマップ探索
- XML（RSS / Atom）応答の抽出器
- エンジン定義の共有レジストリ
- 複数利用者での履歴共有、サーバー常駐 API
