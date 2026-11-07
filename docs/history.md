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
