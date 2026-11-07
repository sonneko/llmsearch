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
