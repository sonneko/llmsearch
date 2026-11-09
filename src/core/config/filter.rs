//! 結果フィルタ・本文フィルタ（JSON）。

use serde::{Deserialize, Serialize};

use crate::core::errors::{AppError, ErrorCode};
use crate::core::model::SearchHit;
use crate::core::url;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct FilterSet {
    pub schema_version: u32,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub results: Option<ResultFilters>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<ContentFilters>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// フィルタ集計。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct FilterStats {
    pub kept: usize,
    pub removed: usize,
    /// 規則ごとの除外件数（キーは `results.deny[0]` 形式）。
    pub reasons: std::collections::BTreeMap<String, usize>,
    pub truncated_by_max_results: usize,
    pub truncated_by_max_per_host: usize,
    pub disabled: bool,
}

impl FilterStats {
    pub fn record(&mut self, key: impl Into<String>) {
        *self.reasons.entry(key.into()).or_insert(0) += 1;
        self.removed += 1;
    }

    pub fn summary_line(&self) -> String {
        if self.disabled {
            return "（フィルタ: 無効）".to_string();
        }
        if self.removed == 0 {
            return format!("（フィルタ: {} 件すべて表示）", self.kept);
        }
        format!(
            "（フィルタ: {} 件中 {} 件を表示、除外 {} 件）",
            self.kept + self.removed,
            self.kept,
            self.removed
        )
    }
}

/// 結果フィルタ。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ResultFilters {
    pub deny: Vec<RuleSpec>,
    pub allow: Vec<RuleSpec>,
    pub require: Vec<RuleSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_results: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_per_host: Option<usize>,
    pub diversity: bool,
}

/// フィルタ規則（複数キーは AND）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct RuleSpec {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<OneOrMany>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_regex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url_regex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_regex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet_regex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_contains: Option<OneOrMany>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheme: Option<OneOrMany>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub older_than_days: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub newer_than_days: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension: Option<OneOrMany>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<OneOrMany>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl OneOrMany {
    pub fn iter(&self) -> impl Iterator<Item = &String> {
        match self {
            OneOrMany::One(s) => std::slice::from_ref(s).iter(),
            OneOrMany::Many(v) => v.iter(),
        }
    }
}

/// コンパイル済みの規則。
#[derive(Debug, Clone)]
pub struct CompiledRule {
    pub id: String,
    pub spec: RuleSpec,
    host_re: Option<regex::Regex>,
    url_re: Option<regex::Regex>,
    title_re: Option<regex::Regex>,
    snippet_re: Option<regex::Regex>,
}

impl CompiledRule {
    pub fn compile(id: String, spec: RuleSpec) -> Result<Self, AppError> {
        let build = |pat: &Option<String>| -> Result<Option<regex::Regex>, AppError> {
            match pat {
                None => Ok(None),
                Some(p) => regex::Regex::new(p).map(Some).map_err(|e| {
                    AppError::with_detail(ErrorCode::E1004, format!("正規表現 {p}: {e}"))
                }),
            }
        };
        Ok(Self {
            id,
            host_re: build(&spec.host_regex)?,
            url_re: build(&spec.url_regex)?,
            title_re: build(&spec.title_regex)?,
            snippet_re: build(&spec.snippet_regex)?,
            spec,
        })
    }
}

fn any_match(values: &OneOrMany, f: impl Fn(&str) -> bool) -> bool {
    values.iter().any(|v| f(v))
}

