//! Markdown の後処理。

use crate::core::config::filter::NormalizeOptions;
use crate::core::text;

/// 正規化（空行圧縮・行末空白除去・末尾改行・エンティティ復号）。
pub fn normalize(md: &str, opts: &NormalizeOptions) -> String {
    let mut lines: Vec<String> = Vec::new();
    let flags = crate::core::outline::fence_flags(&md.lines().collect::<Vec<_>>());
    for (i, line) in md.lines().enumerate() {
        let mut line = line.to_string();
        let in_fence = flags.get(i).copied().unwrap_or(false);
        if opts.trim_line_ends && !in_fence {
            line = line.trim_end().to_string();
        }
        if opts.collapse_blank_lines && line.trim().is_empty() && !in_fence {
            if lines
                .last()
                .map(|l: &String| l.trim().is_empty())
                .unwrap_or(true)
            {
                continue;
            }
            lines.push(String::new());
            continue;
        }
        lines.push(line);
    }
    while lines.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
        lines.pop();
    }
    let mut out = lines.join("\n");
    if opts.decode_entities {
        out = text::decode_entities(&out);
    }
    if opts.ensure_trailing_newline && !out.is_empty() {
        out.push('\n');
    }
    out
}

/// Markdown 中のリンク・画像の相対 URL を絶対化する。
pub fn absolutize(md: &str, base: &url::Url) -> String {
    use regex::Regex;
    let re = Regex::new(r"(!?\[[^\]]*\]\()([^)\s]+)(\))").expect("link regex");
    re.replace_all(md, |caps: &regex::Captures<'_>| {
        let target = &caps[2];
        if target.starts_with('#') || target.starts_with("data:") || target.starts_with("mailto:") {
            let cleaned = if target.starts_with("data:") {
                ""
            } else {
                target
            };
            return format!("{}{}{}", &caps[1], cleaned, &caps[3]);
        }
        if target.starts_with("javascript:") {
            return String::new();
        }
        match base.join(target) {
            Ok(abs) => format!("{}{}{}", &caps[1], abs.as_str(), &caps[3]),
            Err(_) => caps[0].to_string(),
        }
    })
    .to_string()
}

/// 見出しテキストの平坦化（連続空白の圧縮と前後トリム）。
pub fn flatten_heading(text: &str) -> String {
    text::collapse_ws(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapses_blank_lines_and_trims() {
        let out = normalize("a  \n\n\n\nb\n", &NormalizeOptions::default());
        assert_eq!(out, "a\n\nb\n");
    }

    #[test]
    fn keeps_code_block_whitespace() {
        let md = "```\nlet x = 1;   \n```\n";
        let out = normalize(md, &NormalizeOptions::default());
        assert!(out.contains("let x = 1;   "));
    }

    #[test]
    fn absolutizes_relative_links() {
        let base = url::Url::parse("https://example.jp/a/b").unwrap();
        let out = absolutize("[x](../c) ![i](/d.png)", &base);
        assert!(out.contains("https://example.jp/c"), "{out}");
        assert!(out.contains("https://example.jp/d.png"), "{out}");
    }

    #[test]
    fn drops_javascript_links() {
        let base = url::Url::parse("https://example.jp/").unwrap();
        let out = absolutize("[x](javascript:alert(1))", &base);
        assert!(out.is_empty() || !out.contains("javascript"));
    }
}
