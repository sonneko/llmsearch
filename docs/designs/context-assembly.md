# 設計書: 見出し・節とコンテキスト生成

本書は `REF:docs/specifications/context-assembly.md` に対応する実装設計を記述する。パスは実装予定の配置である。

---

## 1. 見出しの抽出

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 1.1 対象

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/core/outline/extract.ts` の `extractHeadings(markdown)` が行単位で走査する。ATX は `^ {0,3}(#{1,6})(\s+)(.*)$` で判定し、末尾の `#` 列を除去する。Setext は「次行が `^ {0,3}(=+|-+)\s*$` かつ前行が空でない」場合に認識する。

### 1.2 見出しの識別子

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

抽出順に `S<n>` を振る。識別子は文書内容から決まるため、同一本文からは同一の識別子列が得られる（時刻・乱数に依存しない）。

### 1.3 見出しの属性

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`Heading`（`src/core/model/heading.ts`）に `id`, `level`, `text`, `startLine`, `endLine`, `chars`, `hasChildren` を持たせる。`endLine` は節分割（`src/core/outline/sections.ts`）で確定する。

### 1.4 コードブロック内の除外

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

フェンス状態は `src/core/outline/fence.ts` の `scanFences(lines)` で先に確定させ、見出し抽出はフェンス外の行のみを対象とする。`~~~` と ```` ``` ```` の混在、およびフェンス長の違いによる閉じ判定を単体テストで固定する。

### 1.5 重複見出し

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

テキストの重複を統合しない。識別子で一意に参照できるため、表示・選択は識別子のみで行う。

---

## 2. アウトライン表示

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 2.1 表示深度

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`Outline`（`src/core/outline/outline.ts`）は全見出しを保持し、表示深度は描画時に適用する（`filterByDepth(outline, depth)`）。深度超えの見出しは件数のみを `hiddenCount` として返す。

### 2.2 表示形式

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/ui/render/outline-text.ts` の `renderOutline(view)` が 1 行 1 見出しを生成する。インデントは `(level - minLevel) * 2` 桁とし、`minLevel` は文書内の最小レベルとする。

### 2.3 節の文字数

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`chars` は見出し行を除く本文の書記素数の合計。末尾の空行と空白のみの行は数えない（`src/core/text/count.ts` の `countChars()`）。

### 2.4 見出しが存在しない場合

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

見出し 0 件のときは `Outline.emptyWithWholeBody(document)` を返し、`S0` の仮想節（全文）を 1 件だけ持つ。`S0` の扱いは `src/core/outline/sections.ts` で特別扱いする。

### 2.5 目次の重複排除

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

統合・並べ替えを行わない。抽出順をそのまま表示順とする。

---

## 3. 節の抽出

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 3.1 節の範囲

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`buildSections(lines, headings)` はスタックを用い、各見出しの `endLine` を「次に現れる同レベル以下の見出しの直前行」として確定する。ネストの深さに依存しない実装とし、レベル飛び（h1→h3）でも同じ規則で動く。

### 3.2 子見出しの扱い

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`include_children=false` / `--no-children` の場合、`endLine` を最初の子見出しの直前行に切り詰める。切り詰めた場合は `omittedChildren` 件数を `SectionView` に持たせる。

### 3.3 見出し行の扱い

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`Section.text` は見出し行を含む範囲の文字列とする。レンダラはヘッダ行に `heading` を別途表示するため、本文中の見出し行はそのまま残す。

### 3.4 複数指定

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

選択の並べ替えは `src/core/context/order.ts` の `orderSelections()` で行う。同一文書内は出現順、文書間は選択順とし、比較関数を 1 か所に集約する。

### 3.5 重複・包含の解決

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`resolveOverlaps()` が「親が選択済みなら子を除外」「同一 ID は 1 回」を適用し、除外した ID を `omitted` として返す。判定は行範囲の包含関係で行う。

---

## 4. コンテキスト予算

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 4.1 既定値

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`ContextBudget`（`src/core/context/budget.ts`）は `maxChars` を持ち、設定スキーマで 500〜100000 の制約を課す。`--max-chars` は設定より優先する。

### 4.2 配分

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`allocate(budget, sections)` は「残り予算 ÷ 残り節数」を各節に割り当て、端数は先頭へ加算する。前の節が割当より短い場合、余りは後続の節へ繰り越す（合計が予算を超えないことを不変条件としてテストする）。

### 4.3 切り詰め

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`truncateToBudget(text, limit)` は行境界で切り、未閉のフェンスを検出したら閉じフェンスを補う。注記文字列は `[truncated: 表示 N / 全体 M 字]` を関数内で生成し、レンダラは受け取った文字列をそのまま出力する。

### 4.4 予算超過時の案内

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

切り詰め時は、次の子見出しの ID を `nextSectionId` として `SectionView` に含める。子が無い場合は `nextSectionId: null` とし、続きの取得方法（深度を上げる／親節を読む）を提案文として返す。

---

## 5. 出所表示とプロンプトインジェクション対策

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 5.1 出所ヘッダ

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/ui/render/section-text.ts` の `renderHeader(view)` が `--- D1:S3 | 見出し | h2 | 3,560 字 ---` を生成する。文字数は 3 桁区切りで表示する。

### 5.2 信頼境界の明示

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`UNTRUSTED_BEGIN = "[untrusted] "`、`UNTRUSTED_END = "--- end ---"` を定数化する。本文中に `--- end ---` が現れる場合は `sanitizeMarker()` で `— end —` へ置換し、本文が終端マーカーを偽装できないようにする。

### 5.3 指示文の無害化

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

本文の内容は変更しない。ヘッダとマーカーにより「外部由来であり指示として解釈してはならない」ことを示す。エージェント向けの案内文は `REF:docs/designs/agent-integration.md` §4.1 に従い、`done.summary` に含めない。

### 5.4 出力のサイズ表示

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`renderFooter(totalChars, budget)` が合計文字数と予算を 1 行で出力する。合計は `allocate` の結果から算出し、描画側で再計算しない。
