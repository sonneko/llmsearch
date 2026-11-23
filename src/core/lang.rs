//! 検索エンジンへ送る言語タグの正規化（仕様 configuration.md §3.2.1）。
//!
//! 表示言語とは独立で、CLI の文言は常に英語である。

/// 正規化した言語タグ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Language {
    /// 小文字化した全体（例: `pt-br`）。
    pub tag: String,
    /// 主言語タグ（例: `pt`）。
    pub primary: String,
}

impl Default for Language {
    fn default() -> Self {
        Self {
            tag: "en".to_string(),
            primary: "en".to_string(),
        }
    }
}

impl Language {
    /// `Accept-Language` に載せる値（`q` 値は付けない）。
    pub fn accept_language(&self) -> &str {
        &self.tag
    }
}

/// 言語タグを正規化する。受理しない形は `None`。
///
/// 受理する形は `<主言語>[-<サブタグ>...]`。主言語は英字 2〜3 文字、
/// サブタグは英数字 1〜8 文字とする（BCP 47 の共通部分のみ）。
pub fn parse(tag: &str) -> Option<Language> {
    let trimmed = tag.trim();
    if trimmed.is_empty() || trimmed != tag {
        return None;
    }
    let mut parts = trimmed.split('-');
    let primary = parts.next()?;
    if !(2..=3).contains(&primary.len()) || !primary.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let mut normalized = vec![primary.to_ascii_lowercase()];
    for part in parts {
        if part.is_empty() || part.len() > 8 || !part.chars().all(|c| c.is_ascii_alphanumeric()) {
            return None;
        }
        normalized.push(part.to_ascii_lowercase());
    }
    Some(Language {
        tag: normalized.join("-"),
        primary: primary.to_ascii_lowercase(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_and_regional_tags() {
        let en = parse("en").unwrap();
        assert_eq!(en.tag, "en");
        assert_eq!(en.primary, "en");

        let pt = parse("pt-BR").unwrap();
        assert_eq!(pt.tag, "pt-br");
        assert_eq!(pt.primary, "pt");

        assert_eq!(parse("zh-Hant-TW").unwrap().primary, "zh");
    }

    #[test]
    fn rejects_malformed_tags() {
        for bad in [
            "",
            " ",
            "e",
            "engl",
            "en_",
            "en-",
            "-en",
            "en--us",
            "en us",
            "1n",
            "en-123456789",
        ] {
            assert!(parse(bad).is_none(), "{bad} should be rejected");
        }
    }
}
