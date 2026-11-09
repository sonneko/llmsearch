//! Markdown の見出し抽出と節分割。

use crate::core::model::Heading;
use crate::core::text::chars;

/// フェンス状態（`true` の行はコードブロック内）。
pub fn fence_flags(lines: &[&str]) -> Vec<bool> {
    let mut flags = Vec::with_capacity(lines.len());
    let mut open: Option<(char, usize)> = None;
    for line in lines {
        let trimmed = line.trim_start();
        let marker = if trimmed.starts_with("```") {
            Some('`')
        } else if trimmed.starts_with("~~~") {
            Some('~')
        } else {
            None
        };
        let count = marker
            .map(|m| trimmed.chars().take_while(|c| *c == m).count())
            .unwrap_or(0);
        match open {
            None => {
                flags.push(false);
                if let Some(m) = marker {
                    open = Some((m, count));
                }
            }
            Some((m, n)) => {
                flags.push(true);
                if marker == Some(m) && count >= n {
                    open = None;
                }
            }
        }
    }
    flags
}

fn atx(line: &str) -> Option<(u8, String)> {
    let trimmed = line.trim_start();
    let level = trimmed.chars().take_while(|c| *c == '#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let rest = trimmed[level..].trim_start();
    let text = rest.trim_end().trim_end_matches('#').trim_end();
    Some((level as u8, text.to_string()))
}

fn setext_underline(line: &str) -> Option<u8> {
    let t = line.trim();
    if t.is_empty() || !t.chars().all(|c| c == '=' || c == '-') {
        return None;
    }
    if t.starts_with('=') { Some(1) } else { Some(2) }
}

/// 見出しを抽出して識別子と節範囲を確定する。
pub fn extract_headings(markdown: &str) -> Vec<Heading> {
    let lines: Vec<&str> = markdown.lines().collect();
    let flags = fence_flags(&lines);
    let mut raw: Vec<(u8, String, usize)> = Vec::new();

    let mut prev_non_empty: Option<usize> = None;
    let mut i = 0;
    while i < lines.len() {
        if flags[i] {
            i += 1;
            continue;
        }
        let line = lines[i];
        if let Some((level, text)) = atx(line) {
            raw.push((level, text, i));
            prev_non_empty = Some(i);
            i += 1;
            continue;
        }
        if let Some(level) = setext_underline(line)
            && let Some(prev) = prev_non_empty
            && prev + 1 == i
        {
            let text = lines[prev].trim().to_string();
            if !text.is_empty() && atx(lines[prev]).is_none() {
                raw.push((level, text, prev));
                prev_non_empty = Some(i);
                i += 1;
                continue;
            }
        }
        if !line.trim().is_empty() {
            prev_non_empty = Some(i);
        }
        i += 1;
    }

    let total = lines.len();
    let mut headings = Vec::with_capacity(raw.len());
    for (idx, (level, text, start)) in raw.iter().enumerate() {
        let mut end = total; // 1 始まりの最終行
        for (next_level, _, next_start) in raw.iter().skip(idx + 1) {
            if *next_level <= *level {
                end = *next_start;
                break;
            }
        }
        let body: usize = if end > start + 1 {
            lines[start + 1..end].iter().map(|l| chars(l)).sum()
        } else {
            0
        };
        let has_children = raw
            .get(idx + 1)
            .map(|(next_level, _, next_start)| *next_start < end && *next_level > *level)
            .unwrap_or(false);
        headings.push(Heading {
            id: (idx + 1) as u32,
            level: *level,
            text: text.clone(),
            start_line: start + 1,
            end_line: end,
            chars: body,
            has_children,
            hidden: false,
        });
    }
    headings
}

/// 見出しを含む節の本文（見出し行を含む）。
pub fn section_text(markdown: &str, heading: &Heading, include_children: bool) -> String {
    let lines: Vec<&str> = markdown.lines().collect();
    let start = heading.start_line.saturating_sub(1);
    let mut end = heading.end_line.min(lines.len());
    if !include_children && let Some(child) = find_first_child(&lines, heading) {
        end = end.min(child.saturating_sub(1));
    }
    lines[start..end].join("\n")
}

fn find_first_child(lines: &[&str], heading: &Heading) -> Option<usize> {
    let flags = fence_flags(lines);
    for i in heading.start_line..lines.len() {
        if flags[i] || i >= heading.end_line {
            continue;
        }
        if let Some((level, _)) = atx(lines[i])
            && level > heading.level
        {
            return Some(i + 1);
        }
    }
    None
}

/// 文書全体の本文。
pub fn whole_body(markdown: &str) -> String {
    markdown.to_string()
}

/// 子見出しの件数。
pub fn child_count(headings: &[Heading], heading: &Heading) -> usize {
    headings
        .iter()
        .filter(|h| {
            h.start_line > heading.start_line
                && h.start_line < heading.end_line
                && h.level > heading.level
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str =
        "# Title\n\nintro text\n\n## A\n\nA body\n\n### A1\n\nA1 body\n\n## B\n\nB body\n";

    #[test]
    fn extracts_levels_and_ranges() {
        let hs = extract_headings(DOC);
        assert_eq!(hs.len(), 4);
        assert_eq!(hs[0].text, "Title");
        assert_eq!(hs[0].level, 1);
        assert_eq!(hs[2].text, "A1");
        assert_eq!(hs[2].level, 3);
        // A の節は B の直前まで
        assert_eq!(hs[1].end_line, hs[3].start_line - 1);
        assert!(hs[1].has_children);
    }

    #[test]
    fn ignores_headings_inside_fences() {
        let md = "# T\n\n```\n# not a heading\n```\n\n## Real\n";
        let hs = extract_headings(md);
        assert_eq!(hs.len(), 2);
        assert_eq!(hs[1].text, "Real");
    }

    #[test]
    fn section_text_includes_heading() {
        let hs = extract_headings(DOC);
        let text = section_text(DOC, &hs[1], true);
        assert!(text.starts_with("## A"));
        assert!(text.contains("A1 body"));
        let no_children = section_text(DOC, &hs[1], false);
        assert!(!no_children.contains("A1 body"));
    }

    #[test]
    fn setext_headings_are_recognized() {
        let md = "Title\n=====\n\nbody\n";
        let hs = extract_headings(md);
        assert_eq!(hs.len(), 1);
        assert_eq!(hs[0].level, 1);
        assert_eq!(hs[0].text, "Title");
    }
}
