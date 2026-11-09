//! URL の正規化・検証・追跡パラメータ除去。

pub use url::Url;

use crate::core::errors::{AppError, ErrorCode};

/// 既定の追跡パラメータ。設定で上書きできる。
pub const DEFAULT_TRACKING: &[&str] = &[
    "fbclid",
    "gclid",
    "yclid",
    "msclkid",
    "mc_cid",
    "mc_eid",
    "igshid",
    "utm_source",
    "utm_medium",
    "utm_campaign",
    "utm_term",
    "utm_content",
    "utm_id",
];

/// 秘匿情報を含みうるパラメータ名（表示時に伏字化する）。
pub const SENSITIVE_PARAMS: &[&str] = &[
    "token",
    "key",
    "apikey",
    "api_key",
    "sig",
    "signature",
    "password",
    "passwd",
    "secret",
    "auth",
];

fn is_tracking(name: &str, extra: &[String]) -> bool {
    let lower = name.to_ascii_lowercase();
    upper_check(&lower) || extra.iter().any(|p| p.eq_ignore_ascii_case(&lower))
}

fn upper_check(lower: &str) -> bool {
    lower.starts_with("utm_") || DEFAULT_TRACKING.contains(&lower)
}

fn is_sensitive(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    SENSITIVE_PARAMS.iter().any(|p| lower.contains(p))
}

/// 追跡パラメータを除去し、スキーム・ホストを小文字化し、既定ポートを落とす。
pub fn normalize(url: &Url, extra_tracking: &[String]) -> Url {
    let mut out = url.clone();
    let _ = out.set_scheme(&url.scheme().to_ascii_lowercase());
    if let Some(host) = url.host_str() {
        let lowered = host.to_ascii_lowercase();
        if lowered != host {
            let _ = out.set_host(Some(&lowered));
        }
    }
    // 既定ポートを落とす
    if matches!(
        (out.scheme(), out.port()),
        ("http", Some(80)) | ("https", Some(443))
    ) {
        let _ = out.set_port(None);
    }
    // 追跡パラメータの除去
    let kept: Vec<(String, String)> = out
        .query_pairs()
        .filter(|(k, _)| !is_tracking(k, extra_tracking))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    if kept.is_empty() {
        out.set_query(None);
    } else {
        let mut ser = out.query_pairs_mut();
        ser.clear();
        for (k, v) in &kept {
            ser.append_pair(k, v);
        }
        drop(ser);
    }
    out
}

/// 重複判定用のキー（フラグメントを落とす）。
pub fn dedup_key(url: &Url) -> String {
    let mut u = url.clone();
    u.set_fragment(None);
    u.to_string()
}

/// 表示用に秘匿パラメータを伏字化した文字列を返す。
pub fn redact(url: &Url, secrets: &[String]) -> String {
    let mut out = url.clone();
    if url.query().is_some() {
        let pairs: Vec<(String, String)> = url
            .query_pairs()
            .map(|(k, v)| {
                let key = k.into_owned();
                let val = v.into_owned();
                let hidden = is_sensitive(&key)
                    || secrets
                        .iter()
                        .any(|s| !s.is_empty() && val.contains(s.as_str()));
                (key, if hidden { "***".to_string() } else { val })
            })
            .collect();
        let mut ser = out.query_pairs_mut();
        ser.clear();
        for (k, v) in &pairs {
            ser.append_pair(k, v);
        }
    }
    out.to_string()
}

/// ホスト名（`www.` を 1 回だけ除去）。
pub fn host_of(url: &Url) -> Option<String> {
    url.host_str().map(|h| {
        let h = h.to_ascii_lowercase();
        h.strip_prefix("www.").map(|s| s.to_string()).unwrap_or(h)
    })
}

/// `http` / `https` 以外を拒否する。
pub fn validate_scheme(url: &Url) -> Result<(), AppError> {
    match url.scheme() {
        "http" | "https" => Ok(()),
        other => Err(AppError::with_detail(
            ErrorCode::E2005,
            format!("scheme={other}"),
        )),
    }
}

/// 入力文字列を URL として解釈する。
pub fn parse(input: &str) -> Result<Url, AppError> {
    let trimmed = input.trim();
    Url::parse(trimmed)
        .map_err(|e| AppError::with_detail(ErrorCode::E2005, format!("{trimmed}: {e}")))
}

/// `*.example.com` 形式の接尾辞一致。
pub fn suffix_match(host: &str, pattern: &str) -> bool {
    let host = host.to_ascii_lowercase();
    let pattern = pattern.to_ascii_lowercase();
    if pattern == "*" {
        return true;
    }
    match pattern.strip_prefix("*.") {
        Some(rest) => host == rest || host.ends_with(&format!(".{rest}")),
        None => host == pattern,
    }
}

/// 拡張子（小文字、ドット無し）。
pub fn extension(url: &Url) -> Option<String> {
    let path = url.path();
    let last = path.rsplit('/').next()?;
    let ext = last.rsplit_once('.')?.1;
    if ext.is_empty() || ext.len() > 8 {
        return None;
    }
    Some(ext.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_tracking_and_ports() {
        let u = Url::parse("HTTPS://Example.COM:443/a?utm_source=x&q=1#f").unwrap();
        let n = normalize(&u, &[]);
        assert_eq!(n.as_str(), "https://example.com/a?q=1#f");
    }

    #[test]
    fn dedup_ignores_fragment() {
        let a = Url::parse("https://example.com/a#x").unwrap();
        let b = Url::parse("https://example.com/a#y").unwrap();
        assert_eq!(dedup_key(&a), dedup_key(&b));
    }

    #[test]
    fn redacts_sensitive_params() {
        let u = Url::parse("https://example.com/s?q=ok&token=abc").unwrap();
        let out = redact(&u, &[]);
        assert!(out.contains("token=***"));
        assert!(out.contains("q=ok"));
    }

    #[test]
    fn host_strips_www() {
        let u = Url::parse("https://www.example.com/").unwrap();
        assert_eq!(host_of(&u).as_deref(), Some("example.com"));
    }
}
