# 設計書: コマンドラインインターフェース

本書は `REF:docs/specifications/cli-interface.md` に対応する実装設計を記述する。

---

## 1. 起動とモード

状態: 実装完了（REF:src/ui/cli.rs, REF:src/ui/interactive.rs）

CLI は `clap` の derive API で定義する。`src/ui/cli.rs` に `Cli`（グローバルオプション）と `Command`（サブコマンドの列挙）を置き、`main.rs` は `Cli::parse()` → `app::run(cli)` の 2 行のみとする。

### 1.1 起動形態

状態: 実装完了（REF:src/ui/cli.rs, REF:src/ui/interactive.rs）

`Command::None` の場合は `ui::cli::interactive::run` を、それ以外は `ui::cli::commands::dispatch` を呼ぶ。両者は同一の `usecases` 関数を呼ぶ。

### 1.2 モード判定

状態: 実装完了（REF:src/ui/cli.rs, REF:src/ui/interactive.rs）

`ports::Env::is_stdin_tty()` / `is_stdout_tty()` で判定する。`is_terminal` クレートではなく `std::io::IsTerminal` を用い、`adapters::env::process_env` に閉じ込める。`E1001` のメッセージにはサブコマンド一覧を含める（`clap` の `Command::render_help` から生成する）。

### 1.3 共通オプション

状態: 実装完了（REF:src/ui/cli.rs, REF:src/ui/interactive.rs）

`GlobalOptions` 構造体に集約し、`resolve` 層への入力とする。`--engines-dir` は `Vec<PathBuf>` で複数指定を受け付ける。`--language <tag>` を追加し（M10）、`Bootstrap.language` として `search.language` を上書きする。`--language` を指定できるのは全体設定の一時上書きとして扱い、`--json` の `config show` でも由来 `argv` として出る。`--no-filter` は `Option<Option<String>>`（値なしは `results+content`、`all` は既定除去要素も対象）として解釈する。

---

## 2. 対話モードのメニュー

状態: 実装完了（REF:src/ui/cli.rs, REF:src/ui/interactive.rs）

### 2.1 検索結果メニュー

状態: 実装完了（REF:src/ui/cli.rs, REF:src/ui/interactive.rs）

`src/ui/menu.rs` の `Screen`（`Query` / `Results` / `SelectResults` / `Outline` / `SelectSections` / `History`）と `parse_input(screen, line) -> Input` が入力の解釈を担い、`results_menu()` / `outline_menu()` が表示文言を返す。番号と項目の対応は両関数に固定し、`tests` の `results_menu_numbers_are_fixed` が並びを検査する。状態遷移そのものは `REF:src/ui/interactive.rs` のループが持つ。

### 2.2 結果選択の入力規則

状態: 実装完了（REF:src/core/select.rs）

`src/core/select.rs` の `parse_selection(input, max_ref) -> Result<SelectionSpec, SelectionError>`。`1,3-5` と `R1,R3` の双方を受け付け、`SelectionSpec { refs: Vec<u32> }` へ正規化する（重複は除去し、順序は入力順）。不正入力の連続回数は画面状態が保持する。

### 2.3 アウトライン・メニュー

状態: 実装完了（REF:src/ui/cli.rs, REF:src/ui/interactive.rs）

取得済みの文書からアウトラインを描画する。深度変更（メニュー 3）は `session.outline_depth` を更新して同じ文書から再描画し、`usecases::document::fetch` を再度呼ばない。メニュー 5 は `history_uc::list` の先頭 10 件を `Renderer::history_line` で表示する。

### 2.4 節本文の表示形式

状態: 実装完了（REF:src/ui/render.rs）

`Renderer::section_block` がヘッダ・信頼境界・本文・終端を描画する。本文中のマーカーは `neutralize_markers` で無害化する。

### 2.5 通常の情報行

状態: 実装完了（REF:src/ui/render.rs）

`src/ui/interactive.rs` が `search: {n} results {filter summary} / {session usage}`、`fetched D{id} {chars} chars / {session usage}`、`(output {chars} chars / budget {budget} chars / {session usage})` の 3 種類を組み立てる。数値は `usecases` が返す `FilterStats` と `usecases::guard::usage_line(metric)` から取り、描画側で再計算しない。

### 2.6 中断と復帰

状態: 実装完了（REF:src/ui/cli.rs, REF:src/ui/interactive.rs）

`src/ui/interactive.rs` の `read_line(input)` が読み取り 1 回分を返し、`Ok(0)`（入力終端）を `None` として終了に写す。シグナルハンドラは持たず、割り込みの扱いは OS と端末の既定動作に任せる（非同期ランタイムは `reqwest` の内部でのみ使い、入力待ちは同期の `read_line` で行う）。

---

## 3. 非対話サブコマンド

状態: 実装完了（REF:src/ui/cli.rs, REF:src/ui/interactive.rs）

### 3.1 コマンド一覧

状態: 実装完了（REF:src/ui/cli.rs, REF:src/ui/interactive.rs）

