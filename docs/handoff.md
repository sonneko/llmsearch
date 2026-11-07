# セッション引き継ぎメモ（Handoff）

本書は開発を始めるにあたって必要な最小限の流動的情報（現在の進捗、作業ブランチ、直近の検証状態）を記録する。毎セッション終了時に更新する。
固定の開発環境構築手順・ドキュメント管理規則・ArenaAgent の仕様・技術的注意点は `REF:docs/developers.md`、今後の実装計画は `REF:docs/plans.md`、完了済みの履歴は `REF:docs/history.md` を参照すること。

---

## 1. 現在の進捗サマリ

<!-- SYNC:START project-status -->
| 区分 | 状態 | 対象範囲 | 参照 |
|---|---|---|---|
| 初期マイルストーン | 完了 | プロジェクトセットアップ・ドキュメント管理規則の策定 | `REF:docs/history.md` |
| 仕様策定（M1） | 完了 | 9 本の仕様書・9 本の設計書の初版作成 | `REF:docs/specifications/overview.md` |
| 実装（M2〜M3） | 未着手 | スキャフォールド以降のすべてのコード | `REF:docs/plans.md#m2-implement` |
<!-- SYNC:END project-status -->

---

## 2. 現在のブランチ・PR・直近の作業状態

- **現在のブランチ**: `arena/7d587114-llmsearch`
- **PR**: 未作成
- **現在の状態**: 仕様・設計ドキュメントの初版を作成し、ユーザーレビュー待ち。
- **未確定事項**: `REF:docs/plans.md` §4 の技術スタックは提案段階であり、ユーザー承認後に確定する。
- **検証**: `python3 docs/script/verify-docs.py` が通過することを確認済み。

---

## 3. 次セッション開始時の手順

1. `/home/user/build/env.sh` が存在しない場合（コンテナ再起動後）は次を実行する（詳細は `REF:docs/developers.md` §2.2）:
   ```bash
   bash scripts/bootstrap-toolchain.sh
   git fetch origin <現在のブランチ名>
   git reset --mixed FETCH_HEAD
   ```
2. ドキュメント規則を確認する:
   ```bash
   python3 docs/script/verify-docs.py
   ```
3. ユーザー承認が得られていれば `REF:docs/plans.md#m2-implement` のスキャフォールドから実装する。

---

## 4. コマンド所要時間の目安（サンドボックス）

| コマンド | 初回 | 2 回目以降 |
|---|---|---|
| `python3 docs/script/verify-docs.py` | 約 1 秒 | 約 1 秒 |
| `node -v` / `npm -v` | 1 秒未満（導入済み） | 1 秒未満 |
| `npm view <pkg> version`（レジストリ照会） | 2〜20 秒 | 2〜20 秒 |
| `npm install`（M2 のスキャフォールド後） | 未計測 | 未計測 |
| `bash scripts/verify.sh` | 約 1 秒（docs 検査のみ） | 同左 |
