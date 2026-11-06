# 外部依存の導入手順

本書は、Arena サンドボックス環境（通信制限あり）で開発している本リポジトリにおいて、
新たに外部ツールや依存ライブラリを追加する際の手順をまとめる。

---

## 0. 前提：サンドボックスの通信制限

Arena サンドボックスから外部へ直接通信できるホストは以下の 6 つに限定されている。

- `github.com` / `codeload.github.com` / `api.github.com`
- `registry.npmjs.org`
- `pypi.org` / `files.pythonhosted.org`

各種レジストリ（crates.io, static.rust-lang.org, dl.google.com 等）や
GitHub Releases のアセット（objects.githubusercontent.com）には直接到達できない。
そのため、npm / PyPI で取得できるもの以外は **GitHub Actions 上で取得し、
Git Blob や専用ブランチを経由してサンドボックスへ搬入する**。

---

## 1. 外部ツールの導入手順

### 1.1 Git Blob 方式（推奨）

GitHub Releases 等のバイナリを `api.github.com` 経由で Git Blob に保存し、
サンドボックスから取り出す方式。

1. **`.github/workflows/sandbox-tools.yml` に取得ジョブを追加**
2. **`sandbox-tools.env` を `ENABLED=1` にして push**
3. **PR コメントに記録された blob SHA と sha256 を控える**
4. **`scripts/bootstrap-toolchain.sh` で取り出す処理を記述**
5. **取得完了後に `ENABLED=0` に戻す**

### 1.2 npm / PyPI パッケージ

`registry.npmjs.org` / `pypi.org` には直接通信できるため、
GitHub Actions を経由する必要はない。

```bash
npm install --prefix /home/user/build/ <package>
pip install --target /home/user/build/python <package>
```

---

## 2. ライブラリ依存の導入手順

### 2.1 差分追加（少数の依存）

Git Blob 方式で `.crate` / `.tgz` ファイルを展開する。

### 2.2 大規模な依存追加

専用ブランチ（`cache/toolchain` 等）で一括管理する。
`.github/workflows/sandbox-tools.yml` で退避データを生成するジョブを追加する。

---

## 3. 導入後の検証

```bash
bash scripts/verify.sh
```

すべての検査が通過することを確認してからコミットする。
