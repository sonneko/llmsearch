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
        let n = self.0.chars().count();
        if n == 0 {
            "(not set)".to_string()
        } else if n <= 4 {
            "****".to_string()
        } else {
            format!("****{}", "")
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
        .filter_map(|n| secrets.value_of(n))
        .filter(|v| v.chars().count() >= 4)
        .map(|v| v.to_string())
        .collect()
}

/// 文字列に含まれる秘匿値を `***` に置換する。
///
/// エラーの詳細やログなど、URL を含む可能性のある文字列に使う。短い値は
/// 誤爆を避けるため置換しない（`redaction_list` と同じ基準）。
pub fn redact_text(text: &str, secrets: &[String]) -> String {
    let mut out = text.to_string();
    for value in secrets.iter().filter(|s| s.chars().count() >= 4) {
        if out.contains(value.as_str()) {
            out = out.replace(value.as_str(), "***");
        }
    }
    out
}

/// JSON 内のすべての文字列から秘匿値を伏字化する（定義の表示用）。
pub fn redact_json(value: &mut serde_json::Value, secrets: &Secrets) {
    let list = redaction_list(secrets);
    redact_json_with(value, &list);
}

/// 文字列一覧を指定して JSON を伏字化する。
///
/// `value_env`（環境変数名そのもの）は秘匿値ではないので残す。
/// `{"value": ...}` の形は定義に値を直接書いたもの（テンプレート値の固定指定）なので、
/// 内容にかかわらず伏せる（仕様 agent-integration.md §4.2）。
pub fn redact_json_with(value: &mut serde_json::Value, secrets: &[String]) {
    match value {
        serde_json::Value::String(text) => {
            // URL として読める文字列は、秘匿パラメータを伏せた形に直す。
            // 伏せる対象があるときだけ差し替える（テンプレートの見た目を変えないため）。
            if let Ok(parsed) = url::Url::parse(text)
                && matches!(parsed.scheme(), "http" | "https")
            {
                let redacted = crate::core::url::redact(&parsed, secrets);
                if redacted != *text {
                    *text = redacted;
                    return;
                }
            }
            *text = redact_text(text, secrets);
        }
        serde_json::Value::Array(items) => {
            for item in items.iter_mut() {
                redact_json_with(item, secrets);
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
                redact_json_with(item, secrets);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_does_not_leak() {
        let s = Secret::new("topsecret");
        assert_eq!(format!("{s:?}"), "Secret(<redacted>)");
        assert_eq!(s.expose(), "topsecret");
    }

    #[test]
    fn redaction_list_skips_short_values() {
        let mut secrets = Secrets::new();
        secrets.insert("A", "abcdef");
        secrets.insert("B", "xy");
        assert_eq!(redaction_list(&secrets), vec!["abcdef".to_string()]);
    }
}

/// ヘッダ名が秘匿対象かどうか（仕様 agent-integration.md §4.2）。
fn is_sensitive_header(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [
        "authorization",
        "proxy-authorization",
        "cookie",
        "set-cookie",
        "x-api-key",
        "x-auth-token",
        "api-key",
        "token",
        "secret",
        "password",
        "passwd",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

/// 送信予定のヘッダから認証・ Cookie 類を伏せる（ `--dry-run` の表示用）。
///
/// 値が環境変数から解決されたものでなくても、ヘッダ名だけで判定して伏せる。
pub fn redact_headers(headers: &mut [(String, String)]) {
    for (name, value) in headers.iter_mut() {
        if is_sensitive_header(name) {
            *value = "***".to_string();
        }
    }
}

#[cfg(test)]
mod redaction_tests {
    use super::*;

    #[test]
    fn redact_text_hides_long_values() {
        let secrets = vec!["abcd1234".to_string()];
        assert_eq!(
            redact_text("https://x.test/?k=abcd1234", &secrets),
            "https://x.test/?k=***"
        );
        // 短い値は誤爆を避けて残す
        assert_eq!(redact_text("a ab", &["ab".to_string()]), "a ab");
    }

    #[test]
    fn redact_json_hides_explicit_values_and_keeps_env_names() {
        let mut value = serde_json::json!({
            "headers": {"X-Api-Key": {"value": "plain-secret"}},
            "auth": {"value_env": "PROXY_PASS"},
            "url": "https://x.test/?token=plain-secret",
        });
        redact_json_with(&mut value, &[]);
        assert_eq!(value["headers"]["X-Api-Key"]["value"], "***");
        assert_eq!(value["auth"]["value_env"], "PROXY_PASS");
        assert_eq!(value["url"], "https://x.test/?token=***");
    }

    #[test]
    fn redact_json_leaves_plain_templates_alone() {
        let mut value = serde_json::json!({"url": "https://x.test/?q={{query}}"});
        redact_json_with(&mut value, &[]);
        assert_eq!(value["url"], "https://x.test/?q={{query}}");
    }

    #[test]
    fn redact_headers_hides_credentials() {
        let mut headers = vec![
            ("Accept".to_string(), "text/html".to_string()),
            ("X-Api-Key".to_string(), "plain-secret".to_string()),
            ("Authorization".to_string(), "Bearer tok".to_string()),
        ];
        redact_headers(&mut headers);
        assert_eq!(headers[0].1, "text/html");
        assert_eq!(headers[1].1, "***");
        assert_eq!(headers[2].1, "***");
    }
}
