//! 出力（テキスト・JSON Lines）。

use serde_json::Value;

use crate::core::errors::{AppError, Check, Diagnostic, Locale};
use crate::core::history::HistoryEntry;
use crate::core::model::{Heading, SearchHit};
use crate::core::session::DocumentRef;
use crate::usecases::doctor::CheckResult;

/// 出力の宛先と装飾の設定。
#[derive(Clone)]
pub struct Renderer {
    pub json: bool,
    pub color: bool,
    pub locale: Locale,
    pub quiet: bool,
    width: usize,
}

impl Renderer {
    pub fn new(json: bool, color: bool, locale: Locale, quiet: bool, width: usize) -> Self {
        Self {
            json,
            color,
            locale,
            quiet,
            width,
        }
    }

    /// 標準出力（データ）。
    pub fn out(&self, line: &str) {
        println!("{line}");
    }

    /// 標準エラー（メニュー・進捗・警告）。
    pub fn err(&self, line: &str) {
        eprintln!("{line}");
    }

    /// 進捗（`--quiet` で抑制）。
    pub fn info(&self, line: &str) {
        if !self.quiet {
            eprintln!("{line}");
        }
    }

    /// JSON Lines を 1 行出力する。
    pub fn event(&self, value: &Value) {
        match serde_json::to_string(value) {
            Ok(text) => println!("{text}"),
            Err(e) => eprintln!("JSON の生成に失敗: {e}"),
        }
    }

    /// エラーを出力する（JSON のときは `error` イベント）。
    pub fn error(&self, err: &AppError) {
        if self.json {
            self.event(&serde_json::json!({
                "v": 1,
                "event": "error",
                "code": err.code.as_str(),
                "message": crate::core::errors::message(err.code, self.locale),
                "hint": crate::core::errors::hint(err.code, self.locale),
                "details": err.detail,
            }));
        }
        eprintln!(
            "error: {} {}",
            err.code.as_str(),
            crate::core::errors::message(err.code, self.locale)
        );
        if let Some(detail) = &err.detail {
            eprintln!("  詳細: {detail}");
        }
        eprintln!(
            "  対処: {}",
            crate::core::errors::hint(err.code, self.locale)
        );
    }

    /// 警告を出力する。
    pub fn warn(&self, diagnostic: &Diagnostic) {
        if self.quiet {
            return;
        }
        eprintln!(
            "warning: {} {}",
            diagnostic.code.as_str(),
            diagnostic.message
        );
    }

    /// 検証結果の 1 行。
    pub fn check(&self, check: &Check) {
        let mark = match check {
            Check::Ok(_) => "ok",
            Check::Warn(_) => "warn",
            Check::Fail(_) => "fail",
        };
        self.out(&format!("[{mark:>4}] {}", check.text()));
    }

    /// 検索結果の 1 件を複数行で出力する。
    pub fn hit_block(&self, hit: &SearchHit, decided_cols: usize) -> String {
        let mut out = String::new();
        out.push_str(&format!("[{}] {}\n", hit.label(), hit.title));
        let url = if decided_cols < 60 {
            String::new()
        } else {
            hit.url.clone()
        };
        if !url.is_empty() {
            out.push_str(&format!("     {url}\n"));
        }
        if let Some(snippet) = &hit.snippet {
            let trimmed = truncate(snippet, self.width.saturating_sub(5));
            out.push_str(&format!("     {trimmed}\n"));
        }
        out
    }

    /// 結果一覧（1 行 1 件の要約）。
    pub fn hit_line(&self, hit: &SearchHit) -> String {
        let snippet = hit
            .snippet
            .as_deref()
            .map(|s| truncate(s, 80))
            .unwrap_or_default();
        if snippet.is_empty() {
            format!("[{}] {} <{}>", hit.label(), hit.title, hit.url)
        } else {
            format!(
                "[{}] {}\n      {} <{}>",
                hit.label(),
                hit.title,
                snippet,
                hit.url
            )
        }
    }

    /// 見出し一覧（アウトライン）。
    pub fn outline_lines(&self, document: &DocumentRef, headings: &[Heading], depth: u8) -> String {
        let mut out = format!(
            "[D{}] {}   {}\n",
            document.id, document.title, document.final_url
        );
        for heading in headings {
            if heading.hidden {
                continue;
            }
            let indent = "  ".repeat(heading.level.saturating_sub(1) as usize);
            out.push_str(&format!(
                "{indent}{:<5} h{}  {:<40}  ({} 字){}\n",
                heading.label(),
                heading.level,
                truncate(&heading.text, 40),
                heading.chars,
                if heading.has_children { " ↓" } else { "" }
            ));
        }
        let hidden = headings.iter().filter(|h| h.hidden).count();
        if hidden > 0 {
            out.push_str(&format!(
                "  （深度 {depth} を超える見出し {hidden} 件は非表示。3) で深度を変更）\n"
            ));
        }
        out
    }

    /// 節本文のブロック。
    pub fn section_block(
        &self,
        header: &str,
        url: &str,
        body: &str,
        truncated: Option<(usize, usize)>,
    ) -> String {
        let mut out = String::new();
        out.push_str(header);
        out.push('\n');
        out.push_str(&format!("[untrusted] {url}\n"));
        out.push_str(body);
        if !body.ends_with('\n') {
            out.push('\n');
        }
        if let Some((shown, total)) = truncated {
            out.push_str(&format!("[truncated: 表示 {shown} / 全体 {total} 字]\n"));
        }
        out.push_str("--- end ---\n");
        out
    }

    /// 履歴の 1 行。
    pub fn history_line(&self, entry: &HistoryEntry) -> String {
        let at = entry
            .at
            .replace('T', " ")
            .chars()
            .take(16)
            .collect::<String>();
        format!(
            "{:<5} {}  {:<8} {}",
            entry.id,
            at,
            entry.kind.as_str(),
            entry.summary()
        )
    }

    /// クォータ余力の 1 行。
    pub fn quota_line(
        &self,
        metrics: &[crate::core::config::quota::MetricUsage],
        metric: &str,
    ) -> String {
        for m in metrics {
            if m.name.ends_with(metric) {
                return format!(
                    "{}/{} ({})",
                    m.used,
                    m.limit.map(|l| l.to_string()).unwrap_or_else(|| "∞".into()),
                    m.name
                );
            }
        }
        String::new()
    }

    /// 診断結果の一覧。
    pub fn doctor_lines(&self, results: &[CheckResult]) -> String {
        let mut out = String::new();
        for r in results {
            out.push_str(&format!(
                "[{:>4}] {:<24} {}\n",
                r.check.label(),
                r.name,
                r.check.text()
            ));
        }
        out
    }
}

impl Renderer {
    /// 対話モード向けの複製。
    pub fn clone_for_interactive(&self) -> Renderer {
        self.clone()
    }
}

/// 端末幅を意識した切り詰め。
pub fn truncate(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0usize;
    for c in text.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if used + cw > width.saturating_sub(1) {
            out.push('…');
            return out;
        }
        used += cw;
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_by_display_width() {
        assert_eq!(truncate("あいうえお", 5), "あい…");
        assert_eq!(truncate("abc", 10), "abc");
        assert_eq!(truncate("abcdef", 4), "abc…");
    }

    #[test]
    fn json_error_has_code() {
        let r = Renderer::new(true, false, Locale::Ja, true, 80);
        // 出力は行わず、組み立てのみ検証する
        let err = AppError::with_detail(crate::core::errors::ErrorCode::E1007, "空");
        assert_eq!(err.code.as_str(), "E1007");
        assert!(r.quiet);
    }
}
