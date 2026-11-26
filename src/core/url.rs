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

pub fn is_sensitive_param_name(name: &str) -> bool {
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
///
/// クエリの秘匿パラメータ名（`token` 等）、クエリ値内のネスト URL、および
/// パス中の `名前=値` 形式を伏せる。パス全体は置換しない（無関係な語を巻き込むため）。
pub fn redact(url: &Url, secrets: &[String]) -> String {
    redact_depth(url, secrets, 0)
}

const MAX_REDACT_DEPTH: usize = 3;

fn redact_depth(url: &Url, secrets: &[String], depth: usize) -> String {
    let mut out = url.clone();
    let mut authority_fallback = false;
    // Never expose URL userinfo, even when the credential does not use a recognized query key.
    if !url.username().is_empty() || url.password().is_some() {
        if out.set_username("***").is_err() {
            authority_fallback = true;
        }
        if url.password().is_some() && out.set_password(Some("***")).is_err() {
            authority_fallback = true;
        }
    }
    if let Some(path) = url.path().strip_prefix('/') {
        let redacted = redact_path_segments(path);
        if redacted != path {
            out.set_path(&format!("/{redacted}"));
        }
    }
    if let Some(raw_query) = url.query() {
        if raw_query.contains("{{") && raw_query.contains("}}") {
            let redacted_q = redact_raw_query(raw_query, secrets, depth);
            if redacted_q != raw_query {
                out.set_query(Some(&redacted_q));
            }
        } else {
            let mut hidden_any = false;
            let pairs: Vec<(String, String)> = url
                .query_pairs()
                .map(|(k, v)| {
                    let key = k.into_owned();
                    let val = v.into_owned();
                    if is_sensitive_param_name(&key)
                        || secrets
                            .iter()
                            .any(|s| !s.is_empty() && val.contains(s.as_str()))
                    {
                        hidden_any = true;
                        return (key, "***".to_string());
                    }
                    if depth < MAX_REDACT_DEPTH
                        && let Some(redacted_nested) =
                            redact_nested_query_url(&val, secrets, depth + 1)
                    {
                        hidden_any = true;
                        return (key, redacted_nested);
                    }
                    (key, val)
                })
                .collect();
            // クエリに伏せる対象が無いときは、パスの置換結果だけを返す
            // （再エンコードで見た目を変えない）。
            if hidden_any {
                let mut ser = out.query_pairs_mut();
                ser.clear();
                for (k, v) in &pairs {
                    ser.append_pair(k, v);
                }
            }
        }
    }
    let mut text = out.to_string();
    if authority_fallback || has_unmasked_userinfo(&text) {
        text = redact_raw_url_token(&text, secrets, depth);
    }
    // 環境変数から解決した秘匿値は、クエリ名に依らず文字列一致で伏せる
    // （パスや断片に埋め込まれた場合もここで拾う）。
    if secrets
        .iter()
        .any(|s| !s.is_empty() && text.contains(s.as_str()))
    {
        crate::core::secret::redact_text(&text, secrets)
    } else {
        text
    }
}

/// 文字列としての URL（構文エラー・未展開テンプレート・任意スキームを含む）を伏字化する。
pub fn redact_url_str(raw: &str, secrets: &[String]) -> String {
    redact_url_str_depth(raw, secrets, 0)
}

fn redact_url_str_depth(raw: &str, secrets: &[String], depth: usize) -> String {
    let trimmed = raw.trim();
    if let Ok(parsed) = Url::parse(trimmed) {
        return redact_depth(&parsed, secrets, depth);
    }
    redact_raw_url_token(trimmed, secrets, depth)
}

fn redact_nested_query_url(val: &str, secrets: &[String], depth: usize) -> Option<String> {
    if val.contains("://") {
        let redacted = redact_url_str_depth(val, secrets, depth);
        if redacted != val {
            return Some(redacted);
        }
    }
    let decoded = percent_decode(val);
    if decoded != val && decoded.contains("://") {
        let redacted = redact_url_str_depth(&decoded, secrets, depth);
        if redacted != decoded {
            return Some(redacted);
        }
    }
    None
}

fn has_unmasked_userinfo(raw: &str) -> bool {
    let Some((_, after_scheme)) = raw.split_once("://") else {
        return false;
    };
    let authority_end = after_scheme
        .find(['/', '?', '#'])
        .unwrap_or(after_scheme.len());
    let authority = &after_scheme[..authority_end];
    let Some((userinfo, _)) = authority.rsplit_once('@') else {
        return false;
    };
    !userinfo.is_empty() && userinfo != "***" && userinfo != "***:***"
}

pub(crate) fn is_valid_scheme(scheme: &str) -> bool {
    let mut bytes = scheme.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    first.is_ascii_alphabetic()
        && bytes.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
}

fn redact_raw_query(query: &str, secrets: &[String], depth: usize) -> String {
    query
        .split('&')
        .map(|seg| {
            let Some((raw_k, raw_v)) = seg.split_once('=') else {
                return crate::core::secret::redact_text(seg, secrets);
            };
            let decoded_k = percent_decode(raw_k);
            let decoded_v = percent_decode(raw_v);
            if is_sensitive_param_name(&decoded_k)
                || secrets.iter().any(|s| {
                    !s.is_empty() && (raw_v.contains(s.as_str()) || decoded_v.contains(s.as_str()))
                })
            {
                return format!("{raw_k}=***");
            }
            if depth < MAX_REDACT_DEPTH {
                if raw_v.contains("://") {
                    let red = redact_url_str_depth(raw_v, secrets, depth + 1);
                    if red != raw_v {
                        return format!("{raw_k}={red}");
                    }
                } else if decoded_v.contains("://") {
                    let red = redact_url_str_depth(&decoded_v, secrets, depth + 1);
                    if red != decoded_v {
                        let enc: String =
                            url::form_urlencoded::byte_serialize(red.as_bytes()).collect();
                        return format!("{raw_k}={enc}");
                    }
                }
            }
            seg.to_string()
        })
        .collect::<Vec<_>>()
        .join("&")
}

fn redact_raw_url_token(raw: &str, secrets: &[String], depth: usize) -> String {
    let Some((scheme, after_scheme)) = raw.split_once("://") else {
        return crate::core::secret::redact_text(raw, secrets);
    };
    if !is_valid_scheme(scheme) {
        return crate::core::secret::redact_text(raw, secrets);
    }
    let authority_end = after_scheme
        .find(['/', '?', '#'])
        .unwrap_or(after_scheme.len());
    let authority = &after_scheme[..authority_end];
    let tail = &after_scheme[authority_end..];

    let masked_authority = match authority.rsplit_once('@') {
        Some((userinfo, hostport)) if !userinfo.is_empty() => {
            if userinfo.contains(':') {
                format!("***:***@{hostport}")
            } else {
                format!("***@{hostport}")
            }
        }
        _ => authority.to_string(),
    };

    let (before_frag, frag_opt) = match tail.split_once('#') {
        Some((b, f)) => (b, Some(f)),
        None => (tail, None),
    };
    let (path_part, query_opt) = match before_frag.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (before_frag, None),
    };

    let masked_path = match path_part.strip_prefix('/') {
        Some(rest) => format!("/{}", redact_path_segments(rest)),
        None => path_part.to_string(),
    };
    let masked_query = match query_opt {
        Some(q) => format!("?{}", redact_raw_query(q, secrets, depth)),
        None => String::new(),
    };
    let masked_frag = match frag_opt {
        Some(f) => format!("#{}", crate::core::secret::redact_text(f, secrets)),
        None => String::new(),
    };

    let out = format!("{scheme}://{masked_authority}{masked_path}{masked_query}{masked_frag}");
    if secrets
        .iter()
        .any(|s| !s.is_empty() && out.contains(s.as_str()))
    {
        crate::core::secret::redact_text(&out, secrets)
    } else {
        out
    }
}

