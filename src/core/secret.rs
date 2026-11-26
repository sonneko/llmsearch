//! 秘匿値（API キー等）。表示時は伏字化する。

/// 秘匿値。`Debug` では中身を出さない。
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// 実際の値。ログ・表示に使ってはならない。
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.trim().is_empty()
    }

    pub fn masked(&self) -> String {
        if self.0.is_empty() {
            "(not set)".to_string()
        } else {
            "****".to_string()
        }
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Secret(<redacted>)")
    }
}

/// 環境変数の解決結果を保持するコンテナ。
#[derive(Debug, Clone, Default)]
pub struct Secrets {
    values: std::collections::BTreeMap<String, Secret>,
}

impl Secrets {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.values.insert(name.into(), Secret::new(value));
    }

    pub fn get(&self, name: &str) -> Option<&Secret> {
        self.values.get(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.values.contains_key(name)
    }

    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.values.keys()
    }

    pub fn value_of(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(|s| s.expose())
    }
}

/// 伏字化に使う文字列一覧（表示時の置換用）。
pub fn redaction_list(secrets: &Secrets) -> Vec<String> {
    secrets
        .names()
        .filter_map(|name| secrets.value_of(name))
        .filter(|value| value.chars().count() >= 4)
        .map(str::to_string)
        .collect()
}

/// 環境 secret に加え、エンジン定義の固定 `TmplValue::Explicit` も伏字化対象にする。
pub fn redaction_list_for_engines(
    secrets: &Secrets,
    engines: &crate::core::engine::EngineRegistry,
) -> Vec<String> {
    let mut values = redaction_list(secrets);
    for definition in engines.all() {
        if let Ok(value) = serde_json::to_value(definition) {
            collect_explicit_values(&value, &mut values);
        }
    }
    values
}

/// 文字列に含まれる既知の秘匿値を `***` に置換する。
///
/// 短い値は誤爆を避けるため置換しない（`redaction_list` と同じ基準）。
pub fn redact_text(text: &str, secrets: &[String]) -> String {
    let mut ranges = Vec::new();
    for secret in secrets.iter().filter(|value| value.chars().count() >= 4) {
        let mut search_from = 0;
        loop {
            let Some(offset) = text
                .get(search_from..)
                .and_then(|remaining| remaining.find(secret.as_str()))
            else {
                break;
            };
            let start = search_from + offset;
            let end = start + secret.len();
            ranges.push((start, end));
            let Some(first_char) = text
                .get(start..)
                .and_then(|remaining| remaining.chars().next())
            else {
                break;
            };
            search_from = start + first_char.len_utf8();
        }
    }
    if ranges.is_empty() {
        return text.to_string();
    }

    ranges.sort_unstable();
    let mut merged = Vec::with_capacity(ranges.len());
    for (start, end) in ranges {
        if let Some((_, last_end)) = merged.last_mut()
            && start <= *last_end
        {
            *last_end = (*last_end).max(end);
        } else {
            merged.push((start, end));
        }
    }

    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for (start, end) in merged {
        out.push_str(&text[cursor..start]);
        out.push_str("***");
        cursor = end;
    }
    out.push_str(&text[cursor..]);
    out
}

/// 表示用の文字列を伏字化する。既知の値に加え、文中に埋め込まれた URL の
/// userinfo と秘匿 query/path パラメータも `core::url::redact` で処理する。
pub fn redact_display_text(text: &str, secrets: &[String]) -> String {
    let text = redact_text(text, secrets);
    let lower = text.to_ascii_lowercase();
    let mut cursor = 0;
    let mut output = String::with_capacity(text.len());

    while let Some(start) = find_http_url(&lower[cursor..]) {
        let start = cursor + start;
        output.push_str(&text[cursor..start]);
        let rest = &text[start..];
        let end = rest
            .char_indices()
            .find(|(_, ch)| {
                ch.is_whitespace() || matches!(ch, '<' | '>' | '"' | '\'' | '`' | '\\' | '|')
            })
            .map(|(index, _)| index)
            .unwrap_or(rest.len());
        let matched = &rest[..end];
        // `]` is a valid closing delimiter for an IPv6 literal; trimming it would
        // make the URL unparsable and could leave embedded userinfo exposed.
        let candidate = matched.trim_end_matches(|ch: char| {
            matches!(ch, '.' | ',' | ';' | ':' | '!' | '?' | ')' | '}')
        });
        let suffix = &matched[candidate.len()..];

        match url::Url::parse(candidate) {
            Ok(parsed) if matches!(parsed.scheme(), "http" | "https") => {
                output.push_str(&crate::core::url::redact(&parsed, secrets));
                output.push_str(suffix);
            }
            _ => output.push_str(matched),
        }
        cursor = start + end;
    }
    output.push_str(&text[cursor..]);
    output
}

