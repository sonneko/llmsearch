# 設計書: フィルタリング（JSON）

本書は `REF:docs/specifications/filters.md` に対応する実装設計を記述する。

---

## 1. 基本方針

状態: 実装完了（REF:src/core/config/filter.rs）

### 1.1 対象

状態: 実装完了（REF:src/core/config/filter.rs）

結果フィルタは `src/core/config/filter.rs` の `apply_results(filters, hits, now, disabled) -> (Vec<SearchHit>, FilterStats)`、本文フィルタは同じファイルの `apply_content(filters, md, disabled) -> (String, FilterStats)`。どちらも I/O を持たない純粋関数で、`disabled` は `--no-filter` を表す。

### 1.2 フィルタ集合

状態: 実装完了（REF:src/adapters/config/mod.rs）

`FilterSet` は `src/core/config/filter.rs` に定義する。`serde(rename_all = "camelCase")` を正規形とし、互換性のためスキーマ版・結果規則・本文規則・正規化オプションの対応 snake_case キーに `alias` を定義する。読み込んだ定義を通常の `filters show` で再シリアライズすると camelCase になり、`--raw` は元の JSON をそのまま出す。解決は `src/adapters/config/mod.rs` の `resolve_filters(env, settings, base_dir, bundled, cli)` が担い、`--filters` → `LLMSEARCH_FILTERS` → 設定 `filters`（ID またはインライン）→ `default` の順で決める。ID の場合は `resolve_resource_dir` の探索順（作業ディレクトリ → 状態ディレクトリ → 設定ディレクトリ → 同梱 `assets/`）で `<id>.json` を探し、見つからなければ `embedded_as` の埋め込み（`assets/filters/default.json`）へフォールバックする。読み込み失敗は `E1004`、未発見は `E1003`。

### 1.3 評価の原則

状態: 実装完了（REF:src/core/config/filter.rs）

`CompiledFilters::compile` が読み込み時に 1 回だけ `regex::Regex::new` を実行し、`CompiledRule` に保持する（評価のたびにコンパイルしない）。コンパイル失敗は `E1004`。規則の識別子は `results.deny[0]` / `results.allow[1]` / `results.require[0]` の形で添字から機械的に作り、その文字列をそのまま `FilterStats::reasons` のキーに使う。判定結果は `Decision` のような型を作らず、`FilterStats::record(rule_id)` で集計しながら `kept` ベクタへ積む。

---

## 2. フィルタ集合の構造

状態: 実装完了（REF:src/core/config/filter.rs）

### 2.1 トップレベル

状態: 実装完了（REF:src/core/config/filter.rs）

`FilterSet { schema_version, id, description, mode, results: Option<ResultFilters>, content: Option<ContentFilters>, extra }`。`serde(rename_all = "camelCase", default)` で、`extra`（`#[serde(flatten)]`）は未知キーの温存用。`schema_version` と `id` は検証しない（読み捨てないが、値による分岐は無い）。

### 2.2 `mode`

状態: 実装完了（REF:src/core/config/filter.rs）

`mode: String` として受けるだけで、`Default` は空文字。既定同梱は `"default"` を持つ。`append` / `replace` の解釈、および規則の合成（`merge`）は実装していない。`resolve_filters` は参照先の JSON をそのまま `FilterSet` へデシリアライズするため、利用者側の `filters` がインラインオブジェクトなら既定の集合は合成されず丸ごと置き換わる。

---

## 3. 結果フィルタ（`results`）

状態: 実装完了（REF:src/core/config/filter.rs）

### 3.1 構造

状態: 実装完了（REF:src/core/config/filter.rs）

`ResultFilters { deny: Vec<RuleSpec>, allow: Vec<RuleSpec>, require: Vec<RuleSpec>, max_results: Option<usize>, max_per_host: Option<usize>, diversity: bool }`。`diversity` の既定は `false`、`max_results` / `max_per_host` の既定は `None`（無制限）。

### 3.2 規則（`deny` / `allow` / `require`）