impl CompiledRule {
    pub fn matches(&self, hit: &SearchHit, now: chrono::NaiveDate) -> bool {
        let spec = &self.spec;
        let url = match url::parse(&hit.url) {
            Ok(u) => u,
            Err(_) => return spec.host.is_some() || spec.url_regex.is_some(),
        };
        let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
        if let Some(patterns) = &spec.host
            && !any_match(patterns, |p| url::suffix_match(&host, p))
        {
            return false;
        }
        if let Some(re) = &self.host_re
            && !re.is_match(&host)
        {
            return false;
        }
        if let Some(re) = &self.url_re
            && !re.is_match(hit.url.as_str())
        {
            return false;
        }
        if let Some(re) = &self.title_re
            && !re.is_match(&hit.title)
        {
            return false;
        }
        if let Some(re) = &self.snippet_re {
            let snippet = hit.snippet.clone().unwrap_or_default();
            if !re.is_match(&snippet) {
                return false;
            }
        }
        if let Some(prefix) = &spec.path_prefix
            && !url.path().starts_with(prefix.as_str())
        {
            return false;
        }
        if let Some(needles) = &spec.title_contains {
            let title = hit.title.to_lowercase();
            if !any_match(needles, |n| title.contains(&n.to_lowercase())) {
                return false;
            }
        }
        if let Some(schemes) = &spec.scheme
            && !any_match(schemes, |s| s.eq_ignore_ascii_case(url.scheme()))
        {
            return false;
        }
        if let Some(exts) = &spec.extension {
            let ext = url::extension(&url).unwrap_or_default();
            if !any_match(exts, |e| e.eq_ignore_ascii_case(&ext)) {
                return false;
            }
        }
        if let Some(lang) = &spec.language {
            let actual = hit.language.clone().unwrap_or_default();
            if !any_match(lang, |l| l.eq_ignore_ascii_case(&actual)) {
                return false;
            }
        }
        if spec.older_than_days.is_some() || spec.newer_than_days.is_some() {
            let Some(published) = hit.published.as_deref().and_then(parse_date) else {
                return false;
            };
            let age = (now - published).num_days();
            if let Some(days) = spec.older_than_days
                && age <= days
            {
                return false;
            }
            if let Some(days) = spec.newer_than_days
                && age >= days
            {
                return false;
            }
        }
        true
    }
}

fn parse_date(s: &str) -> Option<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(s.get(..10).unwrap_or(s), "%Y-%m-%d").ok()
}

/// コンパイル済みフィルタ集合。
#[derive(Debug, Clone)]
pub struct CompiledFilters {
    pub set: FilterSet,
    pub deny: Vec<CompiledRule>,
    pub allow: Vec<CompiledRule>,
    pub require: Vec<CompiledRule>,
}

impl CompiledFilters {
    pub fn compile(set: FilterSet) -> Result<Self, AppError> {
        let compile_list =
            |prefix: &str, rules: &[RuleSpec]| -> Result<Vec<CompiledRule>, AppError> {
                rules
                    .iter()
                    .enumerate()
                    .map(|(i, r)| CompiledRule::compile(format!("{prefix}[{i}]"), r.clone()))
                    .collect()
            };
        let (deny, allow, require) = match &set.results {
            Some(r) => (
                compile_list("results.deny", &r.deny)?,
                compile_list("results.allow", &r.allow)?,
                compile_list("results.require", &r.require)?,
            ),
            None => (Vec::new(), Vec::new(), Vec::new()),
        };
        Ok(Self {
            set,
            deny,
            allow,
            require,
        })
    }

    pub fn result_filters(&self) -> Option<&ResultFilters> {
        self.set.results.as_ref()
    }

    pub fn content_filters(&self) -> Option<&ContentFilters> {
        self.set.content.as_ref()
    }
}

/// 結果フィルタを適用する。
pub fn apply_results(
    filters: &CompiledFilters,
    hits: Vec<SearchHit>,
    now: chrono::NaiveDate,
    disabled: bool,
) -> (Vec<SearchHit>, FilterStats) {
    let mut stats = FilterStats {
        disabled,
        ..Default::default()
    };
    if disabled {
        stats.kept = hits.len();
        return (hits, stats);
    }
    let Some(rf) = filters.result_filters() else {
        stats.kept = hits.len();
        return (hits, stats);
    };

    let mut kept: Vec<SearchHit> = Vec::new();
    for hit in hits {
        if let Some(rule) = filters.deny.iter().find(|r| r.matches(&hit, now))
            && !filters.allow.iter().any(|r| r.matches(&hit, now))
        {
            stats.record(rule.id.clone());
            continue;
        }
        if let Some(rule) = filters.require.iter().find(|r| !r.matches(&hit, now)) {
            stats.record(rule.id.clone());
            continue;
        }
        kept.push(hit);
    }

    // maxPerHost
    if let Some(max) = rf.max_per_host {
        let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
        let mut out = Vec::new();
        for hit in kept {
            let host = url::parse(&hit.url)
                .ok()
                .and_then(|u| u.host_str().map(|s| s.to_string()))
                .unwrap_or_default();
            let c = counts.entry(host).or_insert(0);
            if *c >= max {
                stats.truncated_by_max_per_host += 1;
                continue;
            }
            *c += 1;
            out.push(hit);
        }
        kept = out;
    }

    if rf.diversity {
        kept = interleave_by_host(kept);
    }

    if let Some(max) = rf.max_results
        && kept.len() > max
    {
        stats.truncated_by_max_results = kept.len() - max;
        kept.truncate(max);
    }

    // 識別子を振り直す（欠番を作らない）
    for (i, hit) in kept.iter_mut().enumerate() {
        hit.id = (i + 1) as u32;
        hit.rank = (i + 1) as u32;
    }
    stats.kept = kept.len();
    (kept, stats)
}