fn find_http_url(text: &str) -> Option<usize> {
    let https = text.find("https://");
    let http = text.find("http://");
    match (https, http) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(index), None) | (None, Some(index)) => Some(index),
        (None, None) => None,
    }
}

/// URL エンコードした form body を認識して、秘匿パラメータと既知の値を伏せる。
/// JSON body は秘匿フィールド名を再帰的に検査する。その他の body には既知値と
/// 文中 URL の伏字化を適用する。
pub fn redact_body(body: &str, secrets: &[String]) -> String {
    let text = redact_display_text(body, secrets);
    if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&text) {
        let before = value.clone();
        redact_json_output(&mut value, secrets);
        return if value == before {
            text
        } else {
            serde_json::to_string(&value).unwrap_or(text)
        };
    }

    let trimmed = text.trim();
    if !trimmed.is_empty()
        && trimmed.contains('=')
        && trimmed.split('&').all(|part| part.contains('='))
    {
        let pairs: Vec<(String, String)> = url::form_urlencoded::parse(trimmed.as_bytes())
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect();
        let mut changed = false;
        let safe_pairs: Vec<(String, String)> = pairs
            .into_iter()
            .map(|(name, value)| {
                let secret_value = secrets
                    .iter()
                    .any(|secret| secret.chars().count() >= 4 && value.contains(secret.as_str()));
                let sensitive_name = crate::core::url::is_sensitive_param_name(&name);
                if (secret_value || sensitive_name) && value != "***" {
                    changed = true;
                    (name, "***".to_string())
                } else {
                    (name, value)
                }
            })
            .collect();
        if changed {
            let mut serializer = url::form_urlencoded::Serializer::new(String::new());
            for (name, value) in safe_pairs {
                serializer.append_pair(&name, &value);
            }
            return serializer.finish();
        }
    }
    text
}

/// JSON 定義内の固定 `{"value": ...}` を伏せるため、値を redaction list に加える。
/// 短い値は誤爆防止のため対象外にし、既存値の重複も除く。
pub fn collect_explicit_values(value: &serde_json::Value, secrets: &mut Vec<String>) {
    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                collect_explicit_values(item, secrets);
            }
        }
        serde_json::Value::Object(map) => {
            if !map.contains_key("value_env")
                && let Some(value) = map.get("value").and_then(serde_json::Value::as_str)
                && value.chars().count() >= 4
                && !secrets.iter().any(|secret| secret == value)
            {
                secrets.push(value.to_string());
            }
            for item in map.values() {
                collect_explicit_values(item, secrets);
            }
        }
        _ => {}
    }
}

/// JSON 定義を表示する直前に伏字化する。
pub fn redact_json(value: &mut serde_json::Value, secrets: &Secrets) {
    let mut list = redaction_list(secrets);
    collect_explicit_values(value, &mut list);
    redact_json_with(value, &list);
}

/// JSON イベントなどの汎用出力を伏字化する。`value` キーを一律に伏せず、
/// 認証フィールド名・header 名・URL・body に限定して処理する。
pub fn redact_json_output(value: &mut serde_json::Value, secrets: &[String]) {
    redact_json_output_inner(value, secrets);
}

fn redact_json_output_inner(value: &mut serde_json::Value, secrets: &[String]) {
    match value {
        serde_json::Value::String(text) => *text = redact_display_text(text, secrets),
        serde_json::Value::Array(items) => {
            for item in items.iter_mut() {
                redact_json_output_inner(item, secrets);
            }
        }
        serde_json::Value::Object(map) => {
            for (key, item) in map.iter_mut() {
                if key == "value_env" {
                    continue;
                }
                if key.eq_ignore_ascii_case("headers") {
                    redact_header_collection(item, secrets);
                } else if key.eq_ignore_ascii_case("body") {
                    if let serde_json::Value::String(body) = item {
                        *body = redact_body(body, secrets);
                    } else {
                        redact_json_output_inner(item, secrets);
                    }
                } else if is_sensitive_json_key(key) && !is_public_api_key_state(key, item) {
                    redact_sensitive_value(item);
                } else {
                    redact_json_output_inner(item, secrets);
                }
            }
        }
        _ => {}
    }
}

fn redact_header_collection(value: &mut serde_json::Value, secrets: &[String]) {
    match value {
        serde_json::Value::Object(headers) => {
            for (name, value) in headers.iter_mut() {
                if is_sensitive_header_name(name) {
                    redact_sensitive_value(value);
                } else {
                    redact_json_output_inner(value, secrets);
                }
            }
        }
        serde_json::Value::Array(rows) => {
            for row in rows.iter_mut() {
                match row {
                    serde_json::Value::Array(pair) if pair.len() >= 2 => {
                        let name = pair[0].as_str().unwrap_or_default();
                        if is_sensitive_header_name(name) {
                            redact_sensitive_value(&mut pair[1]);
                        } else {
                            redact_json_output_inner(&mut pair[1], secrets);
                        }
                    }
                    serde_json::Value::Object(headers) => {
                        for (name, value) in headers.iter_mut() {
                            if is_sensitive_header_name(name) {
                                redact_sensitive_value(value);
                            } else {
                                redact_json_output_inner(value, secrets);
                            }
                        }
                    }
                    _ => redact_json_output_inner(row, secrets),
                }
            }
        }
        _ => redact_json_output_inner(value, secrets),
    }
}

