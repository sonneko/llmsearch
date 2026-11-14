//! `reqwest` による HTTP 実装。挙動は `NetworkProfile` のみで決まる。
//!
//! ポートは同期 API なので、内部で専用の tokio ランタイムを回して `block_on` する。

use std::time::Duration;

use url::Url;

use crate::core::config::NetworkProfile;
use crate::core::config::network::RedirectPolicy;
use crate::core::errors::{AppError, Diagnostic, ErrorCode, WarningCode};
use crate::ports::http::{HttpRequest, HttpResponse};
use crate::ports::system::{SystemEnv, in_no_proxy, proxy_password, proxy_url_for_scheme};
use crate::ports::{Clock, Env, HttpClient, Reporter};

/// `reqwest` を使うクライアント。
pub struct ReqwestClient {
    clock: std::sync::Arc<dyn Clock>,
    env: std::sync::Arc<dyn Env>,
    /// 待機などの通知先（省略時は何も出さない）。
    reporter: Option<std::sync::Arc<dyn Reporter>>,
    runtime: tokio::runtime::Runtime,
    /// 直近の送信時刻（ホストごとのレート制限）。
    last: std::sync::Mutex<std::collections::HashMap<String, u64>>,
}

impl ReqwestClient {
    pub fn new(clock: std::sync::Arc<dyn Clock>) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("tokio ランタイムの生成に失敗");
        Self {
            clock,
            env: std::sync::Arc::new(SystemEnv::new()),
            reporter: None,
            runtime,
            last: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// 通知先を設定する（レート制限の待機を `W2001` として知らせるため）。
    pub fn with_reporter(mut self, reporter: std::sync::Arc<dyn Reporter>) -> Self {
        self.reporter = Some(reporter);
        self
    }

    /// 環境依存を差し替える（プロキシ設定の決定論的な検証用）。
    pub fn with_env(mut self, env: std::sync::Arc<dyn Env>) -> Self {
        self.env = env;
        self
    }

    fn build_client(&self, profile: &NetworkProfile) -> Result<reqwest::Client, AppError> {
        let mut builder = reqwest::Client::builder()
            .connect_timeout(Duration::from_millis(profile.timeout_connect_ms))
            .timeout(Duration::from_millis(profile.timeout_total_ms))
            .user_agent(profile.user_agent.clone())
            .danger_accept_invalid_certs(!profile.tls.verify)
            .redirect(match profile.redirect_policy {
                RedirectPolicy::Follow => reqwest::redirect::Policy::limited(profile.max_redirects),
                RedirectPolicy::Manual | RedirectPolicy::Never => reqwest::redirect::Policy::none(),
            })
            .gzip(profile.compression.iter().any(|c| c == "gzip"))
            .deflate(profile.compression.iter().any(|c| c == "deflate"))
            .brotli(profile.compression.iter().any(|c| c == "br"))
            .pool_max_idle_per_host(profile.max_concurrency)
            // reqwest の暗黙の環境探索を止め、下の明示的な規則に一本化する。
            .no_proxy();

        let http_proxy = resolve_proxy(profile, "http", self.env.as_ref())?;
        let https_proxy = resolve_proxy(profile, "https", self.env.as_ref())?;
        if http_proxy.is_some() || https_proxy.is_some() {
            let profile = profile.clone();
            let env = std::sync::Arc::clone(&self.env);
            builder = builder.proxy(reqwest::Proxy::custom(move |url| {
                if in_no_proxy(&profile, url.host_str().unwrap_or_default(), env.as_ref()) {
                    return None;
                }
                match url.scheme() {
                    "http" => http_proxy.clone(),
                    "https" => https_proxy.clone(),
                    _ => None,
                }
            }));
        }

        if let Some(pem) = read_ca(&profile.tls.ca_file) {
            let cert = reqwest::Certificate::from_pem(&pem)
                .map_err(|e| AppError::with_detail(ErrorCode::E2008, format!("CA 読込: {e}")))?;
            builder = builder.add_root_certificate(cert);
        }
        if profile.http_version == "1.1" {
            builder = builder.http1_only();
        } else if profile.http_version == "2" {
            builder = builder.http2_prior_knowledge();
        }
        builder
            .build()
            .map_err(|e| AppError::with_detail(ErrorCode::E2001, format!("クライアント生成: {e}")))
    }

    fn wait_interval(&self, profile: &NetworkProfile, host: &str) {
        if profile.min_interval_ms == 0 {
            return;
        }
        let now_ns = self.clock.monotonic_ns();
        let prev = {
            let map = match self.last.lock() {
                Ok(m) => m,
                Err(poison) => poison.into_inner(),
            };
            map.get(host).copied()
        };
        if let Some(prev) = prev {
            let elapsed_ms = now_ns.saturating_sub(prev) / 1_000_000;
            let remaining = profile.min_interval_ms.saturating_sub(elapsed_ms);
            if remaining > 0 {
                if let Some(reporter) = &self.reporter {
                    reporter.warn(&Diagnostic::new(
                        WarningCode::W2001,
                        format!("{host} への送信を {remaining} ms 待機した（レート制限）"),
                    ));
                }
                self.clock.sleep(Duration::from_millis(remaining));
            }
        }
        if let Ok(mut map) = self.last.lock() {
            map.insert(host.to_string(), self.clock.monotonic_ns());
        }
    }

    async fn send_once(
        client: &reqwest::Client,
        profile: &NetworkProfile,
        request: &HttpRequest,
    ) -> Result<HttpResponse, AppError> {
        let mut builder = match request.method.to_ascii_uppercase().as_str() {
            "POST" => client.post(request.url.clone()),
            "HEAD" => client.head(request.url.clone()),
            _ => client.get(request.url.clone()),
        };
        if let Some(timeout) = request.timeout_ms {
            builder = builder.timeout(Duration::from_millis(timeout));
        }
        for (k, v) in &request.headers {
            builder = builder.header(k, v);
        }
        for (k, v) in &profile.headers {
            builder = builder.header(k, v);
        }
        if let Some(lang) = &profile.accept_language {
            builder = builder.header("Accept-Language", lang);
        }
        if let Some(body) = &request.body {
            builder = builder.body(body.clone());
        }

        let response = builder
            .send()
            .await
            .map_err(|e| map_reqwest_error(e, request))?;
        let status = response.status().as_u16();
        let final_url = response.url().clone();
        let headers: Vec<(String, String)> = response
            .headers()
            .iter()
            .map(|(k, v)| {
                (
                    k.as_str().to_ascii_lowercase(),
                    String::from_utf8_lossy(v.as_bytes()).to_string(),
                )
            })
            .collect();

        if profile.redirect_policy == RedirectPolicy::Manual
            && let Some(location) = response.headers().get("location")
        {
            let target = String::from_utf8_lossy(location.as_bytes()).to_string();
            if let Ok(next) = request.url.join(&target) {
                return Ok(HttpResponse {
                    status,
                    final_url: next,
                    headers,
                    body: Vec::new(),
                    truncated: false,
                    redirects: 1,
                });
            }
        }

        if status >= 400 {
            return Err(http_status_error(status, &headers));
        }

        // サイズ上限を意識して読み込む
        let mut body: Vec<u8> = Vec::new();
        let mut truncated = false;
        let mut stream = response;
        loop {
            match stream.chunk().await {
                Ok(Some(chunk)) => {
                    if body.len() + chunk.len() > profile.max_bytes {
                        let take = profile.max_bytes.saturating_sub(body.len());
                        body.extend_from_slice(&chunk[..take]);
                        truncated = true;
                        break;
                    }
                    body.extend_from_slice(&chunk);
                }
                Ok(None) => break,
                Err(e) => {
                    return Err(AppError::with_detail(
                        ErrorCode::E2002,
                        format!("本文の読み込みに失敗: {}", e.without_url()),
                    ));
                }
            }
        }

        Ok(HttpResponse {
            status,
            final_url,
            headers,
            body,
            truncated,
            redirects: 0,
        })
    }
}

/// エラーに URL や認証値を含めず、プロキシ設定を解決する。
fn resolve_proxy(
    profile: &NetworkProfile,
    scheme: &str,
    env: &dyn Env,
) -> Result<Option<Url>, AppError> {
    let Some(raw) = proxy_url_for_scheme(profile, scheme, env) else {
        return Ok(None);
    };
    let invalid = || {
        AppError::with_detail(
            ErrorCode::E1004,
            "proxy.url は有効な HTTP(S) プロキシ URL が必要",
        )
    };
    let mut url = Url::parse(&raw).map_err(|_| invalid())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(invalid());
    }
    if let Some(crate::core::config::network::ProxySpec::Detailed(detail)) = &profile.proxy
        && detail.auth.is_some()
    {
        let password = proxy_password(profile, env).ok_or_else(|| {
            AppError::with_detail(ErrorCode::E1004, "proxy.auth.value_env の環境変数が未設定")
        })?;
        url.set_password(Some(&password)).map_err(|_| invalid())?;
    }
    Ok(Some(url))
}

