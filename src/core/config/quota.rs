//! クォータポリシー（JSON）とカウンタ。

use serde::{Deserialize, Serialize};

use crate::core::errors::{AppError, Check, ErrorCode};

/// 計測項目。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Metric {
    Searches,
    Fetches,
    Results,
    Chars,
}

impl Metric {
    pub fn as_str(self) -> &'static str {
        match self {
            Metric::Searches => "searches",
            Metric::Fetches => "fetches",
            Metric::Results => "results",
            Metric::Chars => "chars",
        }
    }

    pub fn all() -> [Metric; 4] {
        [
            Metric::Searches,
            Metric::Fetches,
            Metric::Results,
            Metric::Chars,
        ]
    }
}

/// 消費量。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Usage {
    pub searches: u64,
    pub fetches: u64,
    pub results: u64,
    pub chars: u64,
}

impl Usage {
    pub fn of(metric: Metric, amount: u64) -> Self {
        let mut u = Usage::default();
        u.add(metric, amount);
        u
    }

    pub fn add(&mut self, metric: Metric, amount: u64) {
        match metric {
            Metric::Searches => self.searches = self.searches.saturating_add(amount),
            Metric::Fetches => self.fetches = self.fetches.saturating_add(amount),
            Metric::Results => self.results = self.results.saturating_add(amount),
            Metric::Chars => self.chars = self.chars.saturating_add(amount),
        }
    }

    pub fn get(&self, metric: Metric) -> u64 {
        match metric {
            Metric::Searches => self.searches,
            Metric::Fetches => self.fetches,
            Metric::Results => self.results,
            Metric::Chars => self.chars,
        }
    }

    pub fn merge(&mut self, other: Usage) {
        for m in Metric::all() {
            self.add(m, other.get(m));
        }
    }
}

/// 単位ごとの上限（省略は無制限）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Limits {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub searches: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fetches: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub results: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chars: Option<u64>,
}

impl Limits {
    pub fn get(&self, metric: Metric) -> Option<u64> {
        match metric {
            Metric::Searches => self.searches,
            Metric::Fetches => self.fetches,
            Metric::Results => self.results,
            Metric::Chars => self.chars,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.searches.is_none()
            && self.fetches.is_none()
            && self.results.is_none()
            && self.chars.is_none()
    }
}

/// 時間窓。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WindowLimits {
    pub minutes: u64,
    #[serde(flatten)]
    pub limits: Limits,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum OnExceed {
    #[default]
    Stop,
    Warn,
}

/// クォータポリシー。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct QuotaPolicy {
    pub schema_version: u32,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub enabled: bool,
    pub session: Limits,
    pub day: Limits,
    pub window: Vec<WindowLimits>,
    pub total: Limits,
    pub on_exceed: OnExceed,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for QuotaPolicy {
    fn default() -> Self {
        Self {
            schema_version: 2,
            id: "default".into(),
            description: None,
            enabled: true,
            session: Limits {
                searches: Some(50),
                fetches: Some(100),
                results: Some(500),
                chars: Some(200_000),
            },
            day: Limits {
                searches: Some(500),
                fetches: Some(1000),
                results: Some(5000),
                chars: Some(2_000_000),
            },
            window: Vec::new(),
            total: Limits::default(),
            on_exceed: OnExceed::Stop,
            extra: Default::default(),
        }
    }
}

/// 窓のカウンタ。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WindowCounter {
    pub minutes: u64,
    pub started_at: String,
    pub used: Usage,
}

/// 永続化するカウンタ。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Counters {
    pub day_key: String,
    /// セッション消費量の持ち主（セッション識別子）。
    pub session_key: String,
    pub session: Usage,
    pub day: Usage,
    pub window: Vec<WindowCounter>,
    pub total: Usage,
    pub updated_at: String,
}

/// 1 項目の残量表示。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricUsage {
    pub name: String,
    pub used: u64,
    pub limit: Option<u64>,
    pub resets_at: Option<String>,
}

impl MetricUsage {
    pub fn remaining(&self) -> Option<u64> {
        self.limit.map(|l| l.saturating_sub(self.used))
    }

