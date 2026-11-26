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
| 実運用と仕様同期（M6） | 完了 | システム CA ルート・E4004 耐性・M5 残 Issue の解消・ドキュメント同期（PR #28） | `REF:docs/plans.md#m6-dogfood-sync` |
| 取得の作法と計測（M7） | 完了 | robots.txt の強制（#13）・`Retry-After`（#15）・`redirectPolicy` の 3xx（#17）・クォータ消費のずれ（#18） | `REF:docs/plans.md#m7-fetch-discipline` |
| 設定・監査・統合の適合（M8） | 完了 | `config show/path/init`（#11）・`quota reset` の履歴（#19）・設定探索順と環境変数（#20）・`search.merge` の統合（#22） | `REF:docs/plans.md#m8-config-quota-merge` |
| JSON・秘匿情報・ドキュメント（M9） | 完了 | `--json` の統一（#4/#5）・`refine` と対象解決（#7/#9）・表示 URL の伏字化（#12）・ドキュメント整理（#21/#23）（PR #32） | `REF:docs/plans.md#m9-json-and-docs` |
| 表示言語とエンジン言語（M10） | 完了 | CLI メッセージの全英語化・嘘をつく `ui.locale` の削除・`search.language` の追加 | `REF:docs/plans.md#m10-english-and-language` |
| 配布互換ゲート（M11） | 完了 | CI の Build & Distribution Compatibility の `E1003`（ステージした `assets/` の入れ子）を直し、同梱既定の解決を配布配置に依存させない | `REF:docs/plans.md#m11-distribution-assets` |
| ドキュメントとソースの同期（M12） | 完了 | docs 全 31 本の総点検・未実装仕様の切り分け・`GUIDE.md` の統合・未使用コードの削除 | `REF:docs/plans.md#m12-docs-source` |
| M5 棚卸し残課題の再監査（M15） | 完了（PR #46 マージ済み） | Issue #4/#5/#7/#9/#12/#21/#23 の仕様・設計・実装・回帰検証 | `REF:docs/plans.md#m15-m5-residuals` |
| 実環境検証のフォローアップ（M16） | 実装・ローカル検証完了（レビュー待ち） | 同一 refine の W4001 抑制（#47）・JSON 非信頼属性（#48）・Muse 検証記録 | `REF:docs/plans.md#m16-real-provider-followup` |
<!-- SYNC:END project-status -->

---

## 2. 現在のブランチ・PR・直近の作業状態

- **ブランチ**: `arena/b36aced3-llmsearch`（base: develop `5db4708`）。fetch した最新 develop と一致。
- **状態**: M15 PR #46 はマージ済み。M16 は #47 / #48 の実装・ローカル検証完了、develop 向け PR でレビュー依頼。マージは進行役が行う。
- **変更**: 同一 refine の W4001 だけを抑制し再検索・履歴維持。JSON content/document に `untrusted: true` を追加し text・chars・v=1 を維持。
- **検証**: `tests/m16.rs` で loopback HTTP と実バイナリを実行。変更あり／なし refine、quiet の JSON 警告保持、JSON/text 非信頼境界、機微 query 秘匿と通常 query 保持を確認。全体ゲートは `REF:reports/m16-validation.md`。
- **実環境**: Muse のローカル実検索結果を `REF:reports/m16-validation.md` に記録。エージェントは実サイトへ再アクセスしていない。userinfo 成功系は未検証のまま。
- **ツールチェーン**: `/home/user/build/env.sh`。再起動時は `REF:docs/developers.md` §2.2 に従い復旧する。

## 3. 次の作業

1. M16 の PR をレビューし、マージは進行役が行う（エージェントは行わない）。
2. 既存 v0.1.0 / v0.1.1 添付物は未変更。配布には新しいタグで `REF:.github/workflows/release.yml` を実行する。
3. MITM 環境での実 HTTPS 通信（Muse 等）は進行役が確認する。サンドボックス側の検証はローカルのプライベート CA まで。
4. 未計画の機能（`--profile`、`read --heading`、`--no-filter=all`、`history export --with-body` 等）は `REF:docs/plans.md#unplanned` に集約した。安全性の方針を独断で変更しない。

M9 で仕様へ追記した内容（`agent-integration.md` §2.1 / §2.2 / §4.2、`cli-interface.md` §3.2、`errors.md` §4.2、`history.md` §3.3）は、実装との矛盾を解くための追記である。進行役の確認を仰ぐこと。

## 4. コマンド所要時間（サンドボックス）

| コマンド | 初回 | 再実行 |
|---|---|---|
| bootstrap-rust | 約 1 分（339 MB） | 展開済みなら不要 |
| cargo build --locked | 約 74 秒 | 数秒 |
| cargo test --locked | 約 10 秒 | 数秒 |
| python3 docs/script/verify-docs.py | 1 秒未満 | 1 秒未満 |
