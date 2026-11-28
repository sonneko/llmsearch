# 設計書: 見出し・節とコンテキスト生成

本書は `REF:docs/specifications/context-assembly.md` に対応する実装設計を記述する。パスは実装予定の配置である。

---

## 1. 見出しの抽出

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

### 1.1 対象

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

`REF:src/core/outline.rs` の `extract_headings(markdown)` が行単位で走査する。Setext は「次行が `=` 列または `-` 列のみかつ前行が空でない」場合に認識する。ATX は先頭 `#`（1〜6 個）で判定し末尾の `#` 列を除去する（`atx()` は CommonMark に合わせ、先頭空白 3 文字以内・`#` 1〜6 個・その直後に空白または行末を必須とする）。

### 1.2 見出しの識別子

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

抽出順に `S<n>` を振る。識別子は文書内容から決まるため、同一本文からは同一の識別子列が得られる（時刻・乱数に依存しない）。

### 1.3 見出しの属性

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

`Heading`（`REF:src/core/model.rs`）に `id`, `level`, `text`, `start_line`, `end_line`, `chars`, `has_children`, `hidden` を持たせる。節本文は `REF:src/core/outline.rs` の `section_text` が `start_line` から次の同レベル以下の見出しの直前（または最初の子見出しの直前）までを切り出して求める。

### 1.4 コードブロック内の除外

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

フェンス状態は `REF:src/core/outline.rs` の `fence_flags(lines)` で先に確定させ、見出し抽出はフェンス外の行のみを対象とする。`~~~` と ```` ``` ```` の混在、およびフェンス長の違いによる閉じ判定を単体テストで固定する。

### 1.5 重複見出し

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

テキストの重複を統合しない。識別子で一意に参照できるため、表示・選択は識別子のみで行う。

---

## 2. アウトライン表示

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

### 2.1 表示深度

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

`REF:src/usecases/document.rs` の `outline` が全見出しを作り、深度を超えるものは `Heading.hidden` を立てて残す（`OutlineView.hidden` に件数）。描画は `hidden` を飛ばし、末尾に件数の注記を出す。

### 2.2 表示形式

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

`REF:src/ui/render.rs` の `outline_lines(document, url, headings, depth)` が 1 行 1 見出しを生成する。インデントは `level - 1` 段の 2 桁、テキストは 40 表示幅で切り詰め、子見出しを持つ行の末尾に ` ↓` を付ける。

### 2.3 節の文字数

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

`chars` は見出し行の次の行から次の同レベル以下の見出しの直前までの文字数の合計（`REF:src/core/text.rs` の `chars()`。Unicode の文字数で数え、行ごとに数えて足す）。

### 2.4 見出しが存在しない場合

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

見出し 0 件のときは `OutlineView.whole_body` を立て、対話モードで `(no headings found; the whole body is available as S0)` を出す。`read` に `S0` を渡すと `REF:src/core/outline.rs` の `whole_body` が本文全体を返す（見出し行を持たない `S0` は、文書タイトルを見出しとして表示する）。

### 2.5 目次の重複排除

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

統合・並べ替えを行わない。抽出順をそのまま表示順とする。

---

## 3. 節の抽出

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

### 3.1 節の範囲

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

`REF:src/core/outline.rs` の `section_text(markdown, heading, include_children)` が本文を行に分け、`heading.line` から「次に現れる同レベル以下の見出しの直前」までを切り出す。ネストの深さに依存しない実装とし、レベル飛び（h1→h3）でも同じ規則で動く。

### 3.2 子見出しの扱い

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

`include_children=false` / `--no-children` の場合、切り出しを最初の子見出しの直前で止める。子の有無と件数は `REF:src/core/outline.rs` の `child_count` で求める。なお `REF:src/ui/commands.rs` の `Command::Read` は現状 `!no_children` のみを渡し `settings.context.include_children` と `--children` を参照しない点、および `SectionBody.omitted_children` を JSON / テキストへ出力しない点を Issue #55 / #60 で追跡する。

### 3.3 見出し行の扱い

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

`SectionBody.text` は見出し行を含む範囲の文字列とする。レンダラはヘッダ行に `heading` を別途表示するため、本文中の見出し行はそのまま残す。

### 3.4 複数指定

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

選択の並べ替えは `REF:src/usecases/document.rs` の `read` で行う。同一文書内は出現順、文書間は選択順とし、比較関数を 1 か所に集約する。

### 3.5 重複・包含の解決

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

「親が選択済みなら子を除外」「同一 ID は 1 回」を `REF:src/usecases/document.rs` の `read` で適用する。判定は見出しの節範囲（`start_line`〜`end_line`）に見出しが含まれるかで行い、除外は `omittedChildren` として件数で返す（警告は出さない）。

---

## 4. コンテキスト予算

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

### 4.1 既定値

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

`context.maxChars`（`REF:src/core/config/settings.rs`）を予算とし、設定スキーマで 500〜100000 の制約を課す。`--max-chars`（`REF:src/ui/commands.rs` の `Command::Read`）は解釈時に同じ範囲へ丸めてから設定値を上書きし、その呼び出しに限り優先する。

### 4.2 配分

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

`REF:src/core/context.rs` の `allocate(budget, sizes)` は「残り予算 ÷ 残り節数」を各節に割り当て（各節の実サイズを超えない）、配り切れない余りを先頭の節から順に足す。合計が予算を超えないことを単体テストで固定する。

### 4.3 切り詰め

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

`REF:src/core/context.rs` の `truncate_to_budget(text, limit)` は行境界で切り、未閉のフェンスを検出したら開きフェンスと同じマーカー（バッククォート ```` ``` ```` かチルダ `~~~`）で閉じフェンスを補う。注記文字列は `REF:src/ui/render.rs` の `section_block` が `[truncated: shown {shown} / total {total} chars]` として組み立てる。