fn read_ca(path: &str) -> Option<Vec<u8>> {
    if path.trim().is_empty() {
        None
    } else {
        std::fs::read(path).ok()
    }
}

fn map_reqwest_error(err: reqwest::Error, request: &HttpRequest) -> AppError {
    let err = err.without_url();
    let host = request.url.host_str().unwrap_or("(unknown)");
    if err.is_timeout() {
        return AppError::with_detail(ErrorCode::E2002, format!("{host}: {err}"));
    }
    if err.is_connect() {
        return AppError::with_detail(ErrorCode::E2001, format!("{host}: {err}"));
    }
    if err.is_redirect() {
        return AppError::with_detail(ErrorCode::E2004, format!("{host}: {err}"));
    }
    let text = err.to_string().to_lowercase();
    if text.contains("certificate") || text.contains("tls") {
        return AppError::with_detail(ErrorCode::E2008, host.to_string());
    }
    AppError::with_detail(ErrorCode::E2001, format!("{host}: {err}"))
}

fn http_status_error(status: u16, headers: &[(String, String)]) -> AppError {
    let retry_after = headers
        .iter()
        .find(|(k, _)| k == "retry-after")
        .map(|(_, v)| v.clone());
    let detail = match retry_after {
        Some(v) => format!("HTTP {status}（Retry-After: {v}）"),
        None => format!("HTTP {status}"),
    };
    match status {
        429 | 403 => AppError::with_detail(ErrorCode::E4003, detail),
        408 => AppError::with_detail(ErrorCode::E2002, detail),
        _ => AppError::with_detail(ErrorCode::E2003, detail),
    }
}