    pub fn ratio(&self) -> Option<f32> {
        match self.limit {
            Some(0) => Some(1.0),
            Some(l) => Some(self.used as f32 / l as f32),
            None => None,
        }
    }
}

/// 上限判定の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuotaDecision {
    Allowed { warn: bool },
    Exceeded(ExceedInfo),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExceedInfo {
    pub metric: Metric,
    pub scope: &'static str,
    pub limit: u64,
    pub used: u64,
    pub resets_at: Option<String>,
}

impl ExceedInfo {
    pub fn to_error(&self) -> AppError {
        let mut detail = format!(
            "{}.{} = {}/{}",
            self.scope,
            self.metric.as_str(),
            self.used,
            self.limit
        );
        if let Some(at) = &self.resets_at {
            detail.push_str(&format!("（リセット {at}）"));
        }
        AppError::with_detail(ErrorCode::E5005, detail)
    }
}

/// 上限判定。
pub fn check(
    policy: &QuotaPolicy,
    counters: &Counters,
    session: &Usage,
    metric: Metric,
    amount: u64,
    now_local_date: chrono::NaiveDate,
    now_utc: chrono::DateTime<chrono::Utc>,
) -> QuotaDecision {
    if !policy.enabled {
        return QuotaDecision::Allowed { warn: false };
    }
    let mut over: Option<ExceedInfo> = None;

    let test = |scope: &'static str,
                limits: &Limits,
                used: u64,
                resets_at: Option<String>,
                over: &mut Option<ExceedInfo>| {
        if let Some(limit) = limits.get(metric) {
            let hard = match policy.on_exceed {
                OnExceed::Stop => limit,
                OnExceed::Warn => limit.saturating_mul(2),
            };
            if used.saturating_add(amount) > hard {
                *over = Some(ExceedInfo {
                    metric,
                    scope,
                    limit,
                    used,
                    resets_at,
                });
            }
        }
    };

    let day_reset = format!(
        "{}T00:00 local",
        now_local_date.succ_opt().unwrap_or(now_local_date)
    );
    test(
        "day",
        &policy.day,
        counters.day.get(metric),
        Some(day_reset),
        &mut over,
    );
    test(
        "session",
        &policy.session,
        session.get(metric),
        None,
        &mut over,
    );
    test(
        "total",
        &policy.total,
        counters.total.get(metric),
        None,
        &mut over,
    );

    for w in policy.window.iter() {
        if w.limits.get(metric).is_none() {
            continue;
        }
        let counter = counters.window.iter().find(|c| c.minutes == w.minutes);
        let (used, resets) = match counter {
            Some(c) => {
                let active = window_active(c, now_utc);
                if active {
                    let started = chrono::DateTime::parse_from_rfc3339(&c.started_at)
                        .ok()
                        .map(|d| d.with_timezone(&chrono::Utc));
                    let resets = started
                        .map(|s| s + chrono::Duration::minutes(c.minutes as i64))
                        .map(|t| t.to_rfc3339());
                    (c.used.get(metric), resets)
                } else {
                    (0, None)
                }
            }
            None => (0, None),
        };
        let scope: &'static str = "window";
        test(scope, &w.limits, used, resets, &mut over);
    }

    match over {
        None => QuotaDecision::Allowed {
            warn: matches!(policy.on_exceed, OnExceed::Warn),
        },
        Some(info) => {
            if matches!(policy.on_exceed, OnExceed::Warn) {
                // 2 倍に達した場合のみ停止（check 内で hard 判定済み）
                QuotaDecision::Exceeded(info)
            } else {
                QuotaDecision::Exceeded(info)
            }
        }
    }
}

pub fn window_active(counter: &WindowCounter, now: chrono::DateTime<chrono::Utc>) -> bool {
    let started = match chrono::DateTime::parse_from_rfc3339(&counter.started_at) {
        Ok(d) => d.with_timezone(&chrono::Utc),
        Err(_) => return false,
    };
    now < started + chrono::Duration::minutes(counter.minutes as i64)
}

