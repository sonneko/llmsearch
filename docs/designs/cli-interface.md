# 設計書: コマンドラインインターフェース

本書は `REF:docs/specifications/cli-interface.md` に対応する実装設計を記述する。

---

## 1. 起動とモード

状態: 実装完了（REF:src/main.rs, REF:src/ui/cli.rs, REF:src/ui/interactive.rs）

CLI は `clap` の derive API で定義する。`src/ui/cli.rs` に `Cli`（グローバルオプション）と `Command`（サブコマンドの列挙）を置き、`src/main.rs` が引数解析・`Bootstrap` 構築・秘匿値登録・非対話／対話モードへの振り分けと終了コード反映を担う。

### 1.1 起動形態

状態: 実装完了（REF:src/main.rs, REF:src/ui/cli.rs, REF:src/ui/interactive.rs）

サブコマンド省略時は `src/main.rs` が TTY 検査のうえで `ui::interactive::Interactive::run` を回し、サブコマンド指定時は `ui::commands::run` へ分岐する（現状、対話モードの出力先として `std::io::stdout()` を渡している点は仕様 §1.2 との乖離として Issue #63 で追跡）。両者は同一の `usecases` 関数を呼ぶ。

### 1.2 モード判定

状態: 実装完了（REF:src/ui/cli.rs, REF:src/ui/interactive.rs）

`ports::Env::is_stdin_tty()` / `is_stdout_tty()` で判定する。`is_terminal` クレートではなく `std::io::IsTerminal` を用い、`ports::system::SystemEnv`（既定実装）に閉じ込める。サブコマンド無しで stdin が tty でない場合は `E1001`（メッセージに `llmsearch search "..."` の例を含める）を返す。引数解析の誤りは `clap` の `Cli::parse()` がそのまま処理し、`E1001` にはしない。

### 1.3 共通オプション

状態: 実装完了（REF:src/ui/cli.rs, REF:src/ui/interactive.rs）

`Global` 構造体（`Cli` に `#[command(flatten)]`）に集約し、`main.rs` が `Bootstrap` へ写して `resolve` 層への入力とする。`--engines-dir` は `Vec<PathBuf>` で複数指定を受け付ける。`--language <tag>` を追加し（M10）、`Bootstrap.language` として `search.language` を上書きする。`--language` は全体設定の一時上書きとして扱い、`--json` の `config show` でも由来 `argv` として出る。`--no-filter` / `--no-cache` / `--refresh` / `--no-color` / `--color` は値なしの真偽フラグ（`bool`）とする。`--color` は `--no-color` と排他的で、テキスト出力では tty 判定を上書きして ANSI 色を強制する。`--json` では ANSI 色を常に無効にし、`--color` 併用時は `--quiet` でない限り stderr の W1001 と独立した `warnings` JSON イベントを出す。`--quiet` は進捗・警告テキスト・セッション案内を抑制するが、操作イベント内の構造化警告配列は保持する。

---

## 2. 対話モードのメニュー

状態: 実装完了（REF:src/ui/cli.rs, REF:src/ui/interactive.rs）

### 2.1 検索結果メニュー

状態: 実装完了（REF:src/ui/cli.rs, REF:src/ui/interactive.rs）

`src/ui/menu.rs` の `Screen`（`Query` / `Results` / `SelectResults` / `Outline` / `SelectSections` / `History`）と `parse_input(screen, line) -> Input` が入力の解釈を担い、`results_menu(next_result_id, has_more)` / `outline_menu()` が表示文言を返す。結果メニューの 3 番は次の `R<n>`、または終端時の `No more results` を表示する。番号と項目の対応は関数に固定し、`tests` の `results_menu_numbers_are_fixed_and_continuation_is_dynamic` が検査する。状態遷移そのものは `REF:src/ui/interactive.rs` のループが持つ。

### 2.2 結果選択の入力規則

状態: 実装完了（REF:src/core/select.rs）

`src/core/select.rs` の `parse(input) -> Result<Vec<Selection>, AppError>`。`1,3-5` と `R1,R3` の双方を受け付け、`SelectionSpec { refs: Vec<u32> }` へ正規化する（重複は除去し、順序は入力順）。不正入力の連続回数は画面状態が保持する。

### 2.3 アウトライン・メニュー

状態: 実装完了（REF:src/ui/cli.rs, REF:src/ui/interactive.rs）

