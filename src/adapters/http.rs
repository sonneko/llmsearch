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
    /// 追加で信頼するルート証明書のキャッシュ（キーは解決に使った入力）。
    /// クライアントを作り直すたびにシステムストアを読み直さないためのもの。
    roots: std::sync::Mutex<Option<(String, Vec<reqwest::Certificate>)>>,
}

impl ReqwestClient {
    pub fn new(clock: std::sync::Arc<dyn Clock>) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("failed to build the tokio runtime");
        Self {
            clock,
            env: std::sync::Arc::new(SystemEnv::new()),
            reporter: None,
            runtime,
            last: std::sync::Mutex::new(std::collections::HashMap::new()),
            roots: std::sync::Mutex::new(None),
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

        // 検証しない設定ではルート証明書を読まない（読んでも使わない）。
        if profile.tls.verify {
            for cert in self.root_certificates(&profile.tls.ca_file)? {
                builder = builder.add_root_certificate(cert);
            }
        }
        if profile.http_version == "1.1" {
            builder = builder.http1_only();
        } else if profile.http_version == "2" {
            builder = builder.http2_prior_knowledge();
        }
        builder.build().map_err(|e| {
            AppError::with_detail(ErrorCode::E2001, format!("client construction: {e}"))
        })
    }

    /// `tls.caFile` とシステムルートストアを合わせた追加のルート証明書。
    ///
    /// システムストアの読み込みは送信ごとに繰り返さないよう保持する。
    /// 保持のキーには解決に使った入力（設定値と環境変数）を含め、差し替えを
    /// 反映できるようにする。
    pub fn root_certificates(&self, ca_file: &str) -> Result<Vec<reqwest::Certificate>, AppError> {
        let key = root_cache_key(ca_file, self.env.as_ref());
        if let Ok(guard) = self.roots.lock()
            && let Some((cached_key, certs)) = guard.as_ref()
            && *cached_key == key
        {
            return Ok(certs.clone());
        }
        let roots = crate::adapters::tls::load(self.env.as_ref(), ca_file)?;
        if let Some(reporter) = &self.reporter
            && !roots.sources.is_empty()
        {
            reporter.verbose(&format!(
                "added TLS root certificates: {} ({} items)",
                roots.sources.join("; "),
                roots.len()
            ));
        }
        let certs = roots.certs().to_vec();
        if let Ok(mut guard) = self.roots.lock() {
            *guard = Some((key, certs.clone()));
        }
        Ok(certs)
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
                        format!("waited {remaining} ms before sending to {host} (rate limit)"),
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
        now: chrono::DateTime<chrono::Utc>,
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

        // 3xx: ポリシーに従う。`manual` は追わずに `Location` を渡す。
        if (300..400).contains(&status) {
            match profile.redirect_policy {
                RedirectPolicy::Never => return Err(redirect_error(status)),
                RedirectPolicy::Manual => {
                    if let Some(location) = response.headers().get("location") {
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
                    // `Location` が解決できない 3xx は追えないのでエラーにする。
                    return Err(AppError::with_detail(
                        ErrorCode::E2004,
                        format!("HTTP {status} (cannot resolve Location)"),
                    )
                    .status(status));
                }
                // `follow` は reqwest が追う。追いきれなかった分だけが残る。
                RedirectPolicy::Follow => {
                    let max = profile.max_redirects;
                    return Err(AppError::with_detail(
                        ErrorCode::E2004,
                        format!(
                            "HTTP {status} (cannot follow within the configured limit of {max})"
                        ),
                    )
                    .status(status));
                }
            }
        }

        if status >= 400 {
            return Err(http_status_error(status, &headers, now));
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
                        format!("failed to read the body: {}", e.without_url()),
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
            "proxy.url must be a valid HTTP(S) proxy URL",
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
            AppError::with_detail(
                ErrorCode::E1004,
                "the environment variable named by proxy.auth.value_env is not set",
            )
        })?;
        url.set_password(Some(&password)).map_err(|_| invalid())?;
    }
    Ok(Some(url))
}

