# 開発計画（Plans）

本書は現在の開発状況サマリ、実装計画、マイルストーンを記述する。完了済みの経緯・決定ログは `REF:docs/history.md`、固定の開発者情報は `REF:docs/developers.md` を参照すること。

---

## 1. 全体進捗サマリ

<!-- SYNC:START project-status -->
| 区分 | 状態 | 対象範囲 | 参照 |
|---|---|---|---|
| 初期マイルストーン | 完了 | プロジェクトセットアップ・ドキュメント管理規則の策定 | `REF:docs/history.md` |
| 仕様策定（M1） | 完了 | 9 本の仕様書・9 本の設計書の初版作成 | `REF:docs/specifications/overview.md` |
| 実装（M2〜M3） | 未着手 | スキャフォールド以降のすべてのコード | `REF:docs/plans.md#m2-implement` |
<!-- SYNC:END project-status -->

---

## 2. マイルストーン一覧

<a id="m0-setup"></a>
### M0: プロジェクトセットアップ

- [x] リポジトリの作成とドキュメント管理規則の策定
- [x] GitHub Actions（CI）の設定
- [x] `REF:docs/script/verify-docs.py` と `REF:scripts/verify.sh` の作成

<a id="m1-spec"></a>
### M1: 仕様書・設計書の策定

- [x] `REF:docs/specifications/overview.md`（目的・原則・用語・スコープ・全体構成）
- [x] `REF:docs/specifications/cli-interface.md`（対話メニュー・サブコマンド・終了コード）
- [x] `REF:docs/specifications/search-providers.md`（プロバイダ・正規化・ページング）
- [x] `REF:docs/specifications/document-fetching.md`（取得・本文抽出・Markdown 変換・キャッシュ）
- [x] `REF:docs/specifications/context-assembly.md`（見出し・節・コンテキスト予算・出所表示）
- [x] `REF:docs/specifications/session-state.md`（セッション・状態ディレクトリ・破損時の扱い）
- [x] `REF:docs/specifications/history.md`（閲覧履歴の記録・参照・再出力・削除）
- [x] `REF:docs/specifications/configuration.md`（設定ファイル・環境変数・未知キーの温存）
- [x] `REF:docs/specifications/agent-integration.md`（JSON イベント仕様・エージェント向け手順）
- [x] `REF:docs/specifications/errors.md`（エラーコード体系・警告・`doctor`）
- [x] `REF:docs/specifications/testing.md`（受入基準・テスト層・品質ゲート）
- [x] 各仕様に対応する `REF:docs/designs/` の設計書初版
- [ ] ユーザーによる仕様レビューと承認（フィードバックは `REF:docs/history.md` の `SPEC-FIX` として記録する）

<a id="m2-implement"></a>
### M2: コア実装

- [ ] スキャフォールド: `package.json`、`tsconfig.json`、`biome.json`、Vitest 設定、`bin` エントリ
- [ ] `src/ports/` のポート定義（`SearchProvider`, `DocumentFetcher`, `HtmlToMarkdown`, `StateStore`, `HistoryStore`, `Clock`, `IdGenerator`, `Env`, `Reporter`）
- [ ] 設定の解決・検証・`config` サブコマンド
- [ ] 状態ディレクトリの解決・原子的書き込み・セッションストア・履歴ストア
- [ ] 検索: `duckduckgo` プロバイダ実装と結果正規化・URL 正規化・ページング
- [ ] 取得: HTTP アダプタ（リダイレクト・タイムアウト・サイズ上限・文字コード）・robots.txt・レート制限
- [ ] 変換: `linkedom` + `@mozilla/readability` + `turndown` による HTML→Markdown、後処理
- [ ] 見出し抽出・節分割・アウトライン・コンテキスト予算・切り詰め
- [ ] `search` / `next` / `refine` / `outline` / `read` サブコマンドと JSON イベント出力
- [ ] `history` サブコマンド（`list` / `show` / `grep` / `export` / `clear`）
- [ ] 対話モードの画面状態機械（検索結果・選択・アウトライン・節読み・深度変更・履歴）
- [ ] エラーカタログ・終了コード・`doctor`

<a id="m3-hardening"></a>
### M3: 拡張プロバイダと品質強化

- [ ] `searxng` プロバイダ（JSON 優先、HTML フォールバック）
- [ ] `brave` プロバイダ（API キーは環境変数のみ）
- [ ] アーキテクチャ検査テスト（層構造・`console` 禁止・非決定性の禁止・エラーコード定数）
- [ ] フィクスチャ記録スクリプトと契約テストの拡充
- [ ] 容量管理・保持期間・削除操作の実装と検証
- [ ] E2E（実端末・`LLMSEARCH_E2E_LIVE=1`）の整備

<a id="m4-release"></a>
### M4: リリース準備

