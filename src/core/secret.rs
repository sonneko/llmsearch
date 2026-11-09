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
            "（未設定）".to_string()
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
