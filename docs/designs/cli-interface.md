# 設計書: コマンドラインインターフェース

本書は `REF:docs/specifications/cli-interface.md` に対応する実装設計を記述する。

---

## 1. 起動とモード

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

CLI は `clap` の derive API で定義する。`src/ui/cli/args.rs` に `Cli`（グローバルオプション）と `Command`（サブコマンドの列挙）を置き、`main.rs` は `Cli::parse()` → `app::run(cli)` の 2 行のみとする。

### 1.1 起動形態

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`Command::None` の場合は `ui::cli::interactive::run` を、それ以外は `ui::cli::commands::dispatch` を呼ぶ。両者は同一の `usecases` 関数を呼ぶ。

### 1.2 モード判定

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`ports::Env::is_stdin_tty()` / `is_stdout_tty()` で判定する。`is_terminal` クレートではなく `std::io::IsTerminal` を用い、`adapters::env::process_env` に閉じ込める。`E1001` のメッセージにはサブコマンド一覧を含める（`clap` の `Command::render_help` から生成する）。

### 1.3 共通オプション

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`GlobalOptions` 構造体に集約し、`resolve` 層への入力とする。`--engines-dir` は `Vec<PathBuf>` で複数指定を受け付ける。`--no-filter` は `Option<Option<String>>`（値なしは `results+content`、`all` は既定除去要素も対象）として解釈する。

---

## 2. 対話モードのメニュー

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 2.1 検索結果メニュー

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`ScreenState`（`Results` / `Select` / `Outline` / `Read` / `Depth` / `History` / `Done`）の状態機械。番号と項目の対応は定数 `RESULT_MENU: [(u8, &str); 4]` に固定し、描画と解釈が同じ定数を参照する。遷移関数 `transition(state, input, ctx) -> Transition` を純粋関数として単体テストする。

### 2.2 結果選択の入力規則

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/core/parse/selection.rs` の `parse_selection(input, max_ref) -> Result<SelectionSpec, SelectionError>`。`1,3-5` と `R1,R3` の双方を受け付け、`SelectionSpec { refs: Vec<u32> }` へ正規化する（重複は除去し、順序は入力順）。不正入力の連続回数は画面状態が保持する。

### 2.3 アウトライン・メニュー

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`OutlineView` を描画するのみ。深度変更はメモリ上の `Outline` から再描画し、`usecases::document::fetch` を再度呼ばない。

### 2.4 節本文の表示形式

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/ui/render/section_text.rs` の `render_header`, `render_body`, `render_footer` に分離する。マーカーは定数 `UNTRUSTED_BEGIN` / `UNTRUSTED_END`。本文中の `--- end ---` は `sanitize_marker` で `— end —` へ置換する。

### 2.5 通常の情報行

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/ui/render/status_text.rs` の `search_line`, `fetch_line`, `quota_exceeded_line` が仕様の 3 種類を生成する。数値は `usecases` が返す `Usage` と `FilterStats` から取り、描画側で再計算しない。

### 2.6 中断と復帰

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`tokio::signal::ctrl_c()` を `tokio::select!` で入力待ちと競争させる。入力待ちは `tokio::task::spawn_blocking` で `std::io::stdin().read_line` を実行し、`select!` の中断で `AbortHandle` により打ち切る。`Ctrl+D` は `Ok(0)` の読み取りで検出する。

---

## 3. 非対話サブコマンド

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 3.1 コマンド一覧

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/ui/cli/commands/` に機能群ごとのファイルを置く（`search.rs`, `document.rs`, `history.rs`, `engines.rs`, `network.rs`, `filters.rs`, `quota.rs`, `config.rs`, `doctor.rs`）。各ファイルは `async fn run(args, ctx) -> Result<CmdOutcome>` を公開し、`dispatch.rs` が `match` で振り分ける（サブコマンドの分岐はここだけに存在する）。

### 3.2 対象の指定方法

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/usecases/target.rs` の `resolve_target(input, session) -> Result<Target>`。`Target` は `Url(Url)` / `ResultId(u32)` / `DocumentId(u32)`。数値はまず結果番号、次に文書番号として解釈し、どちらも無ければ `E1006`。

### 3.3 終了コード

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/ui/cli/exit_code.rs` の `exit_code_for(&AppError) -> i32`。`ErrorKind`（`Input` / `Network` / `Convert` / `Provider` / `State`）から 1〜5 を導出し、割り込みは `130`。`main.rs` は `std::process::exit(code)` を 1 回だけ呼ぶ。

---

## 4. 出力チャネルと終了コード

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 4.1 チャネルの分離

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`trait Reporter { fn data(&mut self, line: &str); fn info(&mut self, line: &str); fn warn(&mut self, d: &Diagnostic); fn error(&mut self, e: &AppError); }`。実装は `StreamReporter`（`Stdout`/`Stderr`）と `BufferReporter`（テスト用）。リポジトリ内で `println!` / `eprintln!` を直接使わないことを `tests/architecture.rs` で検査する。

### 4.2 テキスト出力の規則

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

描画関数は `src/ui/render/*.rs` に集約し、`Reporter::data` に渡す文字列を組み立てる。色は `ui/render/color.rs` の `Paint` を通す。

### 4.3 JSON 出力の規則

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/ui/render/json_events.rs` の `emit(reporter, event)`。`serde_json::to_string` は 1 行を返すため、本文は必ず文字列フィールドに入れる。`at` は `Clock` から取得する。

---

## 5. キーボード・入力規則

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 5.1 入力プロンプト

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/ui/cli/input.rs` の `read_line(prompt) -> InputResult`（`Value` / `Cancelled` / `Eof`）。行編集は自前実装せず、端末に任せる（`rustyline` 等の追加依存を持たない）。`!<n>` の解釈は `core/parse/reuse.rs`。

### 5.2 表示幅

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/ui/render/width.rs` の `display_width(&str) -> usize`（`unicode_width`）と `truncate_display(&str, width) -> String`。予算計算は `char` 数（`core::text::chars`）で行い、表示幅とは別関数として混同しない。折り返しは `wrap_ansi` 相当を自前で実装せず、`ui/render/wrap.rs` で空白区切りに行を分割する。
