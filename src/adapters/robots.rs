//! robots.txt の取得・判定・キャッシュ。
//!
//! 判定そのものは `crate::core::robots`（純粋ロジック）にあり、ここでは
//! 「どこから取るか」「どう保持するか」「拒否をどうエラーにするか」だけを扱う。

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use url::Url;

use crate::core::config::NetworkProfile;
use crate::core::errors::{AppError, ErrorCode};
use crate::core::robots::{Robots, Verdict};
use crate::ports::http::HttpRequest;
use crate::ports::{Clock, HttpClient, Reporter};

/// robots.txt の判定ゲート（本文取得の前に呼ぶ）。
pub struct RobotsGate {
    cache: RobotsCache,
    client: Arc<dyn HttpClient>,
    clock: Arc<dyn Clock>,
    reporter: Arc<dyn Reporter>,
    /// 取得失敗理由の伏字化に使う値（エラー文字列に URL・認証値が混ざるため）。
    secrets: Vec<String>,
}

impl RobotsGate {
    pub fn new(
        path: std::path::PathBuf,
        client: Arc<dyn HttpClient>,
        clock: Arc<dyn Clock>,
        reporter: Arc<dyn Reporter>,
    ) -> Self {
        Self {
            cache: RobotsCache::new(path),
            client,
            clock,
            reporter,
            secrets: Vec::new(),
        }
    }

    /// 取得失敗理由の伏字化に使う秘匿値を与える。
    pub fn with_secrets(mut self, secrets: Vec<String>) -> Self {
        self.secrets = secrets;
        self
    }

    /// 取得してよいか判定する。拒否の場合は `E2006` を返す。
    ///
    /// `network.robots.respect` が偽のときは何もしない（取得もファイルの
    /// 読み書きも行わない）。
    pub fn check(&self, profile: &NetworkProfile, url: &Url) -> Result<(), AppError> {
        if !profile.robots.respect {
            return Ok(());
        }
        let Some(origin) = origin_of(url) else {
            return Ok(());
        };
        let rules = match self
            .cache
            .get(&origin, self.clock.now_utc(), profile.robots.ttl_minutes)
        {
            Some(cached) => cached,
            None => self.load(profile, &origin)?,
        };
        let path = request_path(url);
        if rules.verdict(&profile.robots.user_agent_token, &path) == Verdict::Deny {
            return Err(AppError::with_detail(
                ErrorCode::E2006,
                format!("{origin} の robots.txt が {path} を拒否している"),
            )
            .hint("別の URL を選ぶか、`network.robots.respect` の設定を確認する"));
        }
        Ok(())
    }

    /// robots.txt を取ってキャッシュへ入れる。取れない場合は `onError` に従う。
    fn load(&self, profile: &NetworkProfile, origin: &str) -> Result<Robots, AppError> {
        let robots_url = match Url::parse(&format!("{origin}/robots.txt")) {
            Ok(url) => url,
            Err(_) => return self.on_fetch_failure(profile, origin, "URL を組み立てられない"),
        };
        let request = HttpRequest::get(robots_url);
        let now = self.clock.now_utc();
        match self.client.send_blocking(profile, &request) {
            Ok(response) => {
                let text = String::from_utf8_lossy(&response.body);
                let robots = Robots::parse(&text, profile.robots.max_bytes);
                self.cache.put(origin, &robots, now);
                self.reporter.verbose(&format!(
                    "{origin} の robots.txt を読んだ（規則 {} 群）",
                    robots.groups.len()
                ));
                Ok(robots)
            }
            Err(err) => self.on_fetch_failure(profile, origin, &format!("{err}")),
        }
    }

