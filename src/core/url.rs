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

/// リダイレクト URL のクエリに含まれる実 URL を取り出す（設定に依存しない既定キー）。
///
/// 値はパーセントエンコードされている場合があるため、復号してから `http` /
/// `https` で始まるものを採る。入れ子（リダイレクトがリダイレクトを包む）は
/// 2 段まで辿る。
pub fn target_of_redirect(url: &Url) -> Option<String> {
    const KEYS: &[&str] = &[
        "uddg",
        "url",
        "u",
        "q",
        "target",
        "redirect",
        "redirect_url",
        "dest",
        "destination",
        "to",
        "link",
        "href",
    ];
    const MAX_DEPTH: usize = 2;
    fn target(url: &Url) -> Option<String> {
        for (key, value) in url.query_pairs() {
            if !KEYS.iter().any(|k| key.eq_ignore_ascii_case(k)) {
                continue;
            }
            let raw = value.trim().to_string();
            for candidate in [raw.clone(), percent_decode(&raw)] {
                let candidate = candidate.trim();
                if candidate.starts_with("http://") || candidate.starts_with("https://") {
                    return Some(candidate.to_string());
                }
                if let Ok(inner) = Url::parse(candidate)
                    && let Some(found) = target(&inner)
                {
                    return Some(found);
                }
            }
        }
        None
    }

    let mut current = url.clone();
    let mut found = None;
    for _ in 0..MAX_DEPTH {
        let Some(next) = target(&current) else { break };
        let Ok(parsed) = Url::parse(&next) else { break };
        if parsed == current {
            break;
        }
        current = parsed;
        found = Some(current.to_string());
    }
    found
}

/// `%XX` を復号する（不正な並びはそのまま残す）。
pub fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(&input[i + 1..i + 3], 16)
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// HTML に書かれたリダイレクト先を取り出す（`<meta http-equiv="refresh">` と
/// `location.href = "..."` / `location.replace("...")`）。
///
/// エンジンのリダイレクトは HTTP の 302 ではなく、HTML の中継ページで
/// 返ることがある。その場合 HTTP 層では追えないため、本文から拾って
/// たどり直す必要がある。
pub fn html_redirect_target(body: &str, base: &Url) -> Option<Url> {
    let lower = body.to_ascii_lowercase();

    // <meta http-equiv="refresh" content="0; url=/next">
    if let Some(pos) = lower.find("http-equiv")
        && lower[pos..].starts_with("http-equiv")
        && lower[pos..].find("refresh").is_some_and(|d| d < 200)
        && let Some(content_pos) = lower[pos..].find("content").map(|i| pos + i)
    {
        let slice = &body[content_pos..];
        let quote_start = slice.find(['"', '\'']).map(|i| content_pos + i)?;
        let quote = body[quote_start..].chars().next()?;
        let rest = &body[quote_start + 1..];
        let quote_end = rest.find(quote).map(|i| quote_start + 1 + i)?;
        let content = &body[quote_start + 1..quote_end];
        let target = content
            .split("url=")
            .nth(1)
            .or_else(|| content.split("URL=").nth(1))?;
        return base.join(target.trim().trim_matches(['"', '\'', ' '])).ok();
    }

    // location.href = "..." / location.replace("...") / window.location = "..."
    for marker in ["location.href", "location.replace", "location.assign"] {
        if let Some(pos) = lower.find(marker) {
            let rest = &body[pos + marker.len()..];
            let rest = rest.trim_start().trim_start_matches('(').trim_start();
            let rest = rest.trim_start_matches('=').trim_start();
            let quote = rest.chars().next()?;
            if quote != '"' && quote != '\'' {
                continue;
            }
            let inner = &rest[1..];
            let end = inner.find(quote)?;
            return base.join(inner[..end].trim()).ok();
        }
    }
    None
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
    fn finds_target_behind_redirect() {
        let u =
            Url::parse("https://relay.example/l/?uddg=https%3A%2F%2Fexample.jp%2Fa%3Fx%3D1&rut=z")
                .unwrap();
        assert_eq!(
            target_of_redirect(&u).as_deref(),
            Some("https://example.jp/a?x=1")
        );
        let plain = Url::parse("https://example.jp/a").unwrap();
        assert_eq!(target_of_redirect(&plain), None);
    }

    #[test]
    fn reads_redirect_from_html() {
        let base = Url::parse("https://relay.example/out?x=1").unwrap();
        let meta =
            r#"<html><head><meta http-equiv="refresh" content="0; url=/real/page"></head></html>"#;
        assert_eq!(
            html_redirect_target(meta, &base)
                .map(|u| u.to_string())
                .as_deref(),
            Some("https://relay.example/real/page")
        );
        let js = r#"<script>window.location.replace("https://example.jp/a");</script>"#;
        assert_eq!(
            html_redirect_target(js, &base)
                .map(|u| u.to_string())
                .as_deref(),
            Some("https://example.jp/a")
        );
        assert_eq!(html_redirect_target("<p>no redirect</p>", &base), None);
    }

    #[test]
    fn host_strips_www() {
        let u = Url::parse("https://www.example.com/").unwrap();
        assert_eq!(host_of(&u).as_deref(), Some("example.com"));
    }
}
