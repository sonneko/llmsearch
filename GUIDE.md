# Arena.ai LLM仕様駆動開発ガイド

本書は、Arena.aiのエージェントモードを使用してLLMによる仕様駆動開発を行うための包括的なガイドである。テンプレートの各要素の意図と、具体的な運用方法を解説する。

---

## 1. 仕様駆動開発とは

仕様駆動開発（Specification-Driven Development）は、**コードよりも先に仕様を明文化し、その仕様を検証可能な形で保持し続ける**開発手法である。LLM（Large Language Model）をエージェントとして使用する場合、この手法は特に効果的である：

- LLMは明確な仕様があれば、一貫した実装を生成できる
- 仕様書がLLMへの「プロンプト」として機能し、品質のばらつきを抑制する
- 仕様と実装の乖離を機械的に検出できる

---

## 2. ドキュメント管理戦略

### 2.1 4層構造

ドキュメントは以下の4つの役割で分離する：

| ファイル | 役割 | 更新頻度 |
|---|---|---|
| `docs/specifications/*.md` | 純粋な仕様（ユーザーから見える振る舞い） | 滅に変更。変更時は人間の承認が必要 |
| `docs/designs/*.md` | 実装設計（コードへの対応） | 実装の進捗に応じて更新 |
| `docs/plans.md` | 一時的な計画・進捗管理 | 頻繁に更新。完了時はhistory.mdへ |
| `docs/history.md` | 過去の意思決定アーカイブ | 追記専用。過去の判断理由を保持 |

さらに以下の補助ドキュメントがある：

| ファイル | 役割 |
|---|---|
| `docs/developers.md` | 開発者向け固定情報（規則・環境・制約） |
| `docs/handoff.md` | セッション間の引き継ぎ情報 |

### 2.2 優先順位

矛盾が生じた場合の優先順位は：

```
specifications/*.md > designs/*.md > ソースコード
```

この優先順位により、以下の判断が機械的に可能になる：
- 仕様書にない機能が実装 → 記述漏れ or 不要な実装
- 仕様書にある機能が未実装 → 未実装 or 仕様変更の反映漏れ

### 2.3 歴史情報の分離

「最初は X だったが Y に変更した」といった歴史的情報は、現行の仕様からは分離し `docs/history.md` に移動する。これにより：
- 現行仕様はノイズなく読める
- 過去の判断理由は `grep` で機械的に検索可能

### 2.4 `REF:` 参照記法

ドキュメント間の相互参照はすべて `REF:` プレフィックスを付ける：

```markdown
実装は `REF:src/main.rs` にある。
詳細は `REF:docs/plans.md#milestone-2` を参照。
```

これにより、`docs/script/verify-docs.py` で参照先の存在を機械的に検証できる。

### 2.5 SYNC ブロック

複数ファイル間で重複が避けられない情報（進捗サマリ等）は `SYNC` マーカーで囲む：

```markdown
<!-- SYNC:START project-status -->
| 実装状況 | 完了 50% | ... |
<!-- SYNC:END project-status -->
```

`verify-docs.py` がすべての SYNC ブロックについて、全出現箇所で内容が一字一句一致することを検証する。

### 2.6 仕様書と設計書の構造一致

`docs/specifications/<name>.md` と `docs/designs/<name>.md` は `## ` および `### ` 見出しを完全一致させる。設計書の各 `### ` 節には必ず実装状態を記載する：

```markdown
### 3.1 データモデル

状態: 実装完了（REF:src/core/model.rs）

実装の詳細...
```

---

## 3. Arena.ai環境への適応

### 3.1 通信制限

Arenaサンドボックスから外部通信が可能なのは6ホストのみ：

| ホスト | 用途 |
|---|---|
| `github.com` | Git リポジトリ操作 |
| `codeload.github.com` | tarball ダウンロード |
| `api.github.com` | GitHub REST/GraphQL API |
| `registry.npmjs.org` | npm パッケージ |
| `pypi.org` / `files.pythonhosted.org` | Python パッケージ |

**到達できないもの**：
- `crates.io` / `static.crates.io`（Rustのクレートレジストリ）
- `static.rust-lang.org`（Rust公式配布元）
- `objects.githubusercontent.com`（GitHub Releases のアセット）
- その他の独自ホスト

#### 解決策：Git Blob 方式

到達できないホストから取得すべきバイナリは、GitHub Actions で取得して Git Blob に保存する：

1. `.github/workflows/sandbox-tools.yml` で取得ジョブを定義
2. 取得したアーカイブを base64 エンコードして `gh api` で Git Blob に POST
3. PR コメントに `blob=<SHA>` と `sha256=<HASH>` を記録
4. サンドボックス側で `gh api repos/$REPO/git/blobs/$BLOB_SHA --jq .content | base64 -d` で取り出し