    /// 取得失敗時の扱い（`onError`）。`deny` は `E2006`、`allow` は規則なし。
    fn on_fetch_failure(
        &self,
        profile: &NetworkProfile,
        origin: &str,
        reason: &str,
    ) -> Result<Robots, AppError> {
        let reason = crate::core::secret::redact_text(reason, &self.secrets);
        if profile.robots.on_error.trim().eq_ignore_ascii_case("deny") {
            return Err(AppError::with_detail(
                ErrorCode::E2006,
                format!("{origin} の robots.txt を取得できない: {reason}"),
            )
            .hint("`network.robots.onError` を `allow` にするか、ネットワーク設定を確認する"));
        }
        self.reporter.verbose(&format!(
            "{origin} の robots.txt を取得できないため許可する: {reason}"
        ));
        // 「規則なし」として保持し、同じホストの文書ごとに取り直さない。
        let robots = Robots::default();
        self.cache.put(origin, &robots, self.clock.now_utc());
        Ok(robots)
    }
}

/// オリジン（`scheme://host[:port]`）。
fn origin_of(url: &Url) -> Option<String> {
    let host = url.host_str()?;
    match url.port() {
        Some(port) => Some(format!("{}://{host}:{port}", url.scheme())),
        None => Some(format!("{}://{host}", url.scheme())),
    }
}

/// 判定に使うパス（パス + クエリ。RFC 9309）。
fn request_path(url: &Url) -> String {
    match url.query() {
        Some(query) if !query.is_empty() => format!("{}?{query}", url.path()),
        _ => url.path().to_string(),
    }
}

/// 判定結果のキャッシュ（`<state-dir>/cache/robots.json`）。
pub struct RobotsCache {
    path: std::path::PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RobotsEntry {
    origin: String,
    fetched_at: String,
    robots: Robots,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RobotsFile {
    entries: Vec<RobotsEntry>,
}

impl RobotsCache {
    pub fn new(path: std::path::PathBuf) -> Self {
        Self { path }
    }

    /// 有効期限内なら保持した規則を返す。
    pub fn get(
        &self,
        origin: &str,
        now: chrono::DateTime<chrono::Utc>,
        ttl_minutes: u64,
    ) -> Option<Robots> {
        let file = self.read()?;
        let entry = file.entries.iter().find(|e| e.origin == origin)?;
        let fetched_at = chrono::DateTime::parse_from_rfc3339(&entry.fetched_at).ok()?;
        let age = now - fetched_at.with_timezone(&chrono::Utc);
        if age.num_minutes() >= ttl_minutes.max(1) as i64 {
            return None;
        }
        Some(entry.robots.clone())
    }

    /// 規則を保持する（同じオリジンは置き換える）。
    ///
    /// 書き込みに失敗しても取得を止めない（キャッシュは最適化のため）。
    pub fn put(&self, origin: &str, robots: &Robots, now: chrono::DateTime<chrono::Utc>) {
        let mut file = self.read().unwrap_or_default();
        file.entries.retain(|e| e.origin != origin);
        file.entries.push(RobotsEntry {
            origin: origin.to_string(),
            fetched_at: now.to_rfc3339(),
            robots: robots.clone(),
        });
        let text = match serde_json::to_string_pretty(&file) {
            Ok(text) => text,
            Err(_) => return,
        };
        let _ = crate::adapters::state::write_atomic(&self.path, text.as_bytes());
    }

    fn read(&self) -> Option<RobotsFile> {
        let text = std::fs::read_to_string(&self.path).ok()?;
        serde_json::from_str(&text).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn origin_keeps_scheme_and_port() {
        assert_eq!(
            origin_of(&url("https://example.com/a/b?q=1")).as_deref(),
            Some("https://example.com")
        );
        assert_eq!(
            origin_of(&url("http://example.com:8080/a")).as_deref(),
            Some("http://example.com:8080")
        );
    }

    #[test]
    fn path_includes_query_when_present() {
        assert_eq!(request_path(&url("https://x.test/a?b=1")), "/a?b=1");
        assert_eq!(request_path(&url("https://x.test/a")), "/a");
        assert_eq!(request_path(&url("https://x.test/a?")), "/a");
    }
}
