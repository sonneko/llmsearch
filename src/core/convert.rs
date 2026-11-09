//! HTML → 本文抽出（密度推定）→ Markdown 変換。

use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};

use crate::core::config::filter::ContentFilters;
use crate::core::errors::{AppError, ErrorCode};
use crate::core::markdown;
use crate::core::model::{DocumentMeta, Heading};
use crate::core::outline;
use crate::core::text;

/// 本文抽出の方式。設定で切り替えられる。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ExtractMode {
    /// テキスト密度による推定（既定）。
    #[default]
    Density,
    /// Readability 相当のスコアリング。
    Readability,
    /// `<body>` 全体。
    Whole,
}

impl ExtractMode {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "density" => ExtractMode::Density,
            "readability" => ExtractMode::Readability,
            "whole" => ExtractMode::Whole,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ExtractMode::Density => "density",
            ExtractMode::Readability => "readability",
            ExtractMode::Whole => "whole",
        }
    }
}

/// 変換結果。
#[derive(Debug, Clone)]
pub struct Converted {
    pub markdown: String,
    pub title: String,
    pub site: Option<String>,
    pub language: Option<String>,
    pub published: Option<String>,
    pub mode: ExtractMode,
    pub warnings: Vec<String>,
    pub headings: Vec<Heading>,
}

/// 除去する定型要素（既定）。
pub const DEFAULT_STRIP: &[&str] = &[
    "script", "style", "noscript", "iframe", "svg", "canvas", "form", "nav", "aside", "footer",
    "header", "dialog",
];

/// HTML を Markdown へ変換する。
pub fn convert(
    html: &str,
    final_url: &url::Url,
    mode: ExtractMode,
    content: Option<&ContentFilters>,
) -> Result<Converted, AppError> {
    let doc = Html::parse_document(html);
    let mut warnings = Vec::new();

    let title = doc
        .select(&sel("title"))
        .next()
        .map(|e| text::collapse_ws(&e.text().collect::<Vec<_>>().join(" ")))
        .or_else(|| meta_content(&doc, "og:title"))
        .unwrap_or_default();
    let site =
        meta_content(&doc, "og:site_name").or_else(|| meta_content(&doc, "application-name"));
    let language = doc
        .root_element()
        .value()
        .attr("lang")
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty());
    let published = meta_content(&doc, "article:published_time")
        .or_else(|| meta_content(&doc, "datePublished"))
        .or_else(|| {
            doc.select(&sel("time[datetime]"))
                .next()
                .and_then(|e| e.value().attr("datetime").map(|s| s.to_string()))
        })
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    // 除去セレクタ（既定 + 設定）。`whole` では既定の除去を行わない（設定のみ従う）。
    let mut strip: Vec<String> = if mode == ExtractMode::Whole {
        Vec::new()
    } else {
        DEFAULT_STRIP.iter().map(|s| s.to_string()).collect()
    };
    if let Some(cf) = content {
        strip.extend(cf.strip_selectors.iter().cloned());
    }

    let container = match mode {
        ExtractMode::Whole => None,
        ExtractMode::Density => pick_by_density(&doc),
        ExtractMode::Readability => pick_by_readability(&doc),
    };

    let html_fragment = match container {
        Some(element) => {
            let mut clone = element.html();
            clone = remove_selectors(&clone, &strip);
            clone
        }
        None => {
            let body = doc
                .select(&sel("body"))
                .next()
                .map(|e| e.inner_html())
                .unwrap_or_else(|| doc.root_element().inner_html());
            let cleaned = remove_selectors(&body, &strip);
            if mode != ExtractMode::Whole {
                warnings.push("本文推定に失敗したため本文全体を変換した".to_string());
            }
            cleaned
        }
    };

    let mut md = htmd::HtmlToMarkdown::builder()
        .skip_tags(vec!["script", "style", "noscript"])
        .build()
        .convert(&html_fragment)
        .map_err(|e| {
            AppError::with_detail(ErrorCode::E3004, format!("Markdown 変換に失敗: {e}"))
        })?;

    // 見出しの無い文書はタイトルを見出しとして補う（節選択を可能にするため）
    let heads = outline::extract_headings(&md);
    if heads.is_empty() && !title.is_empty() {
        md = format!("# {}\n\n{}", text::collapse_ws(&title), md);
    }

    md = markdown::normalize(
        &md,
        &crate::core::config::filter::NormalizeOptions::default(),
    );
    md = markdown::absolutize(&md, final_url);

    let headings = outline::extract_headings(&md);

    if text::chars(&md) < 200 {
        warnings.push("本文が 200 文字未満。JavaScript 依存のページかもしれない".to_string());
    }

    Ok(Converted {
        markdown: md,
        title,
        site,
        language,
        published,
        mode,
        warnings,
        headings,
    })
}

