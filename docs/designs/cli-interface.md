# 設計書: コマンドラインインターフェース

本書は `REF:docs/specifications/cli-interface.md` に対応する実装設計を記述する。パスは実装予定の配置である。

---

## 1. 起動とモード

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

<!-- 時計・乱数源の注入、run(argv) の戻り値を終了コードへ変換する責務の配置 -->

エントリポイントは `src/main.ts` とし、引数を `Command`（判別可能ユニオン）へ変換したうえで、対話モードか非対話モードかを判定する。`process.exitCode` の設定はエントリポイントのみが行い、以降の層は数値を返さない。

### 1.1 起動形態

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`Command` の種別が `interactive` の場合は `src/ui/cli/interactive.ts` の `InteractiveSession` を、それ以外は `src/ui/cli/commands/` 配下の各コマンドを実行する。両者は同一のユースケース関数を呼ぶ。

### 1.2 モード判定

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`stdin.isTTY` / `stdout.isTTY` は `Env` ポート経由で取得する。対話モードで `stdin.isTTY` が偽の場合は `E1001` を生成し、利用可能なサブコマンドを標準エラーへ列挙する。TTY 判定を `process` から直接読む箇所は `src/adapters/env/process-env.ts` に限定する。

### 1.3 共通オプション

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

共通オプション（`--json`, `--session`, `--provider`, `--config`, `--state-dir`, `--no-color`, `--quiet`, `--verbose`）を 1 つのパーサ（`src/ui/cli/options.ts`）で解釈し、`GlobalOptions` 型へ集約する。解決順は「引数 → 環境変数 → 設定 → 既定値」で、解決ロジックは `src/usecases/config/resolve.ts` に置く。

---

## 2. 対話モードのメニュー

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 2.1 検索結果メニュー

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

メニューは `ScreenState`（`results` / `select` / `outline` / `read` / `depth` / `history` / `done`）の状態機械として実装する。各状態は「描画関数」と「入力ハンドラ（文字列 → `Transition`）」を持ち、遷移を純粋関数として単体テストする。番号と項目の対応は定数 `RESULT_MENU` に固定し、描画と解釈が同じ定数を参照する。

### 2.2 結果選択の入力規則

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`R1,3-4` 形式の解釈は純粋関数 `parseSelection(input, maxRef)`（`src/core/parse/selection.ts`）で行う。戻り値は `Ok<SelectionSpec>` または `Err<SelectionError>` とし、警告文言は描画側で組み立てる。3 回連続の不正入力カウントは画面状態が保持する。

### 2.3 アウトライン・メニュー

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

アウトラインは `OutlineView` を描画するのみで、字下げ・文字数・深度超過件数の整形は `src/ui/render/outline-text.ts` が担う。深度変更は再取得を行わず、メモリ上の `Outline` から再描画する。

### 2.4 節本文の表示形式

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

ヘッダ行・信頼境界マーカー・切り詰め注記の生成は `renderSection(view)`（`src/ui/render/section-text.ts`）に閉じる。マーカーは定数 `UNTRUSTED_BEGIN` / `UNTRUSTED_END` とし、本文中に同一文字列が現れた場合の無害化を `sanitizeMarker()` で行う（`REF:docs/designs/context-assembly.md` §5.2）。

### 2.5 中断と復帰

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`SIGINT` は `readline` の `SIGINT` イベントで捕捉し、画面状態へ「キャンセル」遷移を渡す。メニュー画面での連続 2 回は終了遷移とし、終了コード `0` を返す。`Ctrl+D`（`close` イベント）も終了として扱う。

---

## 3. 非対話サブコマンド

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 3.1 コマンド一覧

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

各サブコマンドは `src/ui/cli/commands/<name>.ts` に 1 ファイルずつ置き、`CommandSpec`（名前・引数・オプション・実行関数）をエクスポートする。`src/ui/cli/commands/index.ts` でレジストリ化し、`--help` の生成とディスパッチが同じ定義を参照する。

### 3.2 対象の指定方法

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`outline` / `read` の対象解決は `resolveTarget(input, session)`（`src/usecases/target/resolve-target.ts`）で行う。URL・`R` 識別子・`D` 識別子・数値の順に解釈し、解決できない場合は `E1006` と利用可能な識別子の一覧を返す。

### 3.3 終了コード

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

エラーコードの先頭桁から終了コードを導出する純粋関数 `exitCodeFor(code)`（`src/ui/cli/exit-code.ts`）を置き、割り込みは `130` とする。コマンド実装は終了コードを直接記述しない。

---

## 4. 出力チャネルと終了コード

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 4.1 チャネルの分離

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

出力は `Reporter` ポート（`data(line)`, `info(line)`, `warn(code, line)`, `error(err)`）経由とする。`data` は標準出力、それ以外は標準エラーへ書き出す。層の内部から `console.log` を直接呼ばないことを `tests/architecture/no-console.test.ts` で検査する。

### 4.2 テキスト出力の規則

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

テキスト描画は `src/ui/render/` 配下の関数群に集約する。色付けは TTY 判定と `--no-color` / `NO_COLOR` から決定し、`src/ui/render/color.ts` で一元管理する。

### 4.3 JSON 出力の規則

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

JSON Lines の生成は `emit(event)`（`src/ui/render/json-events.ts`）で行い、`v`（スキーマ版）と `at`（注入した時計）を必ず付与する。改行を含む本文は文字列フィールドへ格納し、1 行 1 オブジェクトを保証する。

---

## 5. キーボード・入力規則

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 5.1 入力プロンプト

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`readline` の `question` をラップし、`InputResult`（`value` / `cancelled` / `eof`）を返す（`src/ui/cli/input.ts`）。検索語の履歴は `SessionState.queries` を参照し、`!<n>` の解釈は `src/core/parse/reuse.ts` に置く。

### 5.2 表示幅

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

全角判定と切り詰めは `src/ui/render/width.ts` に実装し、`…` の付与と端末幅による URL 省略をここで行う。文字数（書記素）と表示幅（桁数）を別関数として扱い、予算計算には書記素を用いる。
