# 設計書: 設定と構成

本書は `REF:docs/specifications/configuration.md` に対応する実装設計を記述する。パスは実装予定の配置である。

---

## 1. 設定の優先順位

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 1.1 優先順位

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`resolveConfig({ argv, env, file })`（`src/usecases/config/resolve.ts`）が 4 層をマージし、各項目の由来を `ConfigOrigin` として保持する。マージは項目単位で行い、層単位の置き換えは行わない。

### 1.2 設定ファイルの探索順

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

探索は `src/adapters/config/locate.ts`。`--config` 指定時の不在は `E1003`、その他の候補の不在は無視する。読み込んだパスは `ResolvedConfig.path` に保持する。

### 1.3 形式

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

TOML の解析には `smol-toml` を用いる（コメント保持は不要だが、未知キーの温存のため素の `Record` として読み、スキーマ検証は別途行う）。初回起動時の既定ファイル生成は `config init` と同じ関数を呼ぶ。

---

## 2. 設定項目

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 2.1 全項目と既定値

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/core/config/schema.ts` に `zod` スキーマで全項目を定義し、既定値は `DEFAULTS` 定数として同ファイルに置く。仕様の表と 1 対 1 に対応させ、表にない項目を追加しない（追加時は仕様の改訂承認を得る）。

### 2.2 環境変数

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

環境変数の写像は `src/usecases/config/from-env.ts` に表として定義する。真偽値は `1`/`true`/`0`/`false` を受理し、それ以外は `E1004`。

### 2.3 秘匿情報

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

設定スキーマに API キーの項目を持たせない。`config show` は `BRAVE_API_KEY` などの環境変数について「設定済み / 未設定」のみを表示し、値は出力しない。

---

## 3. 検証と未知キー

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 3.1 値の検証

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`zod` の `safeParse` を用い、失敗時は最初の 1 件を `E1004` として返す（複数ある場合は `details` に全件）。検証失敗時はユースケースを開始しない。

### 3.2 未知キーの温存

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

スキーマは `zod.object({...}).passthrough()` とし、未知キーを `unknownKeys` に保持する。`config init --force` で書き戻す際は、既存ファイルの未知キーを再出力する。警告は `W1002` として 1 回だけ表示する。

### 3.3 プロバイダ固有設定の検証

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/usecases/search/select-provider.ts` が選択直後にプロバイダ固有の必須項目を検査し、不足時は `E4002` に不足項目名を含める。

### 3.4 破損した設定ファイル

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

TOML パーサの例外から行・列を取り出し、`E1003` のメッセージに含める。既定値でのフォールバックは行わない。

---

## 4. 表示と初期化

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 4.1 `config show`

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/ui/render/config-text.ts` が「キー / 値 / 由来」の表を出力する。由来は `default` / `file` / `env` / `argv` の 4 値。

### 4.2 `config path`

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

設定ファイルの実パス（不在時は探索候補の一覧）と状態ディレクトリをそれぞれ 1 行で出力する。`--json` 時は 1 オブジェクトにまとめる。

### 4.3 `config init`

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

既定値をコメント付き TOML として書き出すテンプレートは `src/adapters/config/template.ts` に置く。既存ファイルがある場合は `--force` がない限り `E1004`（上書き拒否）を返す。