/// 文書メタデータを組み立てる。
pub fn build_meta(
    converted: &Converted,
    requested_url: &url::Url,
    final_url: &url::Url,
    fetched_at: chrono::DateTime<chrono::Utc>,
    ttl_hours: u64,
) -> DocumentMeta {
    DocumentMeta {
        url: requested_url.to_string(),
        final_url: final_url.to_string(),
        title: if converted.title.is_empty() {
            converted
                .headings
                .first()
                .map(|h| h.text.clone())
                .unwrap_or_else(|| final_url.to_string())
        } else {
            converted.title.clone()
        },
        site: converted
            .site
            .clone()
            .or_else(|| crate::core::url::host_of(final_url)),
        fetched_at: fetched_at.to_rfc3339(),
        expires_at: (fetched_at + chrono::Duration::hours(ttl_hours as i64)).to_rfc3339(),
        chars: text::chars(&converted.markdown),
        headings: converted.headings.len(),
        extractor: converted.mode.as_str().to_string(),
        warnings: converted.warnings.clone(),
        cacheable: ttl_hours > 0,
    }
}

fn sel(pattern: &str) -> Selector {
    Selector::parse(pattern).expect("静的セレクタ")
}

fn meta_content(doc: &Html, key: &str) -> Option<String> {
    let patterns = [
        format!("meta[property=\"{key}\"]"),
        format!("meta[name=\"{key}\"]"),
        format!("meta[itemprop=\"{key}\"]"),
    ];
    for p in patterns {
        if let Ok(s) = Selector::parse(&p)
            && let Some(content) = doc
                .select(&s)
                .next()
                .and_then(|e| e.value().attr("content").map(text::collapse_ws))
            && !content.is_empty()
        {
            return Some(content);
        }
    }
    None
}

fn remove_selectors(html: &str, strip: &[String]) -> String {
    let doc = Html::parse_fragment(html);
    let selectors: Vec<Selector> = strip
        .iter()
        .filter_map(|s| Selector::parse(s).ok())
        .collect();
    if selectors.is_empty() {
        return html.to_string();
    }
    // 要素を除去するため、外側から順に「その要素の HTML」を空文字へ置換する
    let mut out = html.to_string();
    for s in &selectors {
        for element in doc.select(s) {
            let fragment = element.html();
            if !fragment.is_empty() {
                out = out.replacen(&fragment, "", 1);
            }
        }
    }
    out
}

fn node_key(el: &scraper::node::Element) -> String {
    match el.id() {
        Some(id) => id.to_string(),
        None => format!("{}:{}", el.name(), el.attr("class").unwrap_or_default()),
    }
}

fn text_len(element: &scraper::ElementRef<'_>) -> usize {
    let text: String = element.text().collect::<Vec<_>>().join("");
    text::chars(&text)
}

fn link_density(element: &scraper::ElementRef<'_>) -> f64 {
    let total = text_len(element);
    if total == 0 {
        return 1.0;
    }
    let link: usize = element
        .select(&sel("a"))
        .map(|a| text::chars(&a.text().collect::<Vec<_>>().join("")))
        .sum();
    link as f64 / total as f64
}

fn pick_by_density(doc: &Html) -> Option<scraper::ElementRef<'_>> {
    let candidates = ["article", "main", "div", "section", "td"];
    let mut best: Option<(f64, scraper::ElementRef<'_>)> = None;
    for pattern in candidates {
        let Ok(s) = Selector::parse(pattern) else {
            continue;
        };
        for element in doc.select(&s) {
            let paras = element.select(&sel("p")).count();
            let len = text_len(&element);
            if paras == 0 && len < 400 {
                continue;
            }
            let density = link_density(&element);
            if density > 0.5 {
                continue;
            }
            let score = (len as f64) * (1.0 - density) + (paras as f64 * 20.0);
            if best.as_ref().map(|(b, _)| score > *b).unwrap_or(true) {
                best = Some((score, element));
            }
        }
    }
    best.map(|(_, e)| e)
}

