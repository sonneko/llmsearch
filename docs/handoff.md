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
| 使いやすさ・安全性（M5） | 完了 | Issue 洗い出し・CLI/JSON/信頼境界・プロキシ・配布互換性 | `REF:docs/plans.md#m5-usability-safety` |
| 実運用と仕様同期（M6） | 実装中 | システム CA ルート・E4004 耐性・M5 残 Issue の解消・ドキュメント同期 | `REF:docs/plans.md#m6-dogfood-sync` |
<!-- SYNC:END project-status -->

---

## 2. 現在のブランチ・PR・直近の作業状態

- **ブランチ**: `arena/6113b48d-llmsearch`（base: develop）
- **PR**: #26 は develop へマージ済み（M5）。M6 の PR は作成前。マージは進行役が行う。
- **状態**: M6 実装中。PR #26 の `Closes` 対象 #6/#8/#10/#14/#16/#24/#25/#27 を develop 上で実機確認し、手動クローズ済み。残る M5 由来の #4/#5/#7/#9/#12/#21/#23 に対応中。進捗は `REF:docs/plans.md#m6-dogfood-sync`。
- **基準検証**: build/test/clippy/fmt 成功。`REF:scripts/verify.sh` 相当の検証はすべてローカルで通過。
- **作業中検証**: 回帰テスト `REF:tests/m6.rs`（9 件）と `REF:tests/tls.rs`（7 件）を追加。システム CA ルートはローカルのプライベート CA を使う HTTPS サーバで 7 ケースを確認。抽出再試行は「応答が揺れる」fixture サーバで試行回数（既定 2 / `maxAttempts=4` で 4）を確認。
- **ツールチェーン**: `/home/user/build/env.sh`。再起動時は `REF:docs/developers.md` §2.2 に従い復旧する。未コミット作業を破棄する reset --hard は行わない。

## 3. 次の作業

1. M6 の PR 作成・レビュー・マージは進行役が行う（エージェントは行わない）。
2. 既存 v0.1.0 添付物は未変更。配布には新しいタグで `REF:.github/workflows/release.yml` を実行する。既存タグの再実行では修正前のソースをビルドする点に注意（#25）。
3. MITM 環境での実 HTTPS 通信（Muse 等）は進行役が確認する。サンドボックス側の検証はローカルのプライベート CA まで。
4. #11/#13/#15/#17〜#20/#22 は M6 の範囲外で未対応。安全性の方針を独断で変更しない。

M6 で `tls.caFile` が無くても HTTPS が通るようにした（システムの CA ストアと `SSL_CERT_FILE` / `CURL_CA_BUNDLE` / `REQUESTS_CA_BUNDLE` / `SSL_CERT_DIR` を参照。`REF:src/adapters/tls.rs`）。`tls.caFile` は従来どおり最優先で、読み込み失敗は `E2008` で停止する。

## 4. コマンド所要時間（サンドボックス）

| コマンド | 初回 | 再実行 |
|---|---|---|
| bootstrap-rust | 約 1 分（339 MB） | 展開済みなら不要 |
| cargo build --locked | 約 74 秒 | 数秒 |
| cargo test --locked | 約 10 秒 | 数秒 |
| python3 docs/script/verify-docs.py | 1 秒未満 | 1 秒未満 |