/// 残量一覧を作る。
pub fn remaining(
    policy: &QuotaPolicy,
    counters: &Counters,
    session: &Usage,
    now_utc: chrono::DateTime<chrono::Utc>,
    now_local_date: chrono::NaiveDate,
) -> Vec<MetricUsage> {
    let mut out = Vec::new();
    let day_reset = format!(
        "{}T00:00 local",
        now_local_date.succ_opt().unwrap_or(now_local_date)
    );
    for metric in Metric::all() {
        for (scope, limits, used, resets) in [
            ("session", &policy.session, session.get(metric), None),
            (
                "day",
                &policy.day,
                counters.day.get(metric),
                Some(day_reset.clone()),
            ),
            ("total", &policy.total, counters.total.get(metric), None),
        ] {
            if let Some(limit) = limits.get(metric) {
                out.push(MetricUsage {
                    name: format!("{scope}.{}", metric.as_str()),
                    used,
                    limit: Some(limit),
                    resets_at: resets,
                });
            }
        }
    }
    for w in &policy.window {
        for metric in Metric::all() {
            if let Some(limit) = w.limits.get(metric) {
                let used = counters
                    .window
                    .iter()
                    .find(|c| c.minutes == w.minutes)
                    .filter(|c| window_active(c, now_utc))
                    .map(|c| c.used.get(metric))
                    .unwrap_or(0);
                out.push(MetricUsage {
                    name: format!("window{}m.{}", w.minutes, metric.as_str()),
                    used,
                    limit: Some(limit),
                    resets_at: None,
                });
            }
        }
    }
    out
}

/// ポリシーの検証。
pub fn validate(policy: &QuotaPolicy) -> Vec<Check> {
    let mut out = Vec::new();
    if policy.schema_version != 2 {
        out.push(Check::Fail(format!(
            "quotas.schemaVersion = {}（対応版は 2）",
            policy.schema_version
        )));
    }
    for w in &policy.window {
        if w.minutes == 0 || w.minutes > 10080 {
            out.push(Check::Fail(format!(
                "quotas.window.minutes = {}（許容範囲 1〜10080）",
                w.minutes
            )));
        }
        if w.limits.is_empty() {
            out.push(Check::Warn("quotas.window に上限が 1 つも無い".into()));
        }
    }
    if out.iter().all(|c| !c.is_fail()) && out.is_empty() {
        out.push(Check::Ok(format!("quota policy '{}'", policy.id)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> (chrono::NaiveDate, chrono::DateTime<chrono::Utc>) {
        (
            chrono::NaiveDate::from_ymd_opt(2026, 11, 7).unwrap(),
            chrono::DateTime::parse_from_rfc3339("2026-11-07T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        )
    }

    #[test]
    fn allows_until_limit_then_stops() {
        let (date, utc) = now();
        let mut policy = QuotaPolicy::default();
        policy.day.searches = Some(2);
        let mut counters = Counters::default();
        let session = Usage::default();
        assert!(matches!(
            check(&policy, &counters, &session, Metric::Searches, 1, date, utc),
            QuotaDecision::Allowed { .. }
        ));
        counters.day.searches = 2;
        assert!(matches!(
            check(&policy, &counters, &session, Metric::Searches, 1, date, utc),
            QuotaDecision::Exceeded(_)
        ));
    }

    #[test]
    fn warn_mode_stops_at_double() {
        let (date, utc) = now();
        let mut policy = QuotaPolicy {
            on_exceed: OnExceed::Warn,
            ..QuotaPolicy::default()
        };
        policy.day.searches = Some(1);
        let mut counters = Counters::default();
        let session = Usage::default();
        counters.day.searches = 1;
        match check(&policy, &counters, &session, Metric::Searches, 0, date, utc) {
            QuotaDecision::Allowed { warn } => assert!(warn),
            other => panic!("unexpected: {other:?}"),
        }
        counters.day.searches = 2;
        assert!(matches!(
            check(&policy, &counters, &session, Metric::Searches, 1, date, utc),
            QuotaDecision::Exceeded(_)
        ));
    }

    #[test]
    fn window_expires() {
        let counter = WindowCounter {
            minutes: 60,
            started_at: "2026-11-07T00:00:00Z".into(),
            used: Usage {
                searches: 5,
                ..Default::default()
            },
        };
        let (_, utc) = now();
        assert!(window_active(&counter, utc));
        assert!(!window_active(
            &counter,
            utc + chrono::Duration::minutes(61)
        ));
    }
}
