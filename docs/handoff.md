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
| リリース準備（M4） | 完了 | README・開発者ガイド・CI・v0.1.0 のタグ付けとリリース | `REF:docs/plans.md#m4-release` |
| 使いやすさ・安全性（M5） | 実装中 | Issue 洗い出し・CLI/JSON/信頼境界・プロキシ・配布互換性 | `REF:docs/plans.md#m5-usability-safety` |
<!-- SYNC:END project-status -->

---

## 2. 現在のブランチ・PR・直近の作業状態

- **ブランチ**: `arena/71d83094-llmsearch`（base: develop）
- **PR**: 未作成。develop 向けに作成し、マージしない。
- **状態**: M5 実装中。課題は GitHub Issues #4〜#25。実装済み・残項目は `REF:docs/plans.md#m5-usability-safety`。
- **基準検証**: build/test 成功。doctor は BRAVE_API_KEY 未設定とネットワーク到達不可を検出。duckduckgo dry-run 成功。ライブ検索は行わない。
- **作業中検証**: 回帰テスト `REF:tests/m5.rs` を追加。直前の verify は中断されたため完了扱いにしない。
- **ツールチェーン**: `/home/user/build/env.sh`。再起動時は `REF:docs/developers.md` §2.2 に従い復旧する。未コミット作業を破棄する reset --hard は行わない。

## 3. 次の作業

1. プロキシ #14 と配布互換性 #25 の修正・ローカル試験。
2. `REF:scripts/verify.sh`、差分レビュー、コミット・push。
3. develop 向け PR の Ready / conflict-free / CI green を確認して報告。

## 4. コマンド所要時間（サンドボックス）

| コマンド | 初回 | 再実行 |
|---|---|---|
| bootstrap-rust | 約 1 分（339 MB） | 展開済みなら不要 |
| cargo build --locked | 約 74 秒 | 数秒 |
| cargo test --locked | 約 10 秒 | 数秒 |
| python3 docs/script/verify-docs.py | 1 秒未満 | 1 秒未満 |
