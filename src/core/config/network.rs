//! ネットワークプロファイル（JSON）。

use serde::{Deserialize, Serialize};

use crate::core::errors::Check;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct NetworkProfile {
    pub schema_version: u32,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub timeout_connect_ms: u64,
    pub timeout_total_ms: u64,
    pub max_bytes: usize,
    pub max_redirects: usize,
    pub redirect_policy: RedirectPolicy,
    pub user_agent: String,
    pub headers: std::collections::BTreeMap<String, String>,
    pub compression: Vec<String>,
    pub http_version: String,
    pub min_interval_ms: u64,
    pub max_concurrency: usize,
    pub retry: RetryPolicy,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy: Option<ProxySpec>,
    pub tls: TlsSpec,
    pub robots: RobotsSpec,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub charset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accept_language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum RedirectPolicy {
    #[default]
    Follow,
    Manual,
    Never,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct RetryPolicy {
    pub enabled: bool,
    pub max_attempts: u32,
    pub backoff_ms: u64,
    pub backoff_factor: f64,
    pub max_backoff_ms: u64,
    pub jitter_ms: u64,
    pub on_status: Vec<u16>,
    pub on_error: Vec<String>,
    pub respect_retry_after: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            max_attempts: 2,
            backoff_ms: 500,
            backoff_factor: 2.0,
            max_backoff_ms: 8000,
            jitter_ms: 250,
            on_status: vec![429, 502, 503, 504],
            on_error: vec!["timeout".into(), "connect".into()],
            respect_retry_after: true,
        }
    }
}

impl RetryPolicy {
    /// 指数バックオフの待機時間（ms）。
    pub fn backoff(&self, attempt: u32, jitter: u64) -> u64 {
        if attempt == 0 {
            return 0;
        }
        let factor = self.backoff_factor.max(1.0);
        let base = self.backoff_ms as f64 * factor.powi((attempt as i32) - 1);
        let capped = base.min(self.max_backoff_ms as f64);
        let extra = if self.jitter_ms > 0 {
            jitter % (self.jitter_ms + 1)
        } else {
            0
        };
        (capped as u64).saturating_add(extra)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ProxySpec {
    Url(String),
    Detailed(ProxyDetail),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ProxyDetail {
    pub url: String,
    pub no_proxy: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<ProxyAuth>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ProxyAuth {
    Env { value_env: String },
    Plain { value: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TlsSpec {
    pub verify: bool,
    pub ca_file: String,
}

impl Default for TlsSpec {
    fn default() -> Self {
        Self {
            verify: true,
            ca_file: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct RobotsSpec {
    pub respect: bool,
    pub ttl_minutes: u64,
    pub user_agent_token: String,
    pub on_error: String,
    pub max_bytes: usize,
}

impl Default for RobotsSpec {
    fn default() -> Self {
        Self {
            respect: true,
            ttl_minutes: 60,
            user_agent_token: "llmsearch".into(),
            on_error: "allow".into(),
            max_bytes: 524288,
        }
    }
}

pub fn default_user_agent() -> String {
    format!(
        "llmsearch/{} (+{})",
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_REPOSITORY")
    )
}

impl Default for NetworkProfile {
    fn default() -> Self {
        Self {
            schema_version: 2,
            id: "default".into(),
            description: None,
            timeout_connect_ms: 5000,
            timeout_total_ms: 20000,
            max_bytes: 5 * 1024 * 1024,
            max_redirects: 5,
            redirect_policy: RedirectPolicy::Follow,
            user_agent: default_user_agent(),
            headers: Default::default(),
            compression: vec!["gzip".into(), "deflate".into(), "br".into()],
            http_version: "auto".into(),
            min_interval_ms: 1000,
            max_concurrency: 2,
            retry: RetryPolicy::default(),
            proxy: None,
            tls: TlsSpec::default(),
            robots: RobotsSpec::default(),
            charset: None,
            accept_language: None,
        }
    }
}

const FORBIDDEN_HEADERS: &[&str] = &["host", "content-length", "connection", "transfer-encoding"];

impl NetworkProfile {
    pub fn validate(&self) -> Vec<Check> {
        let mut out = Vec::new();
        let range = |name: &str, v: u64, min: u64, max: u64, out: &mut Vec<Check>| {
            if v < min || v > max {
                out.push(Check::Fail(format!(
                    "{name} = {v}（許容範囲 {min}〜{max}）"
                )));
            }
        };
        range(
            "network.timeoutConnectMs",
            self.timeout_connect_ms,
            100,
            60000,
            &mut out,
        );
        range(
            "network.timeoutTotalMs",
            self.timeout_total_ms,
            500,
            120000,
            &mut out,
        );
        range(
            "network.maxBytes",
            self.max_bytes as u64,
            1024,
            52428800,
            &mut out,
        );
        range(
            "network.maxRedirects",
            self.max_redirects as u64,
            0,
            10,
            &mut out,
        );
        range(
            "network.minIntervalMs",
            self.min_interval_ms,
            0,
            60000,
            &mut out,
        );
        range(
            "network.maxConcurrency",
            self.max_concurrency as u64,
            1,
            8,
            &mut out,
        );
        range(
            "network.retry.maxAttempts",
            self.retry.max_attempts as u64,
            1,
            5,
            &mut out,
        );
        if self.user_agent.trim().is_empty() {
            out.push(Check::Fail("network.userAgent は空にできない".into()));
        }
        for h in self.headers.keys() {
            if FORBIDDEN_HEADERS.contains(&h.to_ascii_lowercase().as_str()) {
                out.push(Check::Fail(format!("network.headers に禁止ヘッダ {h}")));
            }
        }
        if !self.tls.verify {
            out.push(Check::Warn(
                "network.tls.verify = false（証明書を検証しない）".into(),
            ));
        }
        if let Some(ProxySpec::Detailed(d)) = &self.proxy
            && let Some(ProxyAuth::Plain { .. }) = &d.auth
        {
            out.push(Check::Warn(
                "network.proxy.auth に値を直接記述している（value_env を推奨）".into(),
            ));
        }
        if !self.robots.respect {
            out.push(Check::Warn(
                "network.robots.respect = false（robots.txt を無視する）".into(),
            ));
        }
        if out.is_empty() {
            out.push(Check::Ok(format!("network profile '{}'", self.id)));
        }
        out
    }

    pub fn has_fail(&self) -> bool {
        self.validate().iter().any(|c| c.is_fail())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        assert!(!NetworkProfile::default().has_fail());
    }

    #[test]
    fn rejects_bad_timeout() {
        let p = NetworkProfile {
            timeout_total_ms: 10,
            ..NetworkProfile::default()
        };
        assert!(p.has_fail());
    }

    #[test]
    fn backoff_grows_and_caps() {
        let r = RetryPolicy {
            backoff_ms: 100,
            backoff_factor: 2.0,
            max_backoff_ms: 300,
            jitter_ms: 0,
            ..Default::default()
        };
        assert_eq!(r.backoff(1, 0), 100);
        assert_eq!(r.backoff(2, 0), 200);
        assert_eq!(r.backoff(3, 0), 300);
    }
}