サブコマンドの定義は `REF:src/ui/cli.rs` の `clap` derive に置き、実行時の振り分けは `REF:src/ui/commands.rs` の `run(app, renderer, command) -> Result<i32, AppError>` が 1 か所で行う（機能群ごとの `run_engines` / `run_network` / `run_filters` / `run_quota` / `run_config` / `run_history` / `run_doctor`）。

`REF:src/ui/commands.rs` が非対話コマンドを振り分ける。`search --limit` と `outline --depth` は省略時に解決済み設定値を使い、指定時のみ上書きする。`read` は既定で子節を含み、`--no-children` で除外する（`--children` と排他）。`REF:src/main.rs` は非対話でも `--session` を App に適用する。回帰テストは `REF:tests/m5.rs`。

### 3.2 対象の指定方法

状態: 実装完了（REF:src/ui/commands.rs）

`src/ui/commands.rs` の `resolve_target(app, input) -> Result<(session_id, Url)>`。`Target` は `Url(Url)` / `ResultId(u32)` / `DocumentId(u32)`。数値はまず結果番号、次に文書番号として解釈し、どちらも無ければ `E1006`（利用可能な識別子を `details` に添える）。

セッションの扱いは入口で分ける。URL 指定は `app.ensure_session()` を呼んでから解決する（取得だけならセッション未作成でも成立するため）。識別子指定は `app.current_session_id()` が空なら `E5001` と「`search` で開始する」hint を返し、セッションを暗黙に作らない（失敗した参照解決で状態を変えない）。`next` も同じ規則で、セッション無しは `E5001` とする（`REF:tests/m6.rs`）。

### 3.3 終了コード

状態: 実装完了（REF:src/core/errors.rs）

`src/core/errors.rs` の `AppError::exit_code()` が `ErrorKind`（`Input` / `Network` / `Convert` / `Provider` / `State`）から 1〜5 を導出する。`REF:src/main.rs` は返ってきた値で `std::process::exit(code)` を 1 回だけ呼ぶ。割り込み（`SIGINT`）は捕捉せず、OS と端末の既定動作に従う。

---

## 4. 出力チャネルと終了コード

状態: 実装完了（REF:src/ui/cli.rs, REF:src/ui/interactive.rs）

### 4.1 チャネルの分離

状態: 実装完了（REF:src/ui/cli.rs, REF:src/ui/interactive.rs）

`trait Reporter { fn info(&self, message: &str); fn warn(&self, diagnostic: &Diagnostic); fn verbose(&self, message: &str); }`（`REF:src/ports/mod.rs`）。実装は `src/ui/mod.rs` の `StdReporter`（進捗・警告は標準エラー）と、テスト用の無音実装。標準出力へ出る結果データは `Renderer`（`REF:src/ui/render.rs`）が担当し、役割を分ける。

### 4.2 テキスト出力の規則

状態: 実装完了（REF:src/ui/cli.rs, REF:src/ui/interactive.rs）

描画関数は `src/ui/render.rs` に集約し、`Renderer::out` に渡す文字列を組み立てる。色の有無は `Renderer` の `color` フラグ（`ui.color` / `NO_COLOR` / `--no-color` / tty 判定）で決める。

文言は英語で固定する（M10）。`[untrusted] <url>` マーカー、`[truncated: ...]`、件数・クォータの行など既存の書式は変えず、語だけを英語にする。`[使用]` / `[候補]` の接頭辞は `[used]` / `[candidate]` にする。外部から得た文字列（タイトル・本文・`description`）は翻訳しない。日本語リテラルの混入は `REF:tests/architecture.rs` の検査で防ぐ。

### 4.3 JSON 出力の規則

状態: 実装完了（REF:src/ui/render.rs）

`Renderer::envelope(event, fields)` が共通枠を作り、`Renderer::event` が `serde_json::to_string` で 1 行を標準出力へ書く。本文は必ず文字列フィールドに入れる。`at` は注入された `Clock` から取得する。

---

## 5. キーボード・入力規則

状態: 実装完了（REF:src/ui/cli.rs, REF:src/ui/interactive.rs）

### 5.1 入力プロンプト

状態: 実装完了（REF:src/core/select.rs）

入力の解釈は `src/ui/menu.rs` の `parse_input` に集約する。`src/core/select.rs` は選択範囲（`1,3` / `1-3` / `R1` など）の構文解析だけを行い、区切りはカンマ・読点・空白を受け付ける。行編集は自前実装せず、端末に任せる（`rustyline` 等の追加依存を持たない）。不正入力の回数は `REF:src/ui/interactive.rs` が数え、3 回でメニューへ戻す。

### 5.2 表示幅

状態: 実装完了（REF:src/ui/render.rs）

`src/ui/render.rs` の `display_width(&str) -> usize`（`unicode_width`）と `truncate_display(&str, width) -> String`。予算計算は `char` 数（`core::text::chars`）で行い、表示幅とは別関数として混同しない。折り返しは `wrap_ansi` 相当を自前で実装せず、`ui/render/wrap.rs` で空白区切りに行を分割する。
