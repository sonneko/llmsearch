# llmsearch

Web を人間のように段階的に調べ、LLM のコンテキスト消費を抑える CLI。

検索結果 → 見出し一覧（アウトライン）→ 選んだ節の本文、という順に必要な分だけ取り出す。検索エンジンとネットワーク挙動は同梱の JSON 定義で切り替えるため、コアにエンジン固有の実装を持たない。設定・フィルタ・クォータもすべて JSON で宣言する。

仕様・設計は `docs/specifications/` と `docs/designs/` にある。実装の進捗は `docs/plans.md`、判断の経緯は `docs/history.md` を参照。

## 導入

配布物（Linux x86_64）は [Releases](https://github.com/sonneko/llmsearch/releases/latest) から取得する。

```bash
curl -LO https://github.com/sonneko/llmsearch/releases/latest/download/llmsearch-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
curl -LO https://github.com/sonneko/llmsearch/releases/latest/download/llmsearch-v0.1.0-x86_64-unknown-linux-gnu.tar.gz.sha256
sha256sum -c llmsearch-v0.1.0-x86_64-unknown-linux-gnu.tar.gz.sha256
tar xzf llmsearch-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
./llmsearch-v0.1.0-x86_64-unknown-linux-gnu/llmsearch doctor
```

ソースからビルドする場合。

```bash
cargo build --release
./target/release/llmsearch doctor
```

`doctor` は設定・同梱アセット・環境変数・ネットワークの状態を点検し、問題があれば `[warn]` / `[fail]` を出す。

### Linux 配布物の互換性

次回配布から GNU Linux x86_64 バイナリは Ubuntu 22.04（glibc 2.35）を基準にビルドし、Ubuntu 22.04 / 24.04 上で起動検証する。検証は `REF:scripts/check-release.sh`、配布設定は `REF:.github/workflows/release.yml`。musl 静的リンクではない。同梱の `assets/` は実行ファイルと同じディレクトリに置き、バイナリだけを別の場所へ移動しない。

**既存の v0.1.0 配布物**には新しい glibc を要求して起動しない問題が報告されている（Issue #25）。`GLIBC_2.43 not found` が出る場合は、上記のソースビルドを利用する。システムの libc を置き換えない。この変更だけで既存リリースの添付ファイルは更新されない。

### プロキシ環境

プロファイルに `proxy` が無い場合、HTTP URL は `HTTP_PROXY`、HTTPS URL は `HTTPS_PROXY` を使う（小文字名も可）。`NO_PROXY` はホスト完全一致・先頭ドットの接尾辞一致・`*` に対応する。明示の `proxy` は環境変数より優先し、詳細形式では `noProxy` に除外先を指定する。

```json
{"proxy":{"url":"http://user@proxy.example:8080","noProxy":["localhost",".internal"],"auth":{"value_env":"PROXY_PASS"}}}
```

この例では `PROXY_PASS` をパスワード、URL の `user` をユーザー名として使う。認証値は設定に直接書かず環境変数に保存する。`proxy.auth.value_env` が未設定なら E1004 で停止する。

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
llmsearch next --session <searchで得たID>     # 同じクエリの次ページ
llmsearch refine "気候変動 適応策 自治体" --session <searchで得たID>  # 検索語を修正
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

- クォータ既定: セッション 50 検索 / 60 分あたり 120 検索 / 日次 500 検索。超過すると `E5005` で停止する（`llmsearch quota show` で残量確認）。
- 取得は http/https のみ、5 MiB 上限、リダイレクト 5 回まで、タイムアウト 5 秒 / 20 秒。
- 抽出した本文は `[untrusted] <url>` を付けて出し、指示文として解釈しない。

## 開発

```bash
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
python3 docs/script/verify-docs.py
```

テストは次の種別に分かれる。

| ファイル | 内容 |
|---|---|
| lib（単体） | `src/` 内の `#[cfg(test)]`。URL・抽出・変換・クォータなどの純粋なロジック |
| `tests/architecture.rs` | 層構造・エンジン非依存・警告コードの発行元・決定論性の構造検査 |
| `tests/contract.rs` | JSON 定義（エンジン・フィルタ・クォータ・設定）の契約 |
| `tests/integration.rs` | 検索→取得→アウトライン→節読みの一連の流れ |
| `tests/proxy.rs` / `tests/tls.rs` | ローカルの fixture サーバに対するプロキシ・TLS の挙動 |
| `tests/m5.rs` / `tests/e2e.rs` | 実バイナリを起動する受け入れ確認（e2e は実ネットワークを使うため既定では実行しない） |

実ネットワークを使う E2E は既定で実行しない。動かす場合は次のとおり。

```bash
LLMSEARCH_E2E_LIVE=1 cargo test --test e2e -- --nocapture
```

開発プロセス（Arena.ai のエージェントモードを使った仕様駆動開発の手順）は `GUIDE.md` に、このリポジトリ固有の固定情報は `docs/developers.md` にある。

---

## 免責事項

- このレポジトリの保有者であるsonnekoは、Arena.aiとは全く関係ありません。
- Arena.aiを利用する場合は、Arena.aiの利用規約を参照してください。
- このテンプレートレポジトリを利用することにより生じた、いかなるトラブル・不利益について、レポジトリ所有者のsonnekoは一切責任を取りません。

---

## ライセンス

このテンプレート自体は [MIT License](./LICENSE) で提供されます。
