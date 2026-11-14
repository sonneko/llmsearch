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
- **PR**: #26（develop 向け、Ready for review）。マージは進行役が行う。
- **状態**: M5 実装中。課題は GitHub Issues #4〜#25 と #27。実装済み・残項目は `REF:docs/plans.md#m5-usability-safety`。
- **基準検証**: build/test 成功。doctor は BRAVE_API_KEY 未設定とネットワーク到達不可を検出。duckduckgo dry-run 成功。ライブ検索は行わない。
- **作業中検証**: 回帰テスト `REF:tests/m5.rs` を追加。`REF:scripts/verify.sh` で docs/fmt/clippy/test/release build 成功。M5 実プロセスを含む回帰 7 件、プロキシ実ソケット回帰 9 件も成功。glibc ゲートの拒否試験とローカル ELF（要求 GLIBC 2.34）の起動検査が成功。Ubuntu 22.04/24.04 の実行結果は PR #26 の最新 CI を参照。
- **ツールチェーン**: `/home/user/build/env.sh`。再起動時は `REF:docs/developers.md` §2.2 に従い復旧する。未コミット作業を破棄する reset --hard は行わない。

## 3. 次の作業

1. PR #26 のレビュー・マージは進行役が行う（エージェントは行わない）。
2. 既存 v0.1.0 添付物は未変更。配布には新しいタグで `REF:.github/workflows/release.yml` を実行する。既存タグの再実行では修正前のソースをビルドする点に注意。
3. ライブ検索と公開 HTTPS 宛てのプロキシ通信は進行役が確認する。サンドボックス側の HTTPS 検証はローカル CONNECT まで。
4. #4/#5/#7/#9/#12/#13/#15/#17〜#23 などの残項目・仕様間矛盾は `REF:docs/plans.md#m5-usability-safety` と Issues を参照。安全性の方針を独断で変更しない。

配布互換性の隔離試験で、ビルド元絶対パスの assets を参照する問題 #27 も発見した。配布アーカイブへの assets 同梱と実行ファイル隣からの解決を追加し、開発元の JSON を誤って使わないことを実プロセステストで検証する。CI は配布物だけをコンテナにマウントし、ビルド元のパスを見せない。

## 4. コマンド所要時間（サンドボックス）

| コマンド | 初回 | 再実行 |
|---|---|---|
| bootstrap-rust | 約 1 分（339 MB） | 展開済みなら不要 |
| cargo build --locked | 約 74 秒 | 数秒 |
| cargo test --locked | 約 10 秒 | 数秒 |
| python3 docs/script/verify-docs.py | 1 秒未満 | 1 秒未満 |