/// ルート証明書キャッシュのキー（`tls.caFile` と参照した環境変数の値）。
fn root_cache_key(ca_file: &str, env: &dyn Env) -> String {
    let mut key = String::from(ca_file.trim());
    for name in crate::adapters::tls::BUNDLE_ENV_VARS
        .iter()
        .chain(crate::adapters::tls::DIR_ENV_VARS.iter())
    {
        key.push('\u{1f}');
        key.push_str(name);
        key.push('=');
        key.push_str(env.var(name).unwrap_or_default().trim());
    }
    key
}

fn map_reqwest_error(err: reqwest::Error, request: &HttpRequest) -> AppError {
    let err = err.without_url();
    let host = request.url.host_str().unwrap_or("(unknown)");
    // 送信エラーの文字列には URL や認証値が混ざることがあるため伏字化してから使う。
    let text = crate::core::secret::redact_text(&err.to_string(), &request.secrets);
    if err.is_timeout() {
        return AppError::with_detail(ErrorCode::E2002, format!("{host}: {text}"));
    }
    if err.is_connect() {
        // TLS を含む接続失敗は、原因の切り分けに CA の確認が要る旨を必ず添える。
        return AppError::with_detail(ErrorCode::E2001, format!("{host}: {text}")).hint(
            "check the host and proxy settings; if TLS verification is the cause, check `tls.caFile` \
             or the system CA store (`SSL_CERT_FILE`)",
        );
    }
    if err.is_redirect() {
        return AppError::with_detail(ErrorCode::E2004, format!("{host}: {text}"));
    }
    let lower = text.to_lowercase();
    if lower.contains("certificate") || lower.contains("tls") {
        return AppError::with_detail(ErrorCode::E2008, host.to_string())
            .hint("point `tls.caFile` at a CA certificate (PEM), or check the system CA store");
    }
    AppError::with_detail(ErrorCode::E2001, format!("{host}: {text}"))
}

/// 応答ヘッダから `Retry-After` を取り出す（reqwest が名を小文字に揃えている）。
fn retry_after_header(headers: &[(String, String)]) -> Option<&str> {
    headers
        .iter()
        .find(|(k, _)| k == "retry-after")
        .map(|(_, v)| v.trim())
        .filter(|v| !v.is_empty())
}

fn http_status_error(
    status: u16,
    headers: &[(String, String)],
    now: chrono::DateTime<chrono::Utc>,
) -> AppError {
    let retry_after = retry_after_header(headers);
    let detail = match retry_after {
        Some(v) => format!("HTTP {status} (Retry-After: {v})"),
        None => format!("HTTP {status}"),
    };
    let wait_ms = retry_after.and_then(|v| crate::core::retry::parse_retry_after_ms(v, now));
    let with_meta = |err: AppError| err.retry_after_ms(wait_ms).status(status);
    match status {
        429 | 403 => with_meta(AppError::with_detail(ErrorCode::E4003, detail)),
        408 => with_meta(AppError::with_detail(ErrorCode::E2002, detail)),
        _ => with_meta(AppError::with_detail(ErrorCode::E2003, detail)),
    }
}

/// `redirectPolicy` に従って 3xx を処理する（仕様 network.md §8.3）。
fn redirect_error(status: u16) -> AppError {
    AppError::with_detail(
        ErrorCode::E2003,
        format!("HTTP {status} (redirects are not allowed: redirectPolicy=never)"),
    )
    .status(status)
    .hint("set `network.redirectPolicy` to `follow`, or use the final URL directly")
}

impl HttpClient for ReqwestClient {
    fn send_blocking(
        &self,
        profile: &NetworkProfile,
        request: &HttpRequest,
    ) -> Result<HttpResponse, AppError> {
        self.send_blocking_with_preflight(profile, request, &mut || Ok(()))
    }

    fn send_blocking_with_preflight(
        &self,
        profile: &NetworkProfile,
        request: &HttpRequest,
        preflight: &mut dyn FnMut() -> Result<(), AppError>,
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
                let wait = retry_wait(profile, last_error.as_ref(), attempt, jitter);
                if wait > 0 {
                    self.clock.sleep(Duration::from_millis(wait));
                }
            }
            // Quota/other caller checks run for every transport attempt, including retries.
            preflight()?;
            self.wait_interval(profile, &host);