この方式の利点：
- `api.github.com` は到達可能なので、サンドボックスから取得できる
- SHA256 で完全性を検証できる
- どのブランチの履歴も汚さない（unreachable blob）

### 3.2 スナップショットの制約

| 制約 | 対策 |
|---|---|
| 128MB / 10,000ファイルの上限 | 大きな成果物は `/home/user/build/` や除外ディレクトリに配置 |
| 除外ディレクトリ名の利用 | `target/`, `dist/`, `build/`, `node_modules/`, `.cache/` 等は永続化されない |
| `/home/user/build/` の消滅 | コンテナ再起動時に消える → `bootstrap-toolchain.sh` で再展開 |

### 3.3 コンテナ再起動への対処

セッションが長くなるとコンテナが再起動し、以下の状態になる：
- `/home/user/build/` が消える
- `.git/refs/heads/...` が古い状態に戻る

**復旧手順**：
```bash
bash scripts/bootstrap-toolchain.sh
git fetch origin <現在のブランチ名>
git reset --mixed FETCH_HEAD
```

### 3.4 `gh pr edit` の回避

`gh pr edit` は GraphQL の Classic Projects 廃止エラーで失敗する場合がある。代替として REST API を直接呼ぶ：

```bash
gh api repos/<owner>/<repo>/pulls/<PR番号> -X PATCH --input - <<'JSON'
{"title": "...", "body": "..."}
JSON
```

---

## 4. GitHub Actions の設計パターン

### 4.1 変更フィルタリング

ドキュメントのみの変更で重いビルド・テストジョブをスキップする：

```yaml
jobs:
  docs:
    outputs:
      code_changed: ${{ steps.filter.outputs.code_changed }}
  test:
    needs: docs
    if: needs.docs.outputs.code_changed == 'true'
```

これにより、仕様書のtypo修正で30分のCIを待つ必要がなくなる。

### 4.2 Draft PR の活用

```yaml
on:
  pull_request:
    types: [opened, synchronize, reopened, ready_for_review]
    
jobs:
  test:
    if: github.event_name != 'pull_request' || !github.event.pull_request.draft
```

Draft PR 中はCIをスキップし、Ready for review にした時点で走る。

### 4.3 コンカレンシー制御

```yaml
concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: ${{ github.event_name == 'pull_request' }}
```

PR の push で古い CI を自動キャンセルする。

### 4.4 CI レポートスクリプト

CI の出力をそのままログに流すだけでなく、以下を同時生成する：

1. **GitHub Annotations**: PR の Files changed タブにインライン表示
2. **Job Summary**: Actions の実行ページに Markdown テーブル
3. **PR コメント**: 失敗時のみ投稿される詳細なエラーレポート

---

## 5. LLMエージェントとの協業パターン

### 5.1 自律判断と人間承認の境界

| 種類 | エージェントの権限 |
|---|---|
| 仕様書に書いてあることの実装 | 自律的に進める |
| バグ修正 | 自律的に進める |
| テストの追加 | 自律的に進める |
| 仕様書の変更 | 人間の承認が必要 |
| 設計書の変更 | 人間の承認が必要 |

### 5.2 ブランチ運用

- Arena.aiのセッションに紐づく専用作業ブランチ（`arena/...`）でのみ作業
- 作業の区切りごとに**こまめにコミット & プッシュ**
- PR のマージは必ずユーザー自身が行う

### 5.3 PR テンプレート

PR テンプレート（`.github/pull_request_template.md`）は、LLMがPRを作成する際のチェックリストとして機能する：

```markdown
- [ ] 仕様書・計画書の該当章を参照した
- [ ] `python3 docs/script/verify-docs.py` が通過する
- [ ] CI の全ジョブが緑
- [ ] 仕様書や設計書に変更がある場合は人間の承認を得た
```

### 5.4 セッション引き継ぎ

`docs/handoff.md` はセッション終了時に必ず更新する。次セッション開始時にLLMが読む最初のファイルであり、以下を含む：

- 現在の進捗サマリ
- 作業中のブランチとPR
- 直近の検証状態
- 次セッション開始時の手順

---

## 6. 検証スクリプトの設計

### 6.1 `docs/script/verify-docs.py` の検査項目

| 規則 | 内容 |
|---|---|
| rule-1 | 必須ファイルの存在確認 |
| rule-2 | `developers.md` に管理規則の原文が含まれていること |
| rule-3 | `specifications/*.md` にコード情報・他ファイル参照が含まれないこと |
| rule-4 | `specifications` と `designs` の見出し構成が一致し、各節に実装状態があること |
| rule-5 | すべての `REF:` 参照が存在するファイルを指していること |
| rule-6 | `history.md` の全エントリが grep 検索用の固定ヘッダを持つこと |
| rule-7 | SYNC ブロックの内容が全出現箇所で一致し、SYNC 外の段落重複がないこと |

