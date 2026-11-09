//! 文書取得のポート。

use url::Url;

use crate::core::config::NetworkProfile;
use crate::core::convert::ExtractMode;
use crate::core::errors::AppError;
use crate::core::model::DocumentMeta;
use crate::ports::http::HttpResponse;

/// 取得のオプション。
#[derive(Debug, Clone)]
pub struct FetchOptions {
    pub refresh: bool,
    pub no_cache: bool,
    pub extract_mode: ExtractMode,
    pub ttl_hours: u64,
    pub profile: NetworkProfile,
    pub max_chars: Option<usize>,
    /// 本文フィルタ（設定の `filters.content`）。
    pub content_filters: Option<crate::core::config::filter::ContentFilters>,
    /// 追跡パラメータの一覧。
    pub tracking: Vec<String>,
}

impl FetchOptions {
    pub fn content_filters(&self) -> Option<&crate::core::config::filter::ContentFilters> {
        self.content_filters.as_ref()
    }
}

impl Default for FetchOptions {
    fn default() -> Self {
        Self {
            refresh: false,
            no_cache: false,
            extract_mode: ExtractMode::default(),
            ttl_hours: 24,
            profile: NetworkProfile::default(),
            max_chars: None,
            content_filters: None,
            tracking: Vec::new(),
        }
    }
}

/// HTTP 応答（ヘッダ・本文）。
#[derive(Debug, Clone)]
pub struct RawDocument {
    pub response: HttpResponse,
    pub body: String,
    pub charset: Option<String>,
    pub cacheable: bool,
}

/// 変換済み文書。
#[derive(Debug, Clone)]
pub struct FetchedDocument {
    pub meta: DocumentMeta,
    pub markdown: String,
    pub warnings: Vec<String>,
    pub cached: bool,
}

/// 文書取得（HTTP + 変換 + キャッシュ）。
pub trait Fetcher: Send + Sync {
    fn fetch(&self, url: &Url, options: &FetchOptions) -> Result<FetchedDocument, AppError>;
}

/// 文字符号化の判定（Content-Type → meta charset → UTF-8）。
pub fn detect_charset(
    content_type: Option<&str>,
    body: &[u8],
    default: Option<&str>,
) -> (String, bool) {
    let mut candidates: Vec<String> = Vec::new();
    if let Some(header) = content_type {
        for part in header.split(';').skip(1) {
            let part = part.trim();
            if let Some(value) = part.strip_prefix("charset=") {
                candidates.push(value.trim_matches('"').to_string());
            }
        }
    }
    if let Some(d) = default {
        candidates.push(d.to_string());
    }
    // meta charset を先頭 4096 バイトから探す
    let head = String::from_utf8_lossy(&body[..body.len().min(4096)]).to_lowercase();
    for probe in ["charset=\"", "charset='", "charset="] {
        if let Some(pos) = head.find(probe) {
            let rest = &head[pos + probe.len()..];
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
                .unwrap_or(rest.len());
            let label = rest[..end].trim_matches('"').trim_matches('\'');
            if !label.is_empty() {
                candidates.push(label.to_string());
            }
            break;
        }
    }
    for label in candidates {
        if let Some(encoding) = encoding_rs::Encoding::for_label(label.trim().as_bytes()) {
            return (encoding.name().to_ascii_lowercase(), false);
        }
    }
    ("utf-8".to_string(), true)
}

/// バイト列を文字列へ復号する。
pub fn decode_body(body: &[u8], encoding: &str) -> (String, bool) {
    let enc = encoding_rs::Encoding::for_label(encoding.as_bytes()).unwrap_or(encoding_rs::UTF_8);
    let (text, _, had_errors) = enc.decode(body);
    (text.into_owned(), had_errors)
}

/// 本文が HTML らしいか（Content-Type が不明なときの判定）。
pub fn looks_like_html(body: &[u8]) -> bool {
    let head = String::from_utf8_lossy(&body[..body.len().min(1024)]).to_lowercase();
    head.contains("<html")
        || head.contains("<!doctype html")
        || head.contains("<body")
        || head.contains("<meta")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charset_from_header_then_meta() {
        let (name, _) = detect_charset(Some("text/html; charset=EUC-JP"), b"<html>", None);
        assert_eq!(name, "euc-jp");
        let (name, _) = detect_charset(
            Some("text/html"),
            br#"<html><head><meta charset="shift_jis"></head>"#,
            None,
        );
        assert_eq!(name, "shift_jis");
        let (name, fallback) = detect_charset(Some("text/html"), b"<html>", None);
        assert_eq!(name, "utf-8");
        assert!(fallback);
    }

    #[test]
    fn decodes_shift_jis() {
        let (bytes, _, _) = encoding_rs::SHIFT_JIS.encode("日本語");
        let (text, errors) = decode_body(&bytes, "shift_jis");
        assert_eq!(text, "日本語");
        assert!(!errors);
    }

    #[test]
    fn detects_html() {
        assert!(looks_like_html(b"<!DOCTYPE html><html>"));
        assert!(!looks_like_html(b"%PDF-1.7"));
    }
}
