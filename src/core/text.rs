//! 文字列ユーティリティ（書記素数・切り詰め・検索語の正規化）。

/// 文字数（`char` 単位）。
pub fn chars(s: &str) -> usize {
    s.chars().count()
}

/// 先頭 `limit` 文字へ切り詰める。切り詰めた場合は末尾に `…` を付す。
pub fn truncate_chars(s: &str, limit: usize) -> String {
    if limit == 0 {
        return String::new();
    }
    let count = s.chars().count();
    if count <= limit {
        return s.to_string();
    }
    let mut out: String = s.chars().take(limit.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// 前後の空白（全角空白を含む）を除去する。
pub fn trim(s: &str) -> String {
    s.trim_matches(|c: char| c.is_whitespace() || c == '\u{3000}')
        .to_string()
}

/// 連続する空白（全角空白・改行を含む）を 1 つの半角空白にまとめる。
pub fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_ws = false;
    for ch in s.chars() {
        let is_ws = ch.is_whitespace() || ch == '\u{3000}';
        if is_ws {
            if !in_ws && !out.is_empty() {
                out.push(' ');
            }
            in_ws = true;
        } else {
            out.push(ch);
            in_ws = false;
        }
    }
    out.trim_end().to_string()
}

/// 検索語の正規化。制御文字を除去し、空白をまとめ、512 文字で切り詰める。
///
/// 戻り値の `bool` は「切り詰めが発生したか」。
pub fn normalize_query(input: &str) -> (String, bool) {
    let cleaned: String = input.chars().filter(|c| !c.is_control()).collect();
    let collapsed = collapse_ws(&cleaned);
    if chars(&collapsed) > 512 {
        let mut out: String = collapsed.chars().take(512).collect();
        out = out.trim_end().to_string();
        (out, true)
    } else {
        (collapsed, false)
    }
}

/// HTML エンティティの最小限の復号。
pub fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    loop {
        match rest.find('&') {
            None => {
                out.push_str(rest);
                break;
            }
            Some(idx) => {
                out.push_str(&rest[..idx]);
                let after = &rest[idx..];
                match after.find(';').filter(|p| *p <= 10) {
                    Some(end) => {
                        let entity = &after[1..end];
                        let decoded = match entity {
                            "amp" => Some('&'),
                            "lt" => Some('<'),
                            "gt" => Some('>'),
                            "quot" => Some('"'),
                            "apos" => Some('\''),
                            "nbsp" => Some(' '),
                            "hellip" => Some('…'),
                            "mdash" => Some('—'),
                            "ndash" => Some('–'),
                            _ => {
                                if let Some(num) = entity
                                    .strip_prefix("#x")
                                    .or_else(|| entity.strip_prefix("#X"))
                                {
                                    u32::from_str_radix(num, 16).ok().and_then(char::from_u32)
                                } else if let Some(num) = entity.strip_prefix('#') {
                                    num.parse::<u32>().ok().and_then(char::from_u32)
                                } else {
                                    None
                                }
                            }
                        };
                        match decoded {
                            Some(ch) => {
                                out.push(ch);
                                rest = &after[end + 1..];
                            }
                            None => {
                                out.push('&');
                                rest = &after[1..];
                            }
                        }
                    }
                    None => {
                        out.push('&');
                        rest = &after[1..];
                    }
                }
            }
        }
    }
    out
}

/// 表示幅（全角 2 桁）を考慮して切り詰める。
pub fn truncate_display(s: &str, width: usize) -> String {
    use unicode_width::UnicodeWidthChar;
    let mut used = 0usize;
    let mut out = String::new();
    for ch in s.chars() {
        let w = ch.width().unwrap_or(0);
        if used + w > width.saturating_sub(1) && width > 0 {
            out.push('…');
            return out;
        }
        used += w;
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_keeps_short_strings() {
        assert_eq!(truncate_chars("abc", 5), "abc");
        assert_eq!(truncate_chars("abcdef", 4), "abc…");
    }

    #[test]
    fn collapse_handles_full_width_space() {
        assert_eq!(collapse_ws("a\u{3000}\u{3000}b\n c "), "a b c");
    }

    #[test]
    fn normalize_query_truncates() {
        let long = "あ".repeat(600);
        let (q, truncated) = normalize_query(&long);
        assert!(truncated);
        assert_eq!(q.chars().count(), 512);
    }

    #[test]
    fn decode_entities_works() {
        assert_eq!(
            decode_entities("a &amp; b &#65; &#x42; &unknown;"),
            "a & b A B &unknown;"
        );
    }
}
