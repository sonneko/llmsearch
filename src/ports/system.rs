//! 実時間・環境変数・端末に基づくシステム実装。

use std::path::PathBuf;

use crate::core::config::NetworkProfile;
use crate::core::config::network::ProxyAuth;
use crate::core::config::network::ProxySpec;
use crate::ports::{Clock, Env, IdGen};

/// 実時間。
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock {
    start: Option<std::time::Instant>,
}

impl SystemClock {
    pub fn new() -> Self {
        Self {
            start: Some(std::time::Instant::now()),
        }
    }
}

impl Clock for SystemClock {
    fn now_utc(&self) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc::now()
    }

    fn today_local(&self) -> chrono::NaiveDate {
        chrono::Local::now().date_naive()
    }

    fn monotonic_ns(&self) -> u64 {
        self.start
            .map(|s| s.elapsed().as_nanos() as u64)
            .unwrap_or(0)
    }

    fn sleep(&self, duration: std::time::Duration) {
        std::thread::sleep(duration);
    }
}

/// 乱数を使うセッション識別子の生成。
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemIdGen;

impl SystemIdGen {
    pub fn new() -> Self {
        Self
    }
}

impl IdGen for SystemIdGen {
    fn session_id(&self, now: chrono::DateTime<chrono::Utc>) -> String {
        use std::hash::{BuildHasher, Hasher};
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u64(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0),
        );
        hasher.write_u32(std::process::id());
        let value = hasher.finish();
        let suffix: String = value
            .to_string()
            .chars()
            .take(4)
            .collect::<String>()
            .to_lowercase();
        let suffix = format!(
            "{}{}",
            suffix,
            "abcd"
                .chars()
                .take(4 - suffix.len().min(4))
                .collect::<String>()
        );
        format!(
            "{}-{}",
            now.format("%Y%m%d-%H%M%S"),
            &suffix[..suffix.len().min(4)]
        )
    }
}

/// 実環境。
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemEnv;

impl SystemEnv {
    pub fn new() -> Self {
        Self
    }
}

impl Env for SystemEnv {
    fn var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok().filter(|v| !v.is_empty())
    }

    fn var_names(&self) -> Vec<String> {
        std::env::vars().map(|(k, _)| k).collect()
    }

    fn is_stdin_tty(&self) -> bool {
        use std::io::IsTerminal;
        std::io::stdin().is_terminal()
    }

    fn is_stdout_tty(&self) -> bool {
        use std::io::IsTerminal;
        std::io::stdout().is_terminal()
    }

    fn terminal_width(&self) -> usize {
        std::env::var("COLUMNS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|v| *v >= 20)
            .unwrap_or(100)
    }

    fn home_dir(&self) -> Option<PathBuf> {
        dirs::home_dir()
    }

    fn state_home(&self) -> Option<PathBuf> {
        std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                dirs::state_dir().or_else(|| dirs::home_dir().map(|h| h.join(".local/state")))
            })
    }

    fn config_home(&self) -> Option<PathBuf> {
        dirs::config_dir()
    }
}

/// プロキシの認証情報を環境変数から解決する（表示には出さない）。
pub fn proxy_password(profile: &NetworkProfile, env: &dyn Env) -> Option<String> {
    match &profile.proxy {
        Some(ProxySpec::Detailed(d)) => match &d.auth {
            Some(ProxyAuth::Env { value_env }) => env.var(value_env.as_str()),
            Some(ProxyAuth::Plain { value }) => Some(value.clone()),
            None => None,
        },
        _ => None,
    }
}

/// プロキシ URL を取り出す（未設定時は環境変数）。
pub fn proxy_url(profile: &NetworkProfile, env: &dyn Env) -> Option<String> {
    match &profile.proxy {
        Some(ProxySpec::Url(u)) => Some(u.clone()),
        Some(ProxySpec::Detailed(d)) => Some(d.url.clone()),
        None => env
            .var("HTTPS_PROXY")
            .or_else(|| env.var("https_proxy"))
            .or_else(|| env.var("HTTP_PROXY"))
            .or_else(|| env.var("http_proxy")),
    }
}

/// `noProxy` に該当するか。
pub fn in_no_proxy(profile: &NetworkProfile, host: &str, env: &dyn Env) -> bool {
    let entries: Vec<String> = match &profile.proxy {
        Some(ProxySpec::Detailed(d)) => d.no_proxy.clone(),
        _ => env
            .var("NO_PROXY")
            .or_else(|| env.var("no_proxy"))
            .map(|v| v.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_default(),
    };
    entries.iter().any(|entry| {
        if entry == "*" {
            return true;
        }
        crate::core::url::suffix_match(host, entry)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_id_shape() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-07T05:06:07Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let id = SystemIdGen::new().session_id(now);
        assert!(id.starts_with("20261107-050607-"), "{id}");
        assert_eq!(id.len(), "20261107-050607-abcd".len());
    }

    #[test]
    fn no_proxy_wildcard() {
        let profile = NetworkProfile::default();
        let env = SystemEnv::new();
        assert!(!in_no_proxy(&profile, "example.com", &env));
    }
}