取得済みの文書からアウトラインを描画する。深度変更（メニュー 3）は `session.outline_depth` を更新して同じ文書から再描画し、`usecases::document::fetch` を再度呼ばない。メニュー 5 は `history_uc::list` の先頭 20 件を `Renderer::history_line` で表示する（対話モードのページ送り開始位置・`4) Edit the query` 等の既知課題は Issue #63 で追跡）。

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

`REF:src/ui/commands.rs` が非対話コマンドを振り分ける。`search --limit` は 1〜30 の範囲を検証し、`--page` は 1 から engine の有効結果上限内に制限する。`outline --depth` は省略時に設定値を使い、明示値は 2〜4 にクランプする。`read` は既定で子節を含み、`--no-children` で除外する（`--children` と排他）。`REF:src/main.rs` は `--session` を App に適用し、既存 ID であることを検証する。回帰テストは `REF:tests/m5.rs` と `REF:tests/m14.rs`。

### 3.2 対象の指定方法

状態: 実装完了（REF:src/ui/commands.rs）

`src/ui/commands.rs` の `resolve_target(app, input) -> Result<(session_id, Url)>`。`Target` は `Url(Url)` / `ResultId(u32)` / `DocumentId(u32)`。数値は結果番号として解釈し、無ければ `E1006`（利用可能な識別子を detail に添える）。`read --section` は `parse_sections` が選択パーサーの汎用 hint を `S0` / `S3` / `D1:S3` の例へ置き換え、実在しない節 ID を `document::read` が `E1006` と `outline` hint で返す。

セッションの扱いは入口で分ける。URL 指定は必要なら `app.ensure_session()` で ID を割り当ててから解決する。識別子指定は `app.require_existing_session()` が現行セッションまたは `counters.json` の `session_key` が指す既存セッションを再開し、どちらも無ければ `E5001` を返す（参照解決のために新規状態を作らない）。`next` / `refine` も既存セッションを要求し、暗黙再開時は ID を標準エラーへ通知する。明示 `--session` は `main.rs` が既存 ID のみ受け付ける（`REF:tests/m14.rs`）。

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

描画関数は `src/ui/render.rs` に集約し、`Renderer::out` に渡す文字列を組み立てる。色の有無は `Renderer` の `color` フラグ（`--color` / `--no-color` / tty 判定）で決め、検索結果・アウトラインの識別子に ANSI 色を適用する。JSON 出力では常に装飾を無効にする。

文言は英語で固定する（M10）。`[untrusted] <url>` マーカー、`[truncated: ...]`、件数・クォータの行など既存の書式は変えず、語だけを英語にする。`[使用]` / `[候補]` の接頭辞は `[used]` / `[candidate]` にする。外部から得た文字列（タイトル・本文・`description`）は翻訳しない。日本語リテラルの混入は `REF:tests/architecture.rs` の検査で防ぐ。

### 4.3 JSON 出力の規則

状態: 実装完了（REF:src/ui/render.rs）

`Renderer::envelope(event, fields)` が共通枠を作り、`Renderer::event` が `serde_json::to_string` で 1 行を標準出力へ書く。本文は必ず文字列フィールドに入れる。`at` は注入された `Clock` から取得する。コマンドスコープの警告は `Renderer` が収集し、対象イベントの `warnings[]` または直前の `warnings` イベントへ出す。`--quiet` でも JSON の警告を失わない。

---

## 5. キーボード・入力規則

状態: 実装完了（REF:src/ui/cli.rs, REF:src/ui/interactive.rs）

### 5.1 入力プロンプト

状態: 実装完了（REF:src/core/select.rs）

入力の解釈は `src/ui/menu.rs` の `parse_input` に集約する。`src/core/select.rs` は選択範囲（`1,3` / `1-3` / `R1` など）の構文解析だけを行い、区切りはカンマ・読点・空白を受け付ける。行編集は自前実装せず、端末に任せる（`rustyline` 等の追加依存を持たない）。不正入力の回数は `REF:src/ui/interactive.rs` が数え、3 回でメニューへ戻す。

### 5.2 表示幅

状態: 実装完了（REF:src/ui/render.rs）

`src/ui/render.rs` の `truncate(text, width) -> String` が `unicode_width::UnicodeWidthChar` で表示幅を数え、収まらない場合は末尾を `…` に置き換える（`width - 1` 桁まで）。予算計算は `char` 数（`core::text::chars`）で行い、表示幅とは別関数として混同しない。折り返しは行わない（端末幅は `COLUMNS` から取るが、出力の組み立てには使わない）。