/// ホストを分散させて並べ替える（元順序は保持）。
pub fn interleave_by_host(hits: Vec<SearchHit>) -> Vec<SearchHit> {
    use std::collections::VecDeque;
    let mut order: Vec<String> = Vec::new();
    let mut queues: std::collections::HashMap<String, VecDeque<SearchHit>> = Default::default();
    for hit in hits {
        let host = url::parse(&hit.url)
            .ok()
            .and_then(|u| u.host_str().map(|s| s.to_string()))
            .unwrap_or_default();
        if !queues.contains_key(&host) {
            order.push(host.clone());
        }
        queues.entry(host).or_default().push_back(hit);
    }
    let mut out = Vec::new();
    loop {
        let mut progressed = false;
        for host in &order {
            if let Some(q) = queues.get_mut(host)
                && let Some(hit) = q.pop_front()
            {
                out.push(hit);
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }
    out
}

/// 本文フィルタ。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ContentFilters {
    pub strip_selectors: Vec<String>,
    pub strip_patterns: Vec<String>,
    pub strip_lines: Vec<LineRule>,
    pub keep_only: Vec<LineRule>,
    pub normalize: NormalizeOptions,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_section_chars: Option<usize>,
    pub drop_empty_sections: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_heading_depth: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct LineRule {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub equals: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shorter_than: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeated_at_least: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boilerplate: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct NormalizeOptions {
    pub collapse_blank_lines: bool,
    pub trim_line_ends: bool,
    pub ensure_trailing_newline: bool,
    pub decode_entities: bool,
}

impl Default for NormalizeOptions {
    fn default() -> Self {
        Self {
            collapse_blank_lines: true,
            trim_line_ends: true,
            ensure_trailing_newline: true,
            decode_entities: true,
        }
    }
}

impl LineRule {
    pub fn matches(
        &self,
        line: &str,
        freq: Option<&std::collections::HashMap<String, usize>>,
    ) -> bool {
        if let Some(pat) = &self.regex
            && let Ok(re) = regex::Regex::new(pat)
            && !re.is_match(line)
        {
            return false;
        }
        if let Some(eq) = &self.equals
            && line.trim() != eq.trim()
        {
            return false;
        }
        if let Some(min) = self.shorter_than
            && crate::core::text::chars(line.trim()) >= min
        {
            return false;
        }
        if let Some(at_least) = self.repeated_at_least {
            let key = line.trim().to_string();
            if key.is_empty() {
                return false;
            }
            let count = freq.and_then(|f| f.get(&key)).copied().unwrap_or(0);
            if count < at_least {
                return false;
            }
        }
        if let Some(words) = &self.boilerplate {
            let lower = line.to_lowercase();
            if !words.iter().any(|w| lower.contains(&w.to_lowercase())) {
                return false;
            }
        }
        true
    }
}

/// 本文フィルタを適用する（`stripPatterns` → `stripLines` → `keepOnly` → `normalize`）。
pub fn apply_content(filters: &CompiledFilters, md: &str, disabled: bool) -> (String, FilterStats) {
    let mut stats = FilterStats {
        disabled,
        kept: 1,
        ..Default::default()
    };
    if disabled {
        return (md.to_string(), stats);
    }
    let Some(cf) = filters.content_filters() else {
        return (md.to_string(), stats);
    };

    let mut text = md.to_string();
    for (i, pat) in cf.strip_patterns.iter().enumerate() {
        if let Ok(re) = regex::Regex::new(pat) {
            let before = text.len();
            text = re.replace_all(&text, "").to_string();
            if text.len() != before {
                *stats
                    .reasons
                    .entry(format!("content.stripPatterns[{i}]"))
                    .or_insert(0) += 1;
            }
        }
    }

    // 行頻度（repeatedAtLeast 用）
    let mut freq: std::collections::HashMap<String, usize> = Default::default();
    for line in text.lines() {
        let key = line.trim().to_string();
        if !key.is_empty() {
            *freq.entry(key).or_insert(0) += 1;
        }
    }

    if !cf.strip_lines.is_empty() {
        let lines: Vec<&str> = text.lines().collect();
        let flags = crate::core::outline::fence_flags(&lines);
        let mut out = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if flags[i] {
                out.push(*line);
                continue;
            }
            if let Some((idx, _)) = cf
                .strip_lines
                .iter()
                .enumerate()
                .find(|(_, r)| r.matches(line, Some(&freq)))
            {
                *stats
                    .reasons
                    .entry(format!("content.stripLines[{idx}]"))
                    .or_insert(0) += 1;
                stats.removed += 1;
                continue;
            }
            out.push(*line);
        }
        text = out.join("\n");
    }

    if !cf.keep_only.is_empty() {
        let lines: Vec<&str> = text.lines().collect();
        let flags = crate::core::outline::fence_flags(&lines);
        let mut out = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if flags[i] || line.trim().is_empty() {
                out.push(*line);
                continue;
            }
            if cf.keep_only.iter().any(|r| r.matches(line, Some(&freq))) {
                out.push(*line);
            } else {
                stats.removed += 1;
            }
        }
        text = out.join("\n");
    }

    text = crate::core::markdown::normalize(&text, &cf.normalize);
    (text, stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(id: u32, title: &str, url_str: &str) -> SearchHit {
        SearchHit {
            id,
            rank: id,
            title: title.into(),
            url: url_str.into(),
            site: None,
            snippet: None,
            published: None,
            engine: "x".into(),
            language: None,
        }
    }

    fn compiled(json: serde_json::Value) -> CompiledFilters {
        let set: FilterSet = serde_json::from_value(json).unwrap();
        CompiledFilters::compile(set).unwrap()
    }

    #[test]
    fn denies_by_extension_and_reddens_ids() {
        let f = compiled(serde_json::json!({
            "schemaVersion": 2, "id": "t",
            "results": {"deny": [{"extension": ["pdf"]}], "maxPerHost": 3}
        }));
        let hits = vec![
            hit(1, "a", "https://example.com/a"),
            hit(2, "b", "https://example.com/b.pdf"),
            hit(3, "c", "https://example.com/c"),
        ];
        let (kept, stats) = apply_results(
            &f,
            hits,
            chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            false,
        );
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[1].id, 2);
        assert_eq!(stats.removed, 1);
        assert_eq!(stats.reasons.get("results.deny[0]"), Some(&1));
    }

    #[test]
    fn max_per_host_truncates() {
        let f = compiled(serde_json::json!({
            "schemaVersion": 2, "id": "t",
            "results": {"maxPerHost": 1}
        }));
        let hits = vec![
            hit(1, "a", "https://example.com/a"),
            hit(2, "b", "https://example.com/b"),
            hit(3, "c", "https://other.example.org/c"),
        ];
        let (kept, stats) = apply_results(
            &f,
            hits,
            chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            false,
        );
        assert_eq!(kept.len(), 2);
        assert_eq!(stats.truncated_by_max_per_host, 1);
    }

    #[test]
    fn strips_repeated_lines() {
        let f = compiled(serde_json::json!({
            "schemaVersion": 2, "id": "t",
            "content": {"stripLines": [{"repeatedAtLeast": 2}],
                        "normalize": {"collapseBlankLines": true}}
        }));
        let md = "keep\nNAV\nbody\nNAV\n";
        let (out, _) = apply_content(&f, md, false);
        assert!(!out.contains("NAV"));
        assert!(out.contains("keep"));
    }

    #[test]
    fn boilerplate_matching_is_case_insensitive() {
        let f = compiled(serde_json::json!({
            "schemaVersion": 2, "id": "t",
            "content": {"stripLines": [{"boilerplate": ["cookie"]}]}
        }));
        let (out, _) = apply_content(&f, "本文\nCookie の利用について\n", false);
        assert!(!out.to_lowercase().contains("cookie"));
    }
}
