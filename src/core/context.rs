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
    // 開いているフェンスのマーカー（"```" / "~~~"）。閉じフェンスは開きフェンスと
    // 同一の文字で補う（仕様 context-assembly.md §4.3「コードブロックの途中で
    // 終わった場合は閉じフェンスを補う」）。従来は閉じが "```" 固定のため、
    // `~~~sh` ブロック途中で切り詰めると `~~~` が閉じられず、`[truncated: ...]` と
    // `--- end ---` がコードブロック内に飲み込まれていた。
    let mut open_fence: Option<&'static str> = None;
    for line in text.lines() {
        let line_chars = chars(line) + 1; // 改行込み
        if used + line_chars > limit {
            break;
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            open_fence = match open_fence {
                Some("```") => None, // 同一マーカーで閉じる
                Some(other) => Some(other),
                None => Some("```"),
            };
        } else if trimmed.starts_with("~~~") {
            open_fence = match open_fence {
                Some("~~~") => None,
                Some(other) => Some(other),
                None => Some("~~~"),
            };
        }
        used += line_chars;
        out.push(line);
    }
    let mut body = out.join("\n");
    if let Some(marker) = open_fence {
        body.push('\n');
        body.push_str(marker);
    }
    (body, true, total)
}

/// 切り詰めの注記。
pub fn truncation_note(shown: usize, total: usize) -> String {
    format!("[truncated: shown {shown} / total {total} chars]")
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

    /// #60-3: `~~~sh` ブロックを途中で切り詰めても、閉じフェンスは開きフェンスと同じ
    /// `~~~` で補う（従来は ` ``` ` 固定で閉じられず、フェンスが開いたまま
    /// `[truncated: ...]` / `--- end ---` がコードブロック内に飲み込まれていた）。
    #[test]
    fn truncates_and_closes_tilde_fence_with_tilde() {
        let body = (0..40)
            .map(|i| format!("echo line_{i:04} payload_padding_padding_padding"))
            .collect::<Vec<_>>()
            .join("\n");
        let text = format!("~~~sh\n{body}\n~~~\n");
        let (out, truncated, total) = truncate_to_budget(&text, 200);
        assert!(truncated);
        assert!(total > 200);
        // 開いた `~~~` フェンスは `~~~` で閉じられ、バッククォートで閉じられない。
        assert!(out.ends_with("~~~"));
        assert!(!out.ends_with("```"));
    }
}