fn pick_by_readability(doc: &Html) -> Option<scraper::ElementRef<'_>> {
    // 段落の文字数を加算し、親要素へスコアを伝播させる簡易版
    let mut scores: std::collections::HashMap<String, f64> = Default::default();
    for p in doc.select(&sel("p, pre, td, blockquote")) {
        let len = text_len(&p);
        if len < 25 {
            continue;
        }
        let mut node = p.parent();
        let mut depth = 0;
        while let Some(parent) = node.and_then(|n| n.value().as_element().map(|_| n)) {
            if let Some(el) = scraper::ElementRef::wrap(parent) {
                let key = el
                    .value()
                    .attr("id")
                    .map(|s| format!("id:{s}"))
                    .unwrap_or_else(|| {
                        let tag = el.value().name().to_string();
                        let cls = el.value().attr("class").unwrap_or_default().to_string();
                        format!("{tag}.{cls}#{}", node_key(el.value()))
                    });
                *scores.entry(key).or_insert(0.0) += (len as f64) / (depth as f64 + 1.0);
            }
            depth += 1;
            if depth > 5 {
                break;
            }
            node = parent.parent();
        }
    }
    // 最高スコアの要素を探す
    let mut best: Option<(f64, scraper::ElementRef<'_>)> = None;
    for element in doc.select(&sel("div, article, main, section, body")) {
        let key = element
            .value()
            .attr("id")
            .map(|s| format!("id:{s}"))
            .unwrap_or_else(|| {
                let tag = element.value().name().to_string();
                let cls = element
                    .value()
                    .attr("class")
                    .unwrap_or_default()
                    .to_string();
                format!("{tag}.{cls}#{}", node_key(element.value()))
            });
        if let Some(score) = scores.get(&key) {
            let total = *score - link_density(&element) * 100.0;
            if best.as_ref().map(|(b, _)| total > *b).unwrap_or(true) {
                best = Some((total, element));
            }
        }
    }
    best.map(|(_, e)| e).or_else(|| pick_by_density(doc))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"<html lang="ja"><head>
        <title>テスト記事</title>
        <meta property="og:site_name" content="例のサイト">
        <script>var x = 1;</script>
        </head><body>
        <nav><a href="/">ホーム</a><a href="/b">次へ</a></nav>
        <article>
          <h1>記事の題</h1>
          <p>これは最初の段落である。十分な長さを持たせるために文章を続ける。</p>
          <h2>第一節</h2>
          <p>第一節の本文。ここにも十分な長さの文章を置いておく必要がある。</p>
          <h3>詳細</h3>
          <p>詳細の本文。</p>
          <h2>第二節</h2>
          <p>第二節の本文。</p>
        </article>
        <footer>著作権表示</footer>
        </body></html>"#;

    #[test]
    fn converts_and_keeps_headings() {
        let url = url::Url::parse("https://example.jp/a").unwrap();
        let out = convert(PAGE, &url, ExtractMode::Density, None).unwrap();
        assert!(
            out.markdown.contains("# 記事の題") || out.markdown.contains("## 記事の題"),
            "{}",
            out.markdown
        );
        assert!(out.markdown.contains("第二節"));
        assert!(!out.markdown.contains("著作権表示"));
        assert_eq!(out.site.as_deref(), Some("例のサイト"));
        let levels: Vec<u8> = out.headings.iter().map(|h| h.level).collect();
        assert!(levels.contains(&2));
    }

    #[test]
    fn whole_mode_keeps_footer() {
        let url = url::Url::parse("https://example.jp/a").unwrap();
        let out = convert(PAGE, &url, ExtractMode::Whole, None).unwrap();
        assert!(out.markdown.contains("著作権表示"));
    }

    #[test]
    fn density_mode_strips_scripts_and_nav() {
        let url = url::Url::parse("https://example.jp/a").unwrap();
        let out = convert(PAGE, &url, ExtractMode::Density, None).unwrap();
        assert!(!out.markdown.contains("var x"));
        assert!(!out.markdown.contains("ホーム"));
    }
}