/// パス中の `名前=値` を伏字化する（`/token=abc/x` → `/token=***/x`）。
fn redact_path_segments(path: &str) -> String {
    let mut changed = false;
    let joined = path
        .split('/')
        .map(|seg| match seg.split_once('=') {
            Some((name, value)) if !value.is_empty() && is_sensitive_param_name(name) => {
                changed = true;
                format!("{name}=***")
            }
            _ => seg.to_string(),
        })
        .collect::<Vec<_>>()
        .join("/");
    if changed { joined } else { path.to_string() }
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
    Url::parse(trimmed).map_err(|e| {
        let shown = redact_url_str(trimmed, &[]);
        AppError::with_detail(ErrorCode::E2005, format!("{shown}: {e}"))
    })
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
    fn redacts_url_userinfo() {
        let url = Url::parse("https://alice:supersecret@example.com/a").unwrap();
        assert_eq!(redact(&url, &[]), "https://***:***@example.com/a");

        let password_only = Url::parse("https://:supersecret@example.com/a").unwrap();
        assert_eq!(redact(&password_only, &[]), "https://***:***@example.com/a");
    }

    #[test]
    fn redacts_nested_redirect_userinfo_and_malformed_url_strings() {
        let relay = Url::parse(
            "http://127.0.0.1:8080/l/?uddg=http%3A%2F%2Fwrapuser%3Awrappass%40127.0.0.1%3A8080%2Fdoc%3Ftoken%3DWRAPTOK%26normal%3Dkeep",
        )
        .unwrap();
        let shown = redact(&relay, &[]);
        assert!(!shown.contains("wrapuser"), "{shown}");
        assert!(!shown.contains("wrappass"), "{shown}");
        assert!(!shown.contains("WRAPTOK"), "{shown}");
        assert!(shown.contains("normal%3Dkeep"), "{shown}");

        let malformed = redact_url_str(
            "http://baduser:badpass@127.0.0.1:99999/doc?token=BADTOK&q={{query}}",
            &[],
        );
        assert_eq!(
            malformed,
            "http://***:***@127.0.0.1:99999/doc?token=***&q={{query}}"
        );

        let parse_err = parse("http://baduser:badpass@127.0.0.1:99999/doc").unwrap_err();
        let detail = parse_err.detail.unwrap_or_default();
        assert!(!detail.contains("baduser"), "{detail}");
        assert!(!detail.contains("badpass"), "{detail}");
        assert!(detail.contains("***:***@127.0.0.1:99999"), "{detail}");
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
