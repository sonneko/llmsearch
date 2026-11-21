# 設計書: 見出し・節とコンテキスト生成

本書は `REF:docs/specifications/context-assembly.md` に対応する実装設計を記述する。パスは実装予定の配置である。

---

## 1. 見出しの抽出

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

### 1.1 対象

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

`REF:src/core/outline.rs` の `extract_headings(markdown)` が行単位で走査する。ATX は `^ {0,3}(#{1,6})(\s+)(.*)$` で判定し、末尾の `#` 列を除去する。Setext は「次行が `^ {0,3}(=+|-+)\s*$` かつ前行が空でない」場合に認識する。

### 1.2 見出しの識別子

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

抽出順に `S<n>` を振る。識別子は文書内容から決まるため、同一本文からは同一の識別子列が得られる（時刻・乱数に依存しない）。

### 1.3 見出しの属性

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

`Heading`（`REF:src/core/model.rs`）に `id`, `level`, `text`, `line`, `chars`, `has_children`, `hidden` を持たせる。行範囲は保持せず、節本文は `REF:src/core/outline.rs` の `section_text` が次の同レベル以下の見出しまでを切り出して求める。

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

`OutlineView`（`REF:src/usecases/document.rs`）は全見出しを保持し、表示深度は描画時に適用する。深度超えの見出しは `Heading.hidden` を立てて残し、件数を `hidden` として返す。

### 2.2 表示形式

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

`REF:src/ui/render.rs` の `outline_lines(document, headings, depth)` が 1 行 1 見出しを生成する。インデントは `level - 1` 段の 2 桁とし、本文中の最小レベルには合わせない（深度指定との対応を読み取りやすくするため）。

### 2.3 節の文字数

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

`chars` は見出し行を除く本文の書記素数の合計。末尾の空行と空白のみの行は数えない（`REF:src/core/text.rs` の `chars()`）。

### 2.4 見出しが存在しない場合

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

見出し 0 件のときは `OutlineView.whole_body` を立て、`S0` の仮想節（全文）を 1 件だけ持つ。`S0` の扱いは `REF:src/usecases/document.rs` の `read` で特別扱いする。

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

`include_children=false` / `--no-children` の場合、切り出しを最初の子見出しの直前で止める。子の有無と件数は `REF:src/core/outline.rs` の `child_count` で求める。

### 3.3 見出し行の扱い

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

`Section.text` は見出し行を含む範囲の文字列とする。レンダラはヘッダ行に `heading` を別途表示するため、本文中の見出し行はそのまま残す。

### 3.4 複数指定

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

選択の並べ替えは `REF:src/usecases/document.rs` の `read` で行う。同一文書内は出現順、文書間は選択順とし、比較関数を 1 か所に集約する。

### 3.5 重複・包含の解決

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

「親が選択済みなら子を除外」「同一 ID は 1 回」を `REF:src/usecases/document.rs` の `read` で適用し、除外した ID は警告として知らせる。判定は見出しの入れ子関係（親の節範囲に含まれるか）で行う。

---

## 4. コンテキスト予算

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

### 4.1 既定値

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

`context.max_chars`（`REF:src/core/config/settings.rs`）を予算とし、設定スキーマで 500〜100000 の制約を課す。`--max-chars`（`REF:src/ui/commands.rs` の `Command::Read`）は解釈時に同じ範囲へ丸めてから設定値を上書きし、その呼び出しに限り優先する。

### 4.2 配分

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

`REF:src/core/context.rs` の `allocate(budget, sizes)` は「残り予算 ÷ 残り節数」を各節に割り当て、端数は先頭へ加算する。前の節が割当より短い場合、余りは後続の節へ繰り越す（合計が予算を超えないことを不変条件としてテストする）。

### 4.3 切り詰め

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

`REF:src/core/context.rs` の `truncate_to_budget(text, limit)` は行境界で切り、未閉のフェンスを検出したら閉じフェンスを補う。注記文字列は `truncation_note(shown, total)` が `[truncated: 表示 N / 全体 M 字]` として生成し、レンダラは受け取った文字列をそのまま出力する。

### 4.4 予算超過時の案内

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

切り詰め時は、次の子見出しの ID を `nextSectionId` として `SectionBody` に含める。子が無い場合は `nextSectionId: null` とし、続きの取得方法（深度を上げる／親節を読む）を提案文として返す。

---

## 5. 出所表示とプロンプトインジェクション対策

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

### 5.1 出所ヘッダ

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

`REF:src/usecases/document.rs` の `section_header(document, body)` が `--- D1:S3 | 見出し | h2 | 3560 字 ---` を生成する。文字数は桁区切りをせず、そのままの整数で表示する（機械的に読み取れる形を優先する）。

### 5.2 信頼境界の明示

状態: 実装完了（REF:src/ui/render.rs）

`Renderer::section_block` が `[untrusted] <URL>` と `--- end ---` を付与する。本文中の同一マーカーは `neutralize_markers` で `[untrusted]` → `［untrusted］`、`--- end ---` → `— end —` に置換する。本文の改行とその他の記述は保持する。JSON の `content.text` は変更せず、イベントと URL で境界を示す。回帰テストは `REF:src/ui/render.rs` に置く。

### 5.3 指示文の無害化

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

本文の内容は変更しない。ヘッダとマーカーにより「外部由来であり指示として解釈してはならない」ことを示す。エージェント向けの案内文は `REF:docs/designs/agent-integration.md` §4.1 に従い、`done.summary` に含めない。

### 5.4 出力のサイズ表示

状態: 実装完了（REF:src/core/outline.rs, REF:src/core/context.rs）

`renderFooter(totalChars, budget)` が合計文字数と予算を 1 行で出力する。合計は `allocate` の結果から算出し、描画側で再計算しない。