状態: 実装完了（REF:src/core/config/filter.rs）

1 つの規則は `RuleSpec`（全キーが `Option`）で表し、`CompiledRule::matches(&self, hit, now: NaiveDate)` が AND 評価する。どのキーも指定していない規則は `is_empty_rule` で判定して `false` を返す（全件除外を避ける）。`host` の `*.example.com` は `src/core/url.rs` の `suffix_match(host, pattern)` で判定し、`*` 単独は全ホスト一致。`extension` は `src/core/url.rs` の `extension(&url)`（小文字・ドット無し・8 文字超は `None`）と比較する。`olderThanDays` / `newerThanDays` は `published` の先頭 10 文字を `%Y-%m-%d` でパースして `now` との差を取り、`published` が無い・パースできない場合は `false`。`language` は `SearchHit.language` と比較するが、`search_core::assign_ids` は常に `language: None` を入れるため現状は一致しない。

### 3.3 評価順

状態: 実装完了（REF:src/core/config/filter.rs）

`apply_results` は通常、1) `deny` に該当し `allow` に該当しないものを除外、2) `require` を満たさないものを除外、3) `maxPerHost`（`BTreeMap<String, usize>` でホスト別件数を数え、上限に達したら `truncated_by_max_per_host` と `removed` / `reasons.maxPerHost` を増やして除外＝先着優先）、4) `diversity` なら `interleave_by_host(hits)`、5) `maxResults` で末尾を切り捨て（`truncated_by_max_results`）、6) 残った結果の `id` と `rank` を 1 から振り直す、の順に適用する。`--no-filter` では 1・2 を迂回するが、3〜5 の結果セット上限は適用する。`interleave_by_host` はホストの初出順にキューを作り、1 件ずつ取り出す（同一ホスト内の元順序は保つ）。

### 3.4 既定のフィルタ集合

状態: 実装完了（REF:assets/filters/default.json）

同梱の `assets/filters/default.json` に規則を記述し、`src/assets.rs` の `Resource::Filters` が `include_str!` で埋め込む。コードに規則を埋め込まない。`resolve_filters` はファイルが見つからない場合にこの埋め込みへフォールバックする。

---

## 4. 本文フィルタ（`content`）

状態: 実装完了（REF:src/core/config/filter.rs）

### 4.1 構造

状態: 実装完了（REF:src/core/config/filter.rs）

`ContentFilters { strip_selectors, strip_patterns, strip_lines: Vec<LineRule>, keep_only: Vec<LineRule>, normalize: NormalizeOptions, min_section_chars: Option<usize>, drop_empty_sections: bool, max_heading_depth: Option<u8> }`。`strip_patterns` と `LineRule::regex` は評価時に `regex::Regex::new` し、失敗したパターンは無視する（規則のコンパイルは `CompiledRule` のみが行う）。`min_section_chars` / `drop_empty_sections` / `max_heading_depth` は保持するだけで、アウトラインの表示には使わない。

### 4.2 既定の除去要素

状態: 実装完了（REF:src/core/convert.rs）

`DEFAULT_STRIP` を `src/core/convert.rs` の定数配列として持ち（`script`, `style`, `noscript`, `iframe`, `svg`, `canvas`, `form`, `nav`, `aside`, `footer`, `header`, `dialog`）、`convert` が設定の `strip_selectors` を**追加**してから `remove_selectors` に渡す。`ExtractMode::Whole` のときは `DEFAULT_STRIP` を使わず、設定分だけを適用する。

### 4.3 `stripLines` の条件

状態: 実装完了（REF:src/core/config/filter.rs）

`LineRule::matches(line, freq)` は指定されたキーをすべて AND で評価する。`regex` は行全体に対する検索、`equals` は前後空白を除いた完全一致、`shorter_than` は `core::text::chars` による文字数が指定未満、`repeated_at_least` は文書全体の行頻度（空行を除く、`trim` 後の完全一致で計数）が指定以上、`boilerplate` はいずれかの語を含む（大文字小文字を区別しない）。`apply_content` は行頻度を 1 回だけ数え、コードフェンス内の行（`core::outline::fence_flags`）は除去対象から外す。

