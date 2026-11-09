# セッション引き継ぎメモ（Handoff）

本書は開発を始めるにあたって必要な最小限の流動的情報（現在の進捗、作業ブランチ、直近の検証状態）を記録する。毎セッション終了時に更新する。
固定の開発環境構築手順・ドキュメント管理規則・ArenaAgent の仕様・技術的注意点は `REF:docs/developers.md`、今後の実装計画は `REF:docs/plans.md`、完了済みの履歴は `REF:docs/history.md` を参照すること。

---

## 1. 現在の進捗サマリ

<!-- SYNC:START project-status -->
| 区分 | 状態 | 対象範囲 | 参照 |
|---|---|---|---|
| 初期マイルストーン | 完了 | プロジェクトセットアップ・ドキュメント管理規則の策定 | `REF:docs/history.md` |
| 仕様策定（M1） | 改訂中 | Rust・全 JSON 設定・エンジン非依存への仕様改訂（v2） | `REF:docs/specifications/engine-definitions.md` |
| 実装（M2） | 完了 | ポート・`core`・アダプタ・ユースケース・UI・同梱アセット | `REF:docs/plans.md#m2-implement` |
| 品質強化（M3） | 完了 | アーキテクチャ検査・契約テスト・結合テスト・容量管理 | `REF:docs/plans.md#m3-hardening` |
| リリース準備（M4） | 未着手 | README・CI 差し替え・タグ付け | `REF:docs/plans.md#m4-release` |
<!-- SYNC:END project-status -->

---

## 2. 現在のブランチ・PR・直近の作業状態

- **現在のブランチ**: `arena/7d587114-llmsearch`
- **PR**: #1（`arena/7d587114-llmsearch` → `main`、CI 緑）
- **現在の状態**: M2（実装）・M3（品質強化）とも完了。残るは M4（リリース準備）と E2E（実端末・`LLMSEARCH_E2E_LIVE=1`）のみ。
- **検証**: `cargo fmt --all --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`（すべて `--locked --offline`）が通過。`python3 docs/script/verify-docs.py` も通過。
- **テスト数**: lib 114 / architecture 14 / assets 4 / contract 8 / integration 13。
- **直近の作業**: 警告コードの発行元を仕様どおりに固定、`core` からの I/O・非決定性の除去、`config explain <key>` の実装、履歴の保持期間・上限件数、文書キャッシュの容量削除、README の利用手順。
- **未確定事項**:
  - 依存クレートのバージョン固定は完了（`htmd 0.5`, `scraper 0.29` 等）。追加のクレートが必要になった場合は vendor 退避の再実行が必要。
- **Rust ツールチェーン**: `bash scripts/bootstrap-rust.sh` で搬入済み（`/home/user/build` はスナップショット対象外のため、コンテナ再起動後は再実行する）。復旧手順は `REF:docs/developers.md` §2.2 を参照。

---

## 3. 次セッション開始時の手順

1. `/home/user/build/env.sh` が存在しない場合（コンテナ再起動後・未搬入）は次を実行する（詳細は `REF:docs/developers.md` §2.2）:
   ```bash
   bash scripts/bootstrap-rust.sh
   git fetch origin arena/7d587114-llmsearch
   git reset --hard origin/arena/7d587114-llmsearch
   ```
2. ドキュメント規則を確認する:
   ```bash
   python3 docs/script/verify-docs.py
   ```
3. `REF:docs/plans.md#m3-hardening` の残項目（`unwrap` 禁止・非決定性の禁止の自動検査）と `REF:docs/plans.md#m4-release` を進める。

---

## 4. コマンド所要時間の目安（サンドボックス）

| コマンド | 初回 | 2 回目以降 |
|---|---|---|
| `python3 docs/script/verify-docs.py` | 約 1 秒 | 約 1 秒 |
| `node -v` / `npm -v` | 1 秒未満 | 1 秒未満 |
| `npm view <pkg> version`（レジストリ照会） | 2〜20 秒 | 2〜20 秒 |
| `bash scripts/verify.sh` | 約 1 秒（docs 検査のみ） | 同左 |
| `. /home/user/build/env.sh && cargo check` | 約 43 秒 | 約 1 秒 |
| `. /home/user/build/env.sh && cargo build` | 約 92 秒（release 含む） | 約 4 秒 |
| `bash scripts/verify.sh`（cargo あり） | 約 95 秒 | 約 10 秒 |