### 6.2 `scripts/verify.sh` の役割

コミット前に CI と同じ検証を手元で回すスクリプト。以下の順序で実行：

1. ドキュメント規則の検証
2. 静的解析・フォーマットチェック
3. テスト
4. ビルド（`--quick` で省略可能）

### 6.3 検証の自動化

すべての検証は `exit 0` / `exit 1` で結果を返す。CI と同じスクリプトを使うことで、「CI で通らないものはローカルでも通らない」ことを保証する。

---

## 7. テンプレートの使い方

### 7.1 新規プロジェクトの開始

1. このテンプレートをコピーして新しいリポジトリを作成
2. `docs/specifications/overview.md` にアプリケーションの仕様を記述
3. `docs/designs/overview.md` に対応する設計を記述
4. `docs/developers.md` をプロジェクト固有の情報に調整
5. `.github/workflows/ci.yml` の lint / test / build をプロジェクト固有のコマンドに置換
6. `scripts/bootstrap-toolchain.sh` をプロジェクトのツールチェーンに合わせて調整
7. **技術スタックの環境整備**: `dependencies/` ディレクトリを参照（後述）

### 7.1.1 `dependencies/` の扱い

`dependencies/` には各技術スタック固有の環境整備ドキュメントとスクリプトが含まれている（詳細は `REF:dependencies/STRATEGY.md`）。

プロジェクトで使用する技術スタックに応じて以下を行う：

- **Rust を使う場合**: `dependencies/rust/` のファイルをプロジェクトに移動する
  ```bash
  cp dependencies/rust/bootstrap-rust.sh scripts/bootstrap-rust.sh
  cp dependencies/rust/toolchain-export.yml .github/workflows/toolchain-export.yml
  cp dependencies/rust/toolchain.env .github/toolchain.env
  # dependencies/rust/README.md の内容を docs/add-dependencies.md に追記してもよい
  ```
  詳しい手順は `dependencies/rust/README.md` を参照すること。

- **Rust を使わない場合**: `dependencies/rust/` を削除して問題ない
  ```bash
  rm -rf dependencies/rust/
  ```

- **他の技術スタックが必要な場合**: `dependencies/STRATEGY.md` の手順に従って新しい `dependencies/<stack>/` を追加する

`dependencies/` のファイルはテンプレート利用時の参考情報であり、プロジェクトに統合後は `dependencies/` ディレクトリごと削除しても問題ない。

### 7.2 LLMエージェントへの指示

Arena.ai で開発を開始する際、以下の情報をプロンプトに含める：

1. `docs/developers.md` の内容（規則・環境・制約）
2. `docs/handoff.md` の内容（現在の進捗）
3. `docs/plans.md` の内容（次の作業）
4. 開発の進め方（自律判断の境界、ブランチ運用等）

### 7.3 運用のサイクル

```
仕様策定 → 設計 → 実装 → 検証 → 反映
    ↑                              |
    └──────────────────────────────┘
```

1. **仕様策定**: `docs/specifications/` に明文化（人間の承認が必要）
2. **設計**: `docs/designs/` に実装方針を記述
3. **実装**: ソースコードを書く
4. **検証**: `scripts/verify.sh` で全検査
5. **反映**: 完了した項目を `docs/plans.md` から `docs/history.md` へ移動

---

## 8. 応用パターン

### 8.1 複数エージェントでの開発

- 1つのPRで1つのセッション（1人のエージェント）
- 異なる機能は異なるブランチ・PR で作業
- ドキュメント管理規則が共通の「契約」として機能する

### 8.2 大規模プロジェクトでの拡張

- `docs/specifications/` を機能モジュールごとに分割（例: `auth.md`, `api.md`, `ui.md`）
- `docs/designs/` も同じ構造で追従
- `verify-docs.py` の検査を拡張

### 8.3 CI/CD パイプラインの拡張

- デプロイワークフロー（`.github/workflows/deploy.yml`）
- リリースワークフロー（バージョン tagging に応じた自動リリース）
- セキュリティスキャン

---

## 9. よくある課題と解決策

### 9.1 仕様書の肥大化

- 歴史的情報は `docs/history.md` に分離
- 機能モジュールごとにファイルを分割
- `grep` で機械的に検索可能な構造を維持

### 9.2 LLMが過去の判断を忘れる

- `docs/history.md` に `[HIST-TAG: ...]` 形式でタグ付け
- `grep` で検索可能な構造により、LLMが自律的に参照できる

### 9.3 ツールチェーンの復旧

- `scripts/bootstrap-toolchain.sh` でワンコマンド復旧
- Git Blob 方式で外部依存を管理

### 9.4 PR の競合

- こまめなコミット & プッシュ
- `scripts/verify.sh` でローカル検証を徹底
- CI の変更フィルタリングでフィードバックループを高速化
