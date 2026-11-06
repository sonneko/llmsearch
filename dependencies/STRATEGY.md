# `dependencies/` ディレクトリの戦略

本書は `dependencies/` ディレクトリの設計思想と運用ルールを定義する。

---

## 1. 目的

`dependencies/` は、**各技術スタック固有の環境整備方法（ドキュメント＋スクリプト）を保存する場所**である。

Arena.ai サンドボックスには通信制限があり、各言語・ツールの公式配布元（rustup, npm, pip, go, etc.）の多くに直接到達できない。この制約への対処法は技術スタックごとに固有であり、アプリケーション本体のコードとは切り離して管理すべきである。

このディレクトリは **std-app-dev-template 自体の開発用** ではなく、**このテンプレートを使って作成される各プロジェクトの開発環境整備** を支援するために存在する。

---

## 2. 設計思想

### 2.1 テンプレートからの分離

`dependencies/` の内容は、テンプレートのコア（ドキュメント管理戦略、CI設計、verifyスクリプト）とは独立している。理由は：

1. **技術スタックはプロジェクトごとに異なる**: Rust を使うプロジェクトもあれば、TypeScript、Python、Go を使うプロジェクトもある
2. **環境整備はプロジェクト固有**: 各スタックの通信制限への対処法は、そのスタックの特性に強く依存する
3. **不要なら消せる**: 使わない技術スタックのディレクトリは、プロジェクト開始時に削除して問題ない

### 2.2 「移して使う」パターン

`dependencies/<stack>/` のファイルは、プロジェクト開始時に以下の場所に移動して使う：

| dependencies/ 内のファイル | 移動先 |
|---|---|
| `dependencies/<stack>/bootstrap-*.sh` | `scripts/bootstrap-<stack>.sh` |
| `dependencies/<stack>/*.yml` | `.github/workflows/<name>.yml` |
| `dependencies/<stack>/<name>.env` | `.github/<name>.env` |
| `dependencies/<stack>/README.md` | `docs/add-dependencies-<stack>.md`（参考用） |

### 2.3 追加の仕方

新しい技術スタックの環境整備知識を追加する場合は：

1. `dependencies/<stack-name>/` ディレクトリを作成
2. `README.md` に背景・課題・解決策・使い方を記述
3. 必要なスクリプト（bootstrap等）とワークフローファイルを含める
4. `docs/developers.md` の §2 にそのスタックへの参照を追加
5. `GUIDE.md` に「そのスタックが必要なら `dependencies/<stack-name>/` のファイルを移動してください。不要なら消してください。」と追記

---

## 3. 現在含まれる技術スタック

| ディレクトリ | 技術スタック | 内容 |
|---|---|---|
| `rust/` | Rust | ツールチェーン退避方式、vendored crates、WASM対応 |

---

## 4. 各スタック共通の構造

各 `dependencies/<stack>/` は以下のファイルを含むことを推奨する：

```
dependencies/<stack>/
├── README.md              # 必須：背景・課題・解決策・使い方
├── bootstrap-*.sh         # 推奨：サンドボックス用環境復旧スクリプト
├── *-export.yml           # 推奨：GitHub Actions 用退避ワークフロー
├── *.env                  # 任意：ワークフローの有効化フラグ
└── (その他)               # 任意：追加のヘルパースクリプト、設定テンプレート
```

### README.md の必須項目

1. **背景と課題**: なぜこのスタックは特別な対処が必要か
2. **解決策の概要**: どのようなアプローチを取るかの図解
3. **初期セットアップ**: プロジェクト側の配置手順
4. **各コマンドの実行**: 環境変数の source 方法
5. **トラブルシューティング**: よくある問題と対処法
6. **ファイル構成**: ディレクトリ内の各ファイルの説明

---

## 5. 運用ルール

### 5.1 追加時の検証

新しい `dependencies/<stack>/` を追加する際は：

- `README.md` の手順が実際に動作することを検証する
- `bootstrap-*.sh` が少なくとも 1 回は成功することを確認する
- `docs/developers.md` と `GUIDE.md` に参照を追加する

### 5.2 更新時の注意

- `bootstrap-*.sh` の `REPO` 名の推定ロジックは汎用的に保つ
- `*.yml` の `RUST_VERSION` のようなバージョン指定は、利用プロジェクト側で調整できるようにする
- `cache/*` ブランチ名はスタックごとに異なる名前を使う（`cache/rust-toolchain`, `cache/node-toolchain` 等）

### 5.3 削除基準

以下の場合は `dependencies/<stack>/` を削除してもよい：

- その技術スタックをプロジェクトで使用しない場合
- より良い解決策が見つかった場合（旧版は history に記録）

---

## 6. std-app-dev-template 自体との関係

`dependencies/` はテンプレートの「拡張データ」であり、テンプレート自体のビルド・テストには不要。

- `verify-docs.py` は `dependencies/` を検査対象外とする
- CI ワークフローは `dependencies/` の変更ではトリガーしない
- テンプレート利用者は、必要な `dependencies/<stack>/` をプロジェクトに統合してから `dependencies/` ディレクトリごと削除してよい

---

## 7. 将来の拡張候補

以下の技術スタックについて、同様の環境整備ドキュメントの追加が検討される：

- **Node.js / TypeScript**: `node_modules` の Git Blob 退避、`npm ci` の再現性
- **Python**: `pip` / `venv` のオフライン展開、`pyproject.toml` の管理
- **Go**: `go mod vendor` の退避、`GOPROXY` の制限への対処
- **Docker**: Dockerfile のマルチステージビルド、イメージの退避
- **Swift**: Xcode toolchain の代替、SPM の vendoring

各スタックの追加は、Arena.ai で実際にそのスタックを使って開発を行った経験に基づくことが望ましい。