/// 文字列一覧を指定してエンジン等の JSON 定義を伏字化する。
/// `value_env` は環境変数名なので残す。`{"value": ...}` は固定値なので隠す。
pub fn redact_json_with(value: &mut serde_json::Value, secrets: &[String]) {
    redact_json_definition_inner(value, secrets);
}

fn redact_json_definition_inner(value: &mut serde_json::Value, secrets: &[String]) {
    match value {
        serde_json::Value::String(text) => *text = redact_display_text(text, secrets),
        serde_json::Value::Array(items) => {
            for item in items.iter_mut() {
                redact_json_definition_inner(item, secrets);
            }
        }
        serde_json::Value::Object(map) => {
            if !map.contains_key("value_env")
                && let Some(serde_json::Value::String(text)) = map.get_mut("value")
            {
                *text = "***".to_string();
            }
            for (key, item) in map.iter_mut() {
                if key == "value_env" {
                    continue;
                }
                if key.eq_ignore_ascii_case("headers") {
                    redact_named_definition_entries(item, secrets, is_sensitive_header_name);
                } else if key.eq_ignore_ascii_case("query") {
                    redact_named_definition_entries(
                        item,
                        secrets,
                        crate::core::url::is_sensitive_param_name,
                    );
                } else if key.eq_ignore_ascii_case("body") {
                    if let serde_json::Value::String(body) = item {
                        *body = redact_body(body, secrets);
                    } else {
                        redact_json_definition_inner(item, secrets);
                    }
                } else if is_sensitive_json_key(key) {
                    redact_sensitive_value(item);
                } else {
                    redact_json_definition_inner(item, secrets);
                }
            }
        }
        _ => {}
    }
}

fn redact_named_definition_entries<F>(
    value: &mut serde_json::Value,
    secrets: &[String],
    is_sensitive: F,
) where
    F: Fn(&str) -> bool + Copy,
{
    match value {
        serde_json::Value::Object(entries) => {
            for (name, value) in entries.iter_mut() {
                if is_sensitive(name) {
                    redact_sensitive_value(value);
                } else {
                    redact_json_definition_inner(value, secrets);
                }
            }
        }
        _ => redact_json_definition_inner(value, secrets),
    }
}

fn redact_sensitive_value(value: &mut serde_json::Value) {
    if let serde_json::Value::Object(map) = value {
        if map.contains_key("value_env") {
            return;
        }
        if let Some(value) = map.get_mut("value") {
            *value = serde_json::json!("***");
            return;
        }
    }
    *value = serde_json::json!("***");
}

fn is_public_api_key_state(key: &str, value: &serde_json::Value) -> bool {
    key.eq_ignore_ascii_case("apiKey")
        && value
            .as_str()
            .is_some_and(|state| matches!(state, "not-required" | "configured" | "missing"))
}

fn normalized_key(key: &str) -> String {
    key.to_ascii_lowercase().replace(['-', '_', ' ', '.'], "")
}

fn is_sensitive_json_key(key: &str) -> bool {
    matches!(
        normalized_key(key).as_str(),
        "authorization"
            | "proxyauthorization"
            | "cookie"
            | "setcookie"
            | "xapikey"
            | "apikey"
            | "apitoken"
            | "token"
            | "password"
            | "passwd"
            | "secret"
            | "secretkey"
            | "privatekey"
            | "signature"
            | "sig"
            | "auth"
            | "accesstoken"
            | "refreshtoken"
            | "clientsecret"
            | "credential"
            | "credentials"
    )
}