impl HttpClient for ReqwestClient {
    fn send_blocking(
        &self,
        profile: &NetworkProfile,
        request: &HttpRequest,
    ) -> Result<HttpResponse, AppError> {
        crate::core::url::validate_scheme(&request.url)?;
        let host = request.url.host_str().unwrap_or_default().to_string();

        // リダイレクト先にも同じプロキシ選択・除外規則を適用する。
        let client = self.build_client(profile)?;
        let attempts = profile.retry.max_attempts.max(1);
        let mut last_error: Option<AppError> = None;

        for attempt in 0..attempts {
            if attempt > 0 {
                let jitter = self.clock.monotonic_ns();
                let wait = profile.retry.backoff(attempt, jitter);
                if wait > 0 {
                    self.clock.sleep(Duration::from_millis(wait));
                }
            }
            self.wait_interval(profile, &host);

            let outcome = self
                .runtime
                .block_on(Self::send_once(&client, profile, request));
            match outcome {
                Ok(response) => return Ok(response),
                Err(err) => {
                    let retryable = retryable(profile, &err);
                    if !retryable {
                        return Err(err);
                    }
                    last_error = Some(err);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| AppError::new(ErrorCode::E2001)))
    }

    fn describe(&self, profile: &NetworkProfile) -> String {
        format!(
            "{}: timeout {}ms / max {} bytes / {} 並列 / 間隔 {}ms",
            profile.id,
            profile.timeout_total_ms,
            profile.max_bytes,
            profile.max_concurrency,
            profile.min_interval_ms
        )
    }
}

fn retryable(profile: &NetworkProfile, err: &AppError) -> bool {
    if !profile.retry.enabled {
        return false;
    }
    match err.code {
        ErrorCode::E2001 | ErrorCode::E2002 => profile
            .retry
            .on_error
            .iter()
            .any(|e| matches!(e.as_str(), "timeout" | "connect")),
        ErrorCode::E4003 => true,
        ErrorCode::E2003 => {
            let status = err
                .detail
                .as_deref()
                .and_then(|d| d.split_whitespace().nth(1))
                .and_then(|s| s.parse::<u16>().ok())
                .unwrap_or(0);
            profile.retry.on_status.contains(&status)
        }
        _ => false,
    }
}

/// プロファイルの危険設定を警告へ変換する。
pub fn profile_warnings(profile: &NetworkProfile) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    if !profile.tls.verify {
        out.push(Diagnostic::new(
            WarningCode::W1005,
            format!("network '{}' は TLS 検証を無効にしている", profile.id),
        ));
    }
    if let Some(crate::core::config::network::ProxySpec::Detailed(d)) = &profile.proxy
        && matches!(
            d.auth,
            Some(crate::core::config::network::ProxyAuth::Plain { .. })
        )
    {
        out.push(Diagnostic::new(
            WarningCode::W1004,
            format!(
                "network '{}' のプロキシ認証に値を直接記述している",
                profile.id
            ),
        ));
    }
    out
}

/// URL を表示用に変換する（秘匿ヘッダ・クエリを伏字化）。
pub fn redact_for_display(url: &Url, secrets: &[String]) -> String {
    crate::core::url::redact(url, secrets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_errors_map_to_codes() {
        assert_eq!(http_status_error(429, &[]).code, ErrorCode::E4003);
        assert_eq!(http_status_error(500, &[]).code, ErrorCode::E2003);
        assert_eq!(http_status_error(408, &[]).code, ErrorCode::E2002);
    }

    #[test]
    fn warnings_flag_dangerous_settings() {
        let mut profile = NetworkProfile::default();
        profile.tls.verify = false;
        let w = profile_warnings(&profile);
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].code, WarningCode::W1005);
    }

    #[test]
    fn retryability_follows_profile() {
        let mut profile = NetworkProfile::default();
        assert!(retryable(
            &profile,
            &AppError::with_detail(ErrorCode::E2002, "t")
        ));
        profile.retry.enabled = false;
        assert!(!retryable(
            &profile,
            &AppError::with_detail(ErrorCode::E2002, "t")
        ));
        assert!(retryable(
            &NetworkProfile::default(),
            &AppError::with_detail(ErrorCode::E2003, "HTTP 503")
        ));
        assert!(!retryable(
            &NetworkProfile::default(),
            &AppError::with_detail(ErrorCode::E2003, "HTTP 404")
        ));
    }
}