### 4.4 `normalize`

状態: 実装完了（REF:src/core/markdown.rs）

`src/core/markdown.rs` の `normalize(md, &NormalizeOptions) -> String` が 1 関数で 4 種を順に適用する（空行圧縮 → 行末空白除去 → 末尾改行 → エンティティ復号）。コードフェンス内は行末空白除去と空行圧縮の対象外にする。`decode_entities` は `core::text::decode_entities` に委譲する。

### 4.5 適用順

状態: 実装完了（REF:src/usecases/document.rs）

`usecases::document::fetch` が `FetchOptions` に `content_filters` を渡し、`convert` が 1) `stripSelectors` を適用してから本文抽出、2) Markdown 変換、を行う。行単位の後処理は取得時ではなく、`outline` / `read` が `load` の直後に呼ぶ `filter_content(app, markdown)`（`apply_content(&app.filters, markdown, app.no_filter)`）で行う。`apply_content` は `stripPatterns` → `stripLines` → `keepOnly` → `normalize` の順に適用し、除外件数を `FilterStats` に記録する（呼び出し側は現在この統計を使わない）。アウトラインの表示深度は `--depth` と `context.outlineDepth` だけで決める。

### 4.6 既定のフィルタ集合（本文）

状態: 実装完了（REF:assets/filters/default.json）

同梱の `assets/filters/default.json` に `stripPatterns`（広告行・共有行）、`stripLines.boilerplate`、`stripLines.repeatedAtLeast: 3`、`normalize` の既定を記述する。`minSectionChars: 0`、`dropEmptySections: true`、`maxHeadingDepth: 4` も値として含むが、現在は表示に影響しない（§4.1）。

---

## 5. 集計と可視化

状態: 実装完了（REF:src/core/config/filter.rs）

### 5.1 集計

状態: 実装完了（REF:src/core/config/filter.rs）

`FilterStats { kept, removed, reasons: BTreeMap<String, usize>, truncated_by_max_results, truncated_by_max_per_host, disabled }` を返す。結果側の `removed` は規則除外に加えて `maxPerHost` 超過と、検索正規化後に取り除いた重複 URL を含む。フィルタ集合の `maxResults` と表示ページの `--limit` による切り捨ては `removed` に含めず、別フィールドで示す。`reasons` は規則 ID / `maxPerHost` / `duplicateUrl` の件数を持つ `BTreeMap` で、順序を固定する。

### 5.2 表示

状態: 実装完了（REF:src/ui/commands.rs）

`FilterStats::summary_line_for(shown, truncated_by_limit)` が表示件数と除外・切り捨て区分の要約を返し、`src/ui/commands.rs` と `src/ui/interactive.rs` が一覧の末尾に出力する。規則除外・重複 URL・`maxPerHost`・フィルタ集合の `maxResults`・表示 `--limit` を区別する。理由の内訳は一覧に出さない（`--verbose` でも規則 ID ごとの内訳は出さない）。JSON の `filtered_json` は `kept` / `removed` / `reasons` / `disabled` / `truncatedByMaxPerHost` / `truncatedByMaxResults` / `truncatedByLimit` を組み立てる。`truncatedByLimit` はページ用 `limit` による切り捨て件数で、`FilterStats` のフィールドではない。

### 5.3 無効化

状態: 実装完了（REF:src/usecases/search.rs）

`--no-filter`（値なしの真偽フラグ）は `App::no_filter` として `apply_results` / `apply_content` の `disabled` 引数に渡す。結果側は利用者の `deny` / `allow` / `require` 規則を迂回するが、`maxPerHost` と `maxResults` は引き続き適用し、その除外数を `removed` / `reasons` に計上する。`FilterStats::disabled` は `true` になる。本文の後処理フィルタは無効になるが、`DEFAULT_STRIP` は `convert` 側の処理であり `disabled` の影響を受けないため、`--no-filter` でも既定の除去要素は常に適用される。