/// ヘッダ名が秘匿対象かどうか（仕様 agent-integration.md §4.2）。
pub fn is_sensitive_header_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [
        "authorization",
        "proxy-authorization",
        "cookie",
        "set-cookie",
        "api-key",
        "apikey",
        "auth",
        "token",
        "secret",
        "password",
        "passwd",
        "credential",
        "signature",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

/// 送信予定のヘッダから認証・ Cookie 類を伏せる（`--dry-run` の表示用）。
/// 値が環境変数から解決されたものでなくても、ヘッダ名だけで判定して伏せる。
pub fn redact_headers(headers: &mut [(String, String)]) {
    for (name, value) in headers.iter_mut() {
        if is_sensitive_header_name(name) {
            *value = "***".to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_does_not_leak() {
        let secret = Secret::new("topsecret");
        assert_eq!(format!("{secret:?}"), "Secret(<redacted>)");
        assert_eq!(secret.expose(), "topsecret");
    }

    #[test]
    fn redaction_list_skips_short_values() {
        let mut secrets = Secrets::new();
        secrets.insert("A", "abcdef");
        secrets.insert("B", "xy");
        assert_eq!(redaction_list(&secrets), vec!["abcdef".to_string()]);
    }

    #[test]
    fn redact_text_hides_long_values_but_avoids_short_values() {
        let secrets = vec!["abcd1234".to_string(), "xy".to_string()];
        assert_eq!(
            redact_text("https://x.test/?k=abcd1234&x=xy", &secrets),
            "https://x.test/?k=***&x=xy"
        );
        assert_eq!(
            redact_text(
                "overlap=abcde; repeated=aaaaa",
                &["abcd".to_string(), "bcde".to_string(), "aaaa".to_string()]
            ),
            "overlap=***; repeated=***"
        );
    }

    #[test]
    fn redact_display_text_masks_embedded_url_credentials_and_parameters() {
        let text = "failed at https://alice:password@example.com/token=path-secret?p=public&token=query-secret.";
        let shown = redact_display_text(text, &[]);
        assert_eq!(
            shown,
            "failed at https://***:***@example.com/token=***?p=public&token=***."
        );
        assert!(!shown.contains("alice"));
        assert!(!shown.contains("password"));
        assert!(!shown.contains("path-secret"));
        assert!(!shown.contains("query-secret"));

        let ipv6 = redact_display_text("failed at http://alice:password@[::1]/path.", &[]);
        assert_eq!(ipv6, "failed at http://***:***@[::1]/path.");
    }

    #[test]
    fn redact_json_hides_explicit_values_and_keeps_env_names() {
        let mut value = serde_json::json!({
            "request": {
                "headers": {
                    "X-Api-Key": {"value": "plain-secret"},
                    "X-Session": {"value_env": "SESSION_TOKEN"}
                },
                "query": {"q": "{{query}}", "api_key": "literal-api-key"},
                "url": "https://x.test/path?token=plain-secret"
            }
        });
        redact_json_with(&mut value, &[]);
        assert_eq!(value["request"]["headers"]["X-Api-Key"]["value"], "***");
        assert_eq!(
            value["request"]["headers"]["X-Session"]["value_env"],
            "SESSION_TOKEN"
        );
        assert_eq!(value["request"]["query"]["q"], "{{query}}");
        assert_eq!(value["request"]["query"]["api_key"], "***");
        assert_eq!(value["request"]["url"], "https://x.test/path?token=***");
    }

    #[test]
    fn redact_json_output_masks_credentials_without_blanking_regular_values() {
        let mut value = serde_json::json!({
            "event": "diagnostics",
            "details": "request failed at https://user:pass@example.com/a?token=secret-value",
            "headers": [["X-Session", "visible"], ["Authorization", "Bearer literal-secret"]],
            "body": "{\"password\":\"literal-secret\",\"label\":\"visible\"}",
            "value": "ordinary value"
        });
        redact_json_output(&mut value, &[]);
        assert_eq!(
            value["details"],
            "request failed at https://***:***@example.com/a?token=***"
        );
        assert_eq!(value["headers"][0][1], "visible");
        assert_eq!(value["headers"][1][1], "***");
        let body: serde_json::Value =
            serde_json::from_str(value["body"].as_str().unwrap()).unwrap();
        assert_eq!(body["password"], "***");
        assert_eq!(body["label"], "visible");
        assert_eq!(value["value"], "ordinary value");
    }

    #[test]
    fn redact_body_handles_form_encoded_secrets() {
        assert_eq!(
            redact_body("client_id=visible&access_token=secret123", &[]),
            "client_id=visible&access_token=***"
        );
        assert_eq!(redact_body("secret=***", &[]), "secret=***");
    }

    #[test]
    fn collect_explicit_values_and_headers_use_one_masking_policy() {
        let value = serde_json::json!({
            "headers": {"X-Session": {"value": "static-secret"}},
            "description": "contains static-secret"
        });
        let mut list = Vec::new();
        collect_explicit_values(&value, &mut list);
        assert_eq!(list, ["static-secret"]);
        assert_eq!(
            redact_display_text("Bearer static-secret", &list),
            "Bearer ***"
        );

        let mut headers = vec![
            ("Accept".to_string(), "text/html".to_string()),
            ("X-Api-Key".to_string(), "literal-secret".to_string()),
            ("Authorization".to_string(), "Bearer tok".to_string()),
        ];
        redact_headers(&mut headers);
        assert_eq!(headers[0].1, "text/html");
        assert_eq!(headers[1].1, "***");
        assert_eq!(headers[2].1, "***");
    }
}
