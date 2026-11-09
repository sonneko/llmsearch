//! 全体設定（`settings.json`）。

use serde::{Deserialize, Serialize};

use crate::core::config::{FilterSet, NetworkProfile, QuotaPolicy, RefOr};
use crate::core::errors::{AppError, Check, ErrorCode};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub schema_version: u32,
    pub search: SearchSettings,
    pub network: RefOr<NetworkProfile>,
    pub filters: RefOr<FilterSet>,
    pub quota: RefOr<QuotaPolicy>,
    pub context: ContextSettings,
    pub cache: CacheSettings,
    pub history: HistorySettings,
    pub session: SessionSettings,
    pub ui: UiSettings,
    pub state: StateSettings,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SearchSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    pub limit: usize,
    pub max_results: usize,
    pub fallback: Vec<String>,
    pub merge: bool,
    pub engines: Vec<String>,
    pub snippet_chars: usize,
    pub title_chars: usize,
}

impl Default for SearchSettings {
    fn default() -> Self {
        Self {
            engine: None,
            limit: 7,
            max_results: 30,
            fallback: Vec::new(),
            merge: false,
            engines: Vec::new(),
            snippet_chars: 120,
            title_chars: 100,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ContextSettings {
    pub max_chars: usize,
    pub outline_depth: u8,
    pub include_children: bool,
    pub min_section_chars: usize,
}

impl Default for ContextSettings {
    fn default() -> Self {
        Self {
            max_chars: 8000,
            outline_depth: 3,
            include_children: true,
            min_section_chars: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct CacheSettings {
    pub enabled: bool,
    pub ttl_hours: u64,
    pub max_size_mb: u64,
    pub search_results: bool,
}

impl Default for CacheSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            ttl_hours: 24,
            max_size_mb: 512,
            search_results: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct HistorySettings {
    pub enabled: bool,
    pub retention_days: u64,
    pub max_entries: usize,
}

impl Default for HistorySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            retention_days: 90,
            max_entries: 50_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SessionSettings {
    pub ttl_days: u64,
    pub max_sessions: usize,
}

impl Default for SessionSettings {
    fn default() -> Self {
        Self {
            ttl_days: 7,
            max_sessions: 200,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct UiSettings {
    pub color: String,
    pub locale: String,
    pub page_size: usize,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            color: "auto".into(),
            locale: "ja".into(),
            page_size: 7,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct StateSettings {
    pub dir: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: 2,
            search: SearchSettings::default(),
            network: RefOr::Id("default".into()),
            filters: RefOr::Id("default".into()),
            quota: RefOr::Id("default".into()),
            context: ContextSettings::default(),
            cache: CacheSettings::default(),
            history: HistorySettings::default(),
            session: SessionSettings::default(),
            ui: UiSettings::default(),
            state: StateSettings::default(),
            extra: Default::default(),
        }
    }
}

fn range(name: &str, value: u64, min: u64, max: u64, out: &mut Vec<Check>) {
    if value < min || value > max {
        out.push(Check::Fail(format!(
            "{name} = {value}（許容範囲 {min}〜{max}）"
        )));
    }
}

impl Settings {
    pub fn validate(&self) -> Vec<Check> {
        let mut out = Vec::new();
        if self.schema_version != 2 {
            out.push(Check::Fail(format!(
                "settings.schemaVersion = {}（対応版は 2）",
                self.schema_version
            )));
        }
        range("search.limit", self.search.limit as u64, 1, 30, &mut out);
        range(
            "search.maxResults",
            self.search.max_results as u64,
            1,
            200,
            &mut out,
        );
        range(
            "context.maxChars",
            self.context.max_chars as u64,
            500,
            100_000,
            &mut out,
        );
        range(
            "context.outlineDepth",
            self.context.outline_depth as u64,
            2,
            4,
            &mut out,
        );
        range("ui.pageSize", self.ui.page_size as u64, 1, 30, &mut out);
        range("cache.ttlHours", self.cache.ttl_hours, 0, 8760, &mut out);
        range(
            "cache.maxSizeMb",
            self.cache.max_size_mb,
            1,
            102_400,
            &mut out,
        );
        range(
            "history.retentionDays",
            self.history.retention_days,
            0,
            36500,
            &mut out,
        );
        range("session.ttlDays", self.session.ttl_days, 1, 3650, &mut out);
        if !matches!(self.ui.color.as_str(), "auto" | "always" | "never") {
            out.push(Check::Fail(format!(
                "ui.color = {}（auto / always / never）",
                self.ui.color
            )));
        }
        if crate::core::errors::Locale::parse(&self.ui.locale).is_none() {
            out.push(Check::Fail(format!(
                "ui.locale = {}（ja / en）",
                self.ui.locale
            )));
        }
        if self.search.merge && self.search.engines.is_empty() {
            out.push(Check::Fail(
                "search.merge = true のとき search.engines に 1 件以上必要".into(),
            ));
        }
        for key in self.extra.keys() {
            out.push(Check::Warn(format!(
                "settings に未知キー {key}（保持される）"
            )));
        }
        out
    }

    pub fn has_fail(&self) -> bool {
        self.validate().iter().any(|c| c.is_fail())
    }

    /// 検証に失敗した最初の 1 件をエラーへ変換する。
    pub fn ensure_valid(&self) -> Result<(), AppError> {
        let checks = self.validate();
        match checks.iter().find(|c| c.is_fail()) {
            None => Ok(()),
            Some(fail) => Err(AppError::with_detail(
                ErrorCode::E1004,
                fail.text().to_string(),
            )),
        }
    }

    pub fn locale(&self) -> crate::core::errors::Locale {
        crate::core::errors::Locale::parse(&self.ui.locale).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        assert!(!Settings::default().has_fail());
    }

    #[test]
    fn detects_out_of_range() {
        let mut s = Settings::default();
        s.context.max_chars = 10;
        assert!(s.has_fail());
    }

    #[test]
    fn unknown_keys_are_kept() {
        let s: Settings = serde_json::from_str(r#"{"future": {"a": 1}}"#).unwrap();
        assert!(s.extra.contains_key("future"));
        let back = serde_json::to_value(&s).unwrap();
        assert_eq!(back["future"]["a"], 1);
    }
}
