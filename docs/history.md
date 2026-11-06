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
