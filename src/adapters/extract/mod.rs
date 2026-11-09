//! HTML の取得と変換（`Fetcher` の実装）。

use url::Url;

use crate::core::config::NetworkProfile;
use crate::core::convert::{self, Converted, ExtractMode};
use crate::core::errors::{AppError, ErrorCode};
use crate::core::markdown;
use crate::core::model::DocumentMeta;
use crate::core::outline;
use crate::core::session::cache_key;
use crate::core::url as core_url;
use crate::ports::fetch::{
    FetchOptions, FetchedDocument, decode_body, detect_charset, looks_like_html,
};
use crate::ports::http::{HttpRequest, HttpResponse};
use crate::ports::state::DocumentStore;
use crate::ports::{Clock, HttpClient, Reporter};

/// HTTP で取得し、変換して返す実装。
pub struct HttpFetcher {
    pub client: std::sync::Arc<dyn HttpClient>,
    pub store: Option<std::sync::Arc<dyn DocumentStore>>,
    pub clock: std::sync::Arc<dyn Clock>,
    pub reporter: std::sync::Arc<dyn Reporter>,
}

impl HttpFetcher {
    pub fn new(
        client: std::sync::Arc<dyn HttpClient>,
        store: Option<std::sync::Arc<dyn DocumentStore>>,
        clock: std::sync::Arc<dyn Clock>,
        reporter: std::sync::Arc<dyn Reporter>,
    ) -> Self {
        Self {
            client,
            store,
            clock,
            reporter,
        }
    }

    /// 取得（キャッシュは使わない）。
    fn fetch_fresh(&self, url: &Url, options: &FetchOptions) -> Result<FetchedDocument, AppError> {
        core_url::validate_scheme(url)?;
        let request = HttpRequest::get(url.clone());
        let response = self.client.send_blocking(&options.profile, &request)?;

        if !is_html_response(&response) {
            return Err(AppError::with_detail(
                ErrorCode::E3002,
                response
                    .content_type()
                    .unwrap_or("(Content-Type なし)")
                    .to_string(),
            ));
        }

        let (charset, guessed) = detect_charset(
            response.content_type(),
            &response.body,
            options.profile.charset.as_deref(),
        );
        let (body, had_errors) = decode_body(&response.body, &charset);
        if had_errors || guessed {
            self.reporter.warn(&crate::core::errors::Diagnostic::new(
                crate::core::errors::WarningCode::W3003,
                format!(
                    "文字コードを {} と推定した（置換文字あり: {had_errors}）",
                    charset
                ),
            ));
        }

        let mut converted: Converted = convert::convert(
            &body,
            &response.final_url,
            options.extract_mode,
            options.content_filters(),
        )?;

        let now = self.clock.now_utc();
        let meta =
            convert::build_meta(&converted, url, &response.final_url, now, options.ttl_hours);
        if response.truncated {
            converted
                .warnings
                .push("応答サイズの上限で打ち切った".to_string());
        }
        let mut meta = meta;
        meta.warnings = converted.warnings.clone();
        meta.cacheable = meta.cacheable && options.profile.robots.respect && cacheable(&response);

        if let Some(store) = &self.store
            && !options.no_cache
            && meta.cacheable
        {
            let _ = store.put(&meta, &converted.markdown);
        }

        Ok(FetchedDocument {
            meta,
            markdown: converted.markdown,
            warnings: converted.warnings,
            cached: false,
        })
    }

    /// キャッシュから読む（有効期限内のみ）。
    fn load_cached(&self, url: &Url, ttl_hours: u64) -> Option<FetchedDocument> {
        let store = self.store.as_ref()?;
        let (meta, markdown) = store.get(url).ok()??;
        let expires = chrono::DateTime::parse_from_rfc3339(&meta.expires_at).ok()?;
        if self.clock.now_utc() > expires.with_timezone(&chrono::Utc) {
            return None;
        }
        if ttl_hours == 0 {
            return None;
        }
        Some(FetchedDocument {
            warnings: meta.warnings.clone(),
            meta,
            markdown,
            cached: true,
        })
    }
}

fn is_html_response(response: &HttpResponse) -> bool {
    match response.content_type() {
        Some(ct) => {
            let ct = ct.to_ascii_lowercase();
            ct.contains("text/html")
                || ct.contains("application/xhtml")
                || ct.contains("text/plain")
        }
        None => looks_like_html(&response.body),
    }
}

fn cacheable(response: &HttpResponse) -> bool {
    response
        .header("cache-control")
        .map(|v| !v.to_ascii_lowercase().contains("no-store"))
        .unwrap_or(true)
}

impl crate::ports::fetch::Fetcher for HttpFetcher {
    fn fetch(&self, url: &Url, options: &FetchOptions) -> Result<FetchedDocument, AppError> {
        if !options.refresh
            && !options.no_cache
            && let Some(cached) = self.load_cached(url, options.ttl_hours)
        {
            return Ok(cached);
        }
        self.fetch_fresh(url, options)
    }
}

/// 変換のみを行う（テスト・再変換用）。
pub fn convert_only(
    html: &str,
    final_url: &Url,
    mode: ExtractMode,
    ttl_hours: u64,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<(DocumentMeta, String, Vec<crate::core::model::Heading>), AppError> {
    let converted = convert::convert(html, final_url, mode, None)?;
    let meta = convert::build_meta(&converted, final_url, final_url, now, ttl_hours);
    let headings = outline::extract_headings(&converted.markdown);
    Ok((meta, converted.markdown, headings))
}

/// 文書キャッシュ用のキー。
pub fn document_key(url: &Url) -> String {
    cache_key(url.as_str())
}

/// 見出し索引を作る（キャッシュとセッションで共用）。
pub fn heading_index(markdown_text: &str) -> Vec<crate::core::model::Heading> {
    outline::extract_headings(markdown_text)
}

/// Markdown の相対リンクを絶対化する（保存前の補正）。
pub fn absolutize(markdown_text: &str, base: &Url) -> String {
    markdown::absolutize(markdown_text, base)
}

/// 変換方式を文字列から解決する。
pub fn resolve_mode(name: &str) -> Option<ExtractMode> {
    ExtractMode::parse(name)
}

/// 既定のネットワークプロファイルから取得オプションを作る。
pub fn options_from(profile: &NetworkProfile, ttl_hours: u64) -> FetchOptions {
    FetchOptions {
        profile: profile.clone(),
        ttl_hours,
        ..FetchOptions::default()
    }
}
