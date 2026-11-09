//! コンテキスト予算の配分と切り詰め。

use crate::core::text::chars;

/// 予算を節へ配分する。端数は先頭に加算し、余りは後続へ繰り越す。
pub fn allocate(budget: usize, sizes: &[usize]) -> Vec<usize> {
    let mut out = vec![0usize; sizes.len()];
    let mut remaining = budget;
    let mut left = sizes.len();
    for (i, size) in sizes.iter().enumerate() {
        if left == 0 {
            break;
        }
        let share = remaining / left.max(1);
        let give = (*size).min(share);
        out[i] = give;
        remaining -= give;
        left -= 1;
    }
    // 余りを後続へ回す（先頭から順に埋める）
    for i in 0..out.len() {
        if remaining == 0 {
            break;
        }
        let size = sizes[i];
        if out[i] < size {
            let extra = (size - out[i]).min(remaining);
            out[i] += extra;
            remaining -= extra;
        }
    }
    out
}

/// 行境界で切り詰め、未閉のフェンスを閉じる。
///
/// 戻り値は `(本文, 切り詰めたか, 全体の文字数)`。
pub fn truncate_to_budget(text: &str, limit: usize) -> (String, bool, usize) {
    let total = chars(text);
    if total <= limit {
        return (text.to_string(), false, total);
    }
    let mut used = 0usize;
    let mut out: Vec<&str> = Vec::new();
    let mut open_fence: Option<String> = None;
    for line in text.lines() {
        let line_chars = chars(line) + 1; // 改行込み
        if used + line_chars > limit {
            break;
        }
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("```") {
            if open_fence.is_some() {
                open_fence = None;
            } else {
                open_fence = Some(format!("```{rest}"));
            }
        } else if let Some(rest) = trimmed.strip_prefix("~~~") {
            if open_fence.is_some() {
                open_fence = None;
            } else {
                open_fence = Some(format!("~~~{rest}"));
            }
        }
        used += line_chars;
        out.push(line);
    }
    let mut body = out.join("\n");
    if open_fence.is_some() {
        body.push('\n');
        body.push_str("```");
    }
    (body, true, total)
}

/// 切り詰めの注記。
pub fn truncation_note(shown: usize, total: usize) -> String {
    format!("[truncated: 表示 {shown} / 全体 {total} 字]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocates_without_exceeding_budget() {
        let alloc = allocate(100, &[10, 200, 50]);
        assert!(alloc.iter().sum::<usize>() <= 100);
        assert_eq!(alloc[0], 10);
        assert!(alloc[1] >= 30);
    }

    #[test]
    fn truncates_on_line_boundary_and_closes_fence() {
        let text = "line1\nline2\n```\ncode code code\n";
        let (out, truncated, total) = truncate_to_budget(text, 18);
        assert!(truncated);
        assert!(total > 18);
        assert!(out.ends_with("```"));
    }

    #[test]
    fn no_truncation_when_fits() {
        let (out, truncated, total) = truncate_to_budget("abc", 10);
        assert_eq!(out, "abc");
        assert!(!truncated);
        assert_eq!(total, 3);
    }
}
