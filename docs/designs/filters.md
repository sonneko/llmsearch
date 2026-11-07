# 設計書: フィルタリング（JSON）

本書は `REF:docs/specifications/filters.md` に対応する実装設計を記述する。

---

## 1. 基本方針

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 1.1 対象

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

結果フィルタは `src/core/filter/results.rs` の `apply_results(filters, hits) -> (Vec<SearchHit>, FilterStats)`、本文フィルタは `src/core/filter/content.rs` の `apply_content(filters, md) -> (String, FilterStats)` と `apply_outline(filters, outline) -> (Outline, FilterStats)`。いずれも純粋関数とする。

### 1.2 フィルタ集合

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`FilterSet` は `core::config::filter` に定義し、`adapters::config::loader` が `filters/<id>.json` から読む。`mode=append` の合成は `core::filter::merge(base, extra)` で行い、規則を連結したうえで元の `mode` を保持する。

### 1.3 評価の原則

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

正規表現は読み込み時に 1 回だけ `Regex::new` してコンパイル結果を `Rule` に保持する（評価のたびにコンパイルしない）。コンパイル失敗は `E1004`。評価結果は `Decision { keep: bool, reason: Option<RuleId> }` とし、`RuleId` は「種別 + 配列名 + 添字」で機械的に生成する。

---

## 2. フィルタ集合の構造

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 2.1 トップレベル

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`FilterSet { schema_version, id, description, mode, results: Option<ResultFilters>, content: Option<ContentFilters>, extra }`。`extra` は未知キーの温存用。

### 2.2 `mode`

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`merge` は `Mode::Append` の場合のみ規則を連結する。`Mode::Replace` の設定が利用者側で使われた場合は `E1004`（利用者設定では `append` のみ許可）とする。

---

## 3. 結果フィルタ（`results`）

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 3.1 構造

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`ResultFilters { deny: Vec<Rule>, allow: Vec<Rule>, require: Vec<Rule>, max_results: Option<usize>, max_per_host: Option<usize>, diversity: bool }`。

### 3.2 規則（`deny` / `allow` / `require`）

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`Rule` は全キーを `Option` にした構造体とし、`matches(&self, hit: &SearchHit, ctx: &RuleCtx) -> bool` で AND 評価する。`host` の `*.example.com` は `core::net::host::suffix_match` で判定。`language` は `SearchHit.language`（エンジン定義の `defaults.language` から設定）と比較する。`olderThanDays` は `chrono` で現在日付（`Clock` 由来）と比較し、`published` が無い場合は `false`。

### 3.3 評価順

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`apply_results` は仕様の 6 段階をそのまま実装する。`maxPerHost` は `BTreeMap<String, usize>` でホスト別件数を数え、超過分を除く。`diversity` は `core::filter::diversity::interleave(hits)` で、ホストごとのキューを作って 1 件ずつ取り出す（安定ソートを維持するため元順序を保持する）。

### 3.4 既定のフィルタ集合

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

同梱の `assets/filters/default.json` に仕様の内容を記述し、`include_str!` で読み込む。コードに規則を埋め込まない。

---

## 4. 本文フィルタ（`content`）

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 4.1 構造

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`ContentFilters` は仕様の全キーを持つ。`stripPatterns` と `stripLines.regex` は読み込み時にコンパイルする。

### 4.2 既定の除去要素

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`DEFAULT_STRIP_SELECTORS` を定数配列として `core/filter/content.rs` に置き、利用者指定と連結する。`adapters::extract` は「除去対象のセレクタ一覧」を引数で受け取り、既定値を自分で持たない。

### 4.3 `stripLines` の条件

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`repeatedAtLeast` は文書全体の行頻度を先に数える（`HashMap<&str, usize>`）。`boilerplate` は部分一致（大文字小文字を区別しない）。`equals` は完全一致（前後空白を除去した行で比較）。

### 4.4 `normalize`

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`core::markdown::normalize::apply(md, &NormalizeOptions) -> String` に 4 つの処理を個別の関数として実装し、`NormalizeOptions` の真偽値で呼び分ける。コードブロック内の行末空白はフェンス状態を追跡して保護する。

### 4.5 適用順

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`usecases::document::fetch` が 1) 抽出時に `stripSelectors`、2) 変換後に `apply_content`、を順に呼ぶ。アウトライン表示では `apply_outline` を呼び、本文そのものは書き換えない（`read` で指定された節は隠されていても読める）。

### 4.6 既定のフィルタ集合（本文）

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

同梱の `assets/filters/default.json` に記載する。`minSectionChars: 1` と `dropEmptySections: true` により、見出しだけの節はアウトラインから除かれる。

---

## 5. 集計と可視化

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 5.1 集計

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`FilterStats { kept, removed, reasons: BTreeMap<String, usize>, truncated_by: (usize, usize) }` を返す。`reasons` のキーは `results.deny[2]` のような安定した文字列とし、`BTreeMap` で順序を固定する。

### 5.2 表示

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/ui/render/filter_text.rs` の `summary_line(stats)` が 1 行の要約を作る。`--verbose` では `detail_lines(stats)` を追加出力する。JSON では `SearchEvent.filtered` に `FilterStats` をそのまま射影する。

### 5.3 無効化

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`--no-filter` は `FilterSet` の `results` と `content.strip*` を空にした複製を作る。`--no-filter=all` では `DEFAULT_STRIP_SELECTORS` も空にする。`disabled: true` を `FilterStats` に含める。