            let now = self.clock.now_utc();
            let outcome = self
                .runtime
                .block_on(Self::send_once(&client, profile, request, now));
            match outcome {
                Ok(response) => return Ok(response),
                Err(err) => {
                    // `Retry-After` が `maxBackoffMs` を超える場合は、長時間の
                    // 沈黙を避けるため再試行せずに `E4003` で打ち切る
                    // （仕様 network.md §3）。
                    if profile.retry.respect_retry_after
                        && crate::core::retry::exceeds(
                            err.retry_after_ms,
                            profile.retry.max_backoff_ms,
                        )
                    {
                        return Err(too_long_wait_error(&err));
                    }
                    if !retryable(profile, &err) {
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
            "{}: timeout {}ms / max {} bytes / {} concurrent / interval {}ms",
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
            // 状態番号は詳細文の解析ではなくエラーが持つ値を使う
            // （詳細文には `Retry-After` などが混ざり解析できないため）。
            let status = err.status.unwrap_or(0);
            profile.retry.on_status.contains(&status)
        }
        _ => false,
    }
}

/// 次の再試行までの待機時間（ms）。
///
/// `respectRetryAfter` が真で `Retry-After` を解釈できている場合は、その値を
/// そのまま使う（揺らぎは加えない）。それ以外は指数バックオフと揺らぎ。
fn retry_wait(
    profile: &NetworkProfile,
    last_error: Option<&AppError>,
    attempt: u32,
    jitter: u64,
) -> u64 {
    if profile.retry.respect_retry_after
        && let Some(ms) = last_error.and_then(|e| e.retry_after_ms)
    {
        return ms;
    }
    profile.retry.backoff(attempt, jitter)
}

/// 待機が長すぎるために再試行を打ち切る旨のエラー（仕様 network.md §3）。
fn too_long_wait_error(err: &AppError) -> AppError {
    let wait = err.retry_after_ms.unwrap_or_default();
    AppError::with_detail(
        ErrorCode::E4003,
        format!("gave up because the retry would need to wait {wait} ms"),
    )
    .hint("raise `network.retry.maxBackoffMs`, or try again later")
    .warnings(err.warnings.clone())
}

/// プロファイルの危険設定を警告へ変換する。
pub fn profile_warnings(profile: &NetworkProfile) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    if !profile.tls.verify {
        out.push(Diagnostic::new(
            WarningCode::W1005,
            format!("network '{}' has TLS verification disabled", profile.id),
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
                "network '{}' stores the proxy credentials as a literal value",
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

    fn fixed_now() -> chrono::DateTime<chrono::Utc> {
        use chrono::TimeZone;
        chrono::Utc.with_ymd_and_hms(2026, 11, 14, 0, 0, 0).unwrap()
    }

    #[test]
    fn status_errors_map_to_codes() {
        let now = fixed_now();
        assert_eq!(http_status_error(429, &[], now).code, ErrorCode::E4003);
        assert_eq!(http_status_error(500, &[], now).code, ErrorCode::E2003);
        assert_eq!(http_status_error(408, &[], now).code, ErrorCode::E2002);
    }

    #[test]
    fn status_errors_carry_retry_after() {
        let now = fixed_now();
        let headers = vec![("retry-after".to_string(), "120".to_string())];
        let err = http_status_error(429, &headers, now);
        assert_eq!(err.retry_after_ms, Some(120_000));
    }

    #[test]
    fn unparsable_retry_after_is_ignored() {
        let now = fixed_now();
        let headers = vec![("retry-after".to_string(), "soon".to_string())];
        let err = http_status_error(503, &headers, now);
        assert_eq!(err.retry_after_ms, None);
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
            &AppError::with_detail(ErrorCode::E2003, "HTTP 503").status(503)
        ));
        assert!(!retryable(
            &NetworkProfile::default(),
            &AppError::with_detail(ErrorCode::E2003, "HTTP 404").status(404)
        ));
        // 詳細文に `Retry-After` が混ざっても状態番号で判定する
        assert!(retryable(
            &NetworkProfile::default(),
            &AppError::with_detail(ErrorCode::E2003, "HTTP 503 (Retry-After: 1)").status(503)
        ));
        // 状態番号が無い場合は再試行しない（解析に頼らない）
        assert!(!retryable(
            &NetworkProfile::default(),
            &AppError::with_detail(ErrorCode::E2003, "HTTP 503 (Retry-After: 1)")
        ));
    }
}