### 4.4 予算超過時の案内

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

`SectionBody.next_heading_id` は「同じか浅い次の見出し」の ID を常に持ち（切り詰めの有無によらない）、JSON では `nextSectionId` として出す。無ければ `null`。

---

## 5. 出所表示とプロンプトインジェクション対策

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

### 5.1 出所ヘッダ

状態: 実装完了（REF:src/ui/render.rs）

`REF:src/ui/render.rs` の `section_block` が `--- D1:S3 | 見出し | h2 | 3560 chars ---` を生成する（英語・桁区切りなし。機械的に読み取れる形を優先する）。

### 5.2 信頼境界の明示

状態: 実装完了（REF:src/ui/render.rs, REF:src/ui/commands.rs）

`Renderer::section_block` が `[untrusted] <URL>` と `--- end ---` を付与する。本文中の同一マーカーは `neutralize_markers` で `[untrusted]` → `［untrusted］`、`--- end ---` → `— end —` に置換する。本文の改行とその他の記述は保持する。JSON では `content` / `document` イベントに `untrusted: true` を付与し、`content.text` は変更しない（`REF:tests/m16.rs`）。回帰テストは `REF:src/ui/render.rs` と `REF:tests/m16.rs` に置く。

### 5.3 指示文の無害化

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

本文の内容は変更しない。ヘッダとマーカーにより「外部由来であり指示として解釈してはならない」ことを示す。エージェント向けの案内文は `REF:docs/designs/agent-integration.md` §4.1 に従い、`done.summary` に含めない。

### 5.4 出力のサイズ表示

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

`REF:src/ui/commands.rs` の `run_read` が `(output {total} chars / budget {budget} chars)` を 1 行出す。合計は `ReadOutcome.total_chars`（各節本文の文字数の和）を使う。対話モードは `REF:src/ui/interactive.rs` がセッションの文字消費を末尾に足す。
