//! 閲覧履歴のレコード（追記専用 JSON Lines）。

use serde::{Deserialize, Serialize};

/// イベント種別。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EventType {
    Search,
    Next,
    Refine,
    Fetch,
    Outline,
    Read,
    Select,
    Engine,
    Quota,
    Config,
    Delete,
}

impl EventType {
    pub fn as_str(self) -> &'static str {
        match self {
            EventType::Search => "search",
            EventType::Next => "next",
            EventType::Refine => "refine",
            EventType::Fetch => "fetch",
            EventType::Outline => "outline",
            EventType::Read => "read",
            EventType::Select => "select",
            EventType::Engine => "engine",
            EventType::Quota => "quota",
            EventType::Config => "config",
            EventType::Delete => "delete",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "search" => EventType::Search,
            "next" => EventType::Next,
            "refine" => EventType::Refine,
            "fetch" => EventType::Fetch,
            "outline" => EventType::Outline,
            "read" => EventType::Read,
            "select" => EventType::Select,
            "engine" => EventType::Engine,
            "quota" => EventType::Quota,
            "config" => EventType::Config,
            "delete" => EventType::Delete,
            _ => return None,
        })
    }
}

/// 消費量。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Cost {
    pub searches: u64,
    pub fetches: u64,
    pub results: u64,
    pub chars: u64,
}

impl Cost {
    pub fn is_zero(&self) -> bool {
        self.searches == 0 && self.fetches == 0 && self.results == 0 && self.chars == 0
    }

    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if self.searches > 0 {
            parts.push(format!("searches +{}", self.searches));
        }
        if self.fetches > 0 {
            parts.push(format!("fetches +{}", self.fetches));
        }
        if self.results > 0 {
            parts.push(format!("results +{}", self.results));
        }
        if self.chars > 0 {
            parts.push(format!("chars +{}", self.chars));
        }
        parts.join(" ")
    }
}

/// 履歴 1 件。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub at: String,
    pub session: String,
    #[serde(rename = "type")]
    pub kind: EventType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<Cost>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining: Option<Cost>,
    #[serde(default)]
    pub payload: serde_json::Value,
}

impl HistoryEntry {
    pub fn new(
        id: impl Into<String>,
        at: chrono::DateTime<chrono::Utc>,
        session: impl Into<String>,
        kind: EventType,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            id: id.into(),
            at: at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            session: session.into(),
            kind,
            engine: None,
            cost: None,
            remaining: None,
            payload,
        }
    }

    /// 一覧表示用の要約。
    pub fn summary(&self) -> String {
        let p = &self.payload;
        let get = |k: &str| p.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
        match self.kind {
            EventType::Search | EventType::Refine => {
                let q = get("query");
                let n = p.get("results").and_then(|v| v.as_u64()).unwrap_or(0);
                let dropped = p.get("filtered").and_then(|v| v.as_u64()).unwrap_or(0);
                format!("{q}  {n} 件（除外 {dropped} 件）")
            }
            EventType::Next => {
                let n = p.get("added").and_then(|v| v.as_u64()).unwrap_or(0);
                format!("+{n} 件")
            }
            EventType::Fetch => {
                let url = get("url");
                let chars = p.get("chars").and_then(|v| v.as_u64()).unwrap_or(0);
                format!("{url}  ({chars} 字)")
            }
            EventType::Outline => {
                let n = p.get("headings").and_then(|v| v.as_u64()).unwrap_or(0);
                let depth = p.get("depth").and_then(|v| v.as_u64()).unwrap_or(0);
                format!("見出し {n} 件 (depth={depth})")
            }
            EventType::Read => {
                let sec = get("section");
                let head = get("heading");
                let chars = p.get("chars").and_then(|v| v.as_u64()).unwrap_or(0);
                format!("{sec}  {head}  ({chars} 字)")
            }
            EventType::Select => p
                .get("items")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .unwrap_or_default(),
            EventType::Engine => {
                let id = get("engine");
                let fb = p.get("fallback").and_then(|v| v.as_bool()).unwrap_or(false);
                if fb {
                    format!("{id}（フォールバック）")
                } else {
                    id
                }
            }
            EventType::Quota => get("note"),
            EventType::Config => get("path"),
            EventType::Delete => {
                let kind = get("target");
                let n = p.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
                format!("{kind}  {n} 件")
            }
        }
    }
}

/// 履歴の絞り込み（ポート定義と重複しないよう、純粋な判定のみ）。
pub struct Filter<'a> {
    pub query: Option<&'a str>,
    pub kind: Option<EventType>,
    pub session: Option<&'a str>,
    pub engine: Option<&'a str>,
    pub before: Option<&'a str>,
    pub after: Option<&'a str>,
}

impl Filter<'_> {
    pub fn matches(&self, entry: &HistoryEntry) -> bool {
        if let Some(kind) = self.kind
            && entry.kind != kind
        {
            return false;
        }
        if let Some(session) = self.session
            && entry.session != session
        {
            return false;
        }
        if let Some(engine) = self.engine
            && entry.engine.as_deref() != Some(engine)
        {
            return false;
        }
        if let Some(before) = self.before
            && entry.at.as_str() >= before
        {
            return false;
        }
        if let Some(after) = self.after
            && entry.at.as_str() <= after
        {
            return false;
        }
        if let Some(q) = self.query {
            let needle = q.to_lowercase();
            let haystack = format!(
                "{} {} {}",
                entry.summary(),
                entry.payload,
                entry.engine.clone().unwrap_or_default()
            )
            .to_lowercase();
            if !haystack.contains(&needle) {
                return false;
            }
        }
        true
    }
}

/// Markdown へのエクスポート（本文は含めない）。
pub fn export_markdown(entries: &[HistoryEntry]) -> String {
    let mut out = String::new();
    let mut current = String::new();
    for e in entries {
        if e.session != current {
            if !current.is_empty() {
                out.push('\n');
            }
            current = e.session.clone();
            out.push_str(&format!("## セッション {current}\n\n"));
        }
        out.push_str(&format!(
            "- `{}` **{}** {}\n",
            e.at,
            e.kind.as_str(),
            e.summary()
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: EventType, payload: serde_json::Value) -> HistoryEntry {
        HistoryEntry::new(
            "H1",
            chrono::DateTime::parse_from_rfc3339("2026-11-07T05:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            "s1",
            kind,
            payload,
        )
    }

    #[test]
    fn summarizes_search() {
        let e = entry(
            EventType::Search,
            serde_json::json!({"query": "気候", "results": 7, "filtered": 5}),
        );
        assert_eq!(e.summary(), "気候  7 件（除外 5 件）");
        assert_eq!(e.at, "2026-11-07T05:00:00Z");
    }

    #[test]
    fn filter_matches_query_and_kind() {
        let e = entry(
            EventType::Read,
            serde_json::json!({"section": "D1:S3", "heading": "適応"}),
        );
        let ok = Filter {
            query: Some("適応"),
            kind: Some(EventType::Read),
            session: None,
            engine: None,
            before: None,
            after: None,
        };
        assert!(ok.matches(&e));
        let ng = Filter {
            query: Some("適応"),
            kind: Some(EventType::Search),
            session: None,
            engine: None,
            before: None,
            after: None,
        };
        assert!(!ng.matches(&e));
    }

    #[test]
    fn export_groups_by_session() {
        let e = entry(EventType::Search, serde_json::json!({"query": "q"}));
        let md = export_markdown(&[e]);
        assert!(md.contains("## セッション s1"));
        assert!(md.contains("**search**"));
    }
}
