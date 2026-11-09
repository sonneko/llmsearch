# llmsearch

Web を人間のように段階的に調べ、LLM のコンテキスト消費を抑える CLI。

検索結果 → 見出し一覧（アウトライン）→ 選んだ節の本文、という順に必要な分だけ取り出す。検索エンジンとネットワーク挙動は同梱の JSON 定義で切り替えるため、コアにエンジン固有の実装を持たない。設定・フィルタ・クォータもすべて JSON で宣言する。

仕様・設計は `docs/specifications/` と `docs/designs/` にある。実装の進捗は `docs/plans.md`、判断の経緯は `docs/history.md` を参照。

## 導入（ソースから）

```bash
cargo build --release
./target/release/llmsearch doctor
```

`doctor` は設定・同梱アセット・環境変数・ネットワークの状態を点検し、問題があれば `[warn]` / `[fail]` を出す。

## 初期設定

初回は同梱の既定値だけで動く。ユーザー設定を作る場合は次のいずれか。

```bash
llmsearch config path        # 設定ファイルの探索先を表示する
llmsearch config init        # 雛形を書き出す（既存ファイルがあれば --force が必要）
llmsearch config show        # 解決後の値と由来を表示する
llmsearch config explain search.limit   # 個別のキーを説明する
```

状態ディレクトリ（セッション・履歴・文書キャッシュ）は既定で `~/.local/state/llmsearch`。

## 典型的な使い方

```bash
llmsearch search "気候変動 適応策"        # 1 ページ 7 件を検索
llmsearch outline https://example.jp/doc  # 見出し一覧（本文は出さない）
llmsearch read https://example.jp/doc --section D1:S3   # 節だけを読む
llmsearch read https://example.jp/doc --section D1:S3-D2:S1 --max-chars 4000
llmsearch next                             # 同じクエリの次ページ
llmsearch refine "気候変動 適応策 自治体"  # 検索語を変えて再検索
```

引数なしで起動すると対話モードになり、番号を選ぶだけで検索 → 選択 → 取得 → 節読みを進められる（`1)終了 2)検索結果の選択（複数可） 3)8 件目以降を見る 4)検索ワードを修正する`）。

エージェントから使う場合やパイプで受ける場合は `--json` を付ける。データは標準出力、メニュー・進捗・警告は標準エラーに出る。

## エンジンの追加方法

`assets/engines/*.json` と同じ形式の JSON を `--engines-dir <PATH>` かユーザー設定ディレクトリに置く。再ビルドは不要。

```json
{
  "id": "mysearx",
  "url": "https://searx.example.jp/search?q={query}&pageno={page}",
  "response": {
    "kind": "html",
    "items": { "selector": "article.result" },
    "fields": {
      "title": { "selector": "h3", "attr": "text" },
      "url":   { "selector": "a", "attr": "href", "absolutize": true }
    }
  }
}
```

```bash
llmsearch engines validate mysearx          # 定義を検証する
llmsearch engines test --query テスト mysearx --dry-run   # 送信せず組み立てだけ確認する
llmsearch engines test --query テスト mysearx   # 1 回だけ実行する
```

API キーは JSON に書かず `value_env` で環境変数名を参照する。同梱の `brave` は `BRAVE_API_KEY` を参照する。

## 制限と注意

- クォータ既定: セッション 50 検索 / 600 秒あたり 120 検索 / 日次 500 検索。超過すると `E5005` で停止する（`llmsearch quota show` で残量確認）。
- 取得は http/https のみ、5 MiB 上限、リダイレクト 5 回まで、タイムアウト 5 秒 / 20 秒。
- 抽出した本文は `[untrusted] <url>` を付けて出し、指示文として解釈しない。

## 開発

```bash
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
python3 docs/script/verify-docs.py
```

テストは lib（単体）・`tests/architecture.rs`（層構造・エンジン非依存・警告コードの発行元・決定論性）・`tests/contract.rs`（JSON 定義の契約）・`tests/integration.rs`（検索→取得→アウトライン→節読み）の 4 種。

---

## リポジトリの来歴（テンプレート）

このテンプレートは、Arena.aiのエージェントモードを使用してLLMによる仕様駆動開発を行うためのリポジトリ構成とワークフローのベストプラクティスをまとめたものです。

具体的なアプリケーション（Webアプリ、CLIツール、モバイルアプリ等）に依存しない、**開発プロセス・ドキュメント管理・CI/CD・環境対処法**に特化したテンプレートです。

---

## 概要

このテンプレートレポジトリによるLLM（Large Language Model）による仕様駆動開発は以下の3つの要素で構成されます：

1. **ドキュメント管理戦略**: 仕様書・設計書・計画・履歴を構造化し、LLMと人間の双方が参照・検証可能にする
2. **Arena.ai環境への適応**: サンドボックスの制約（通信制限・スナップショット上限・コンテナ再起動）への対処
3. **CI/CD自動化**: GitHub Actionsによる継続的検証と、PRテンプレートによる品質保証

---

## クイックスタート

1. このテンプレートを元にして新しいリポジトリを作成

2. レポジトリをArena.aiと連携し、以下のプロンプトをArena.aiのエージェントモードで入力する。

```md
レポジトリには、あなた(Agent)と仕様駆動で効率的に開発を行うための汎用的な基盤が用意されています。以下の作業を行なってください。

1. `docs/developers.md`を確認する。
2. 私(ユーザー)に作りたいソフトウェアがどんなものであるか質問する。

私(ユーザー)が質問に答えたら、それに基づき、技術スタック・具体的な仕様など開発を始めるために必要な情報すべてを提案し、`docs/`にそれをドキュメント化・概要をチャットで私(ユーザー)に説明してください。
```

---

## ドキュメント構成

```
docs/
├── script/
│   └── verify-docs.py       # ドキュメント管理規則の機械検証スクリプト
├── specifications/           # 純粋な仕様書（コード情報を含まない）
│   └── overview.md           # 設計原則・用語・アーキテクチャ
├── designs/                  # 実装設計書（仕様書と構造を一致させる）
│   └── overview.md           # 対応ソースコードへの参照を含む
├── handoff.md                # セッション引き継ぎ情報（毎セッション更新）
├── developers.md             # 開発者ガイド（固定情報・規則）
├── plans.md                  # 一時的な計画・進捗
└── history.md                # 変更履歴・意思決定アーカイブ（追記専用）
```

---

## 免責事項

- このレポジトリの保有者であるsonnekoは、Arena.aiとは全く関係ありません。
- Arena.aiを利用する場合は、Arena.aiの利用規約を参照してください。
- このテンプレートレポジトリを利用することにより生じた、いかなるトラブル・不利益について、レポジトリ所有者のsonnekoは一切責任を取りません。

---

## ライセンス

このテンプレート自体は [MIT License](./LICENSE) で提供されます。