- [ ] `README.md` の利用手順（インストール・初期設定・典型的な使い方）
- [ ] `docs/developers.md` §2・§3・§7 を本プロジェクトの実態に合わせて更新
- [ ] CI（lint / test / build）を実際のコマンドへ差し替え
- [ ] 設計書の `状態:` 行を実装完了へ更新し、`REF:` 参照を実コードへ切り替え
- [ ] `v0.1.0` のタグ付けとリリースノート作成

---

## 3. フェーズ別実行順序

1. **Phase 1（基盤）**: スキャフォールド、ポート定義、設定、状態ディレクトリ、履歴ストア。ここが通れば以降の機能はポート差し替えでテストできる。
2. **Phase 2（取得と変換）**: HTTP アダプタ、robots.txt、レート制限、`duckduckgo` プロバイダ、HTML→Markdown 変換。
3. **Phase 3（理解と切り出し）**: 見出し抽出、節分割、アウトライン描画、コンテキスト予算と切り詰め。
4. **Phase 4（操作と出力）**: サブコマンド、JSON イベント、対話モード、履歴表示。
5. **Phase 5（品質）**: アーキテクチャ検査、追加プロバイダ、E2E、ドキュメント更新。

各フェーズの完了条件は `REF:docs/specifications/testing.md` §3.1 の品質ゲート通過とする。

---

## 4. 技術スタック（決定事項）

| 領域 | 採用 | 決定理由（要約） |
|---|---|---|
| 言語 | TypeScript（strict） | npm レジストリへ直接到達でき、サンドボックスでの依存導入が最短で済む |
| 実行環境 | Node.js 22 以上 | `fetch` / `AbortSignal.timeout` / `node:test` 系の標準機能で外部依存を減らせる |
| CLI 解析 | `commander` | サブコマンド・ヘルプ生成が安定し、依存が小さい |
| 設定検証 | `zod` | スキーマ検証と未知キーの温存を同時に扱える |
| HTML パース | `linkedom` | jsdom より軽量で、サンドボックスでの導入が容易 |
| 本文抽出 | `@mozilla/readability` | 記事本文抽出の事実上の標準で、抽出失敗の判定も容易 |
| Markdown 変換 | `turndown` + `turndown-plugin-gfm` | 表・取り消し線を含む GFM 出力に対応 |
| robots.txt | `robots-parser` | 判定ロジックを自前実装しない |
| 表示幅 | `string-width` / `wrap-ansi` | 全角文字を含む端末表示の桁計算を自前実装しない |
| テスト | `vitest` | 実行が速く、スナップショットとモックエージェントの併用が容易 |
| 整形・lint | `biome` | 単一ツールで整形と静的検査を賄え、設定が小さい |

バージョンは導入時に固定し、`package-lock.json` をコミットする（`REF:docs/designs/overview.md` §6.1 の層構造を壊さない範囲で追加する）。

---

## 5. 仕様書と設計書の対応

| ファイル | 主な内容 | 主な実装単位 |
|---|---|---|
| `overview` | 目的・原則・用語・スコープ | 全体配線・ポート定義 |
| `cli-interface` | 対話メニュー・サブコマンド・入出力 | `src/ui/` と `src/usecases/` |
| `search-providers` | 検索・正規化・ページング | `src/adapters/providers/` |
| `document-fetching` | 取得・抽出・変換・キャッシュ | `src/adapters/http/`, `src/adapters/extract/` |
| `context-assembly` | 見出し・節・予算・出所表示 | `src/core/outline/`, `src/core/context/` |
| `session-state` | 状態ディレクトリ・セッション・破損対応 | `src/adapters/state/` |
| `history` | 履歴の記録と参照 | `src/core/history/`, `src/adapters/state/history-jsonl.ts` |
| `configuration` | 設定と環境変数 | `src/core/config/`, `src/usecases/config/` |
| `agent-integration` | JSON イベント | `src/ui/render/json-events.ts` |
| `errors` | エラー・警告・`doctor` | `src/core/errors/`, `src/usecases/doctor/` |
| `testing` | 受入基準・品質ゲート | `tests/` 全体 |

---

## 6. 既知のリスクと対処

| リスク | 影響 | 対処 |
|---|---|---|
| DuckDuckGo の HTML 構造変更で検索が失敗する | 既定プロバイダが使えなくなる | 解析を `parse.ts` に隔離し、フィクスチャ差し替えのみで追従できるようにする。代替として SearXNG を用意 |
| 本文抽出の精度がサイト依存で不安定 | 節の欠落・ノイズ混入 | 抽出失敗を `W3001` として可視化し、`<body>` 全体へのフォールバックを提供する |
| JavaScript 描画前提のページで本文が取れない | 一部サイトが読めない | `W3002` を出して原因を明示し、初期リリースでは非対応と明記する |
| 状態ディレクトリの肥大化 | サンドボックスのスナップショット上限に影響 | 既定で `~/.local/state` に置き、リポジトリ外に保存する。容量管理（`cache.max_size_mb`）を実装する |
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
- 複数利用者での履歴共有、サーバー常駐 API
