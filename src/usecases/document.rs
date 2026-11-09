//! 文書の取得・アウトライン・節の読み出し。

use crate::core::config::filter::apply_content;
use crate::core::config::quota::{Metric, Usage};
use crate::core::context;
use crate::core::errors::{AppError, Diagnostic, ErrorCode, WarningCode};
use crate::core::model::{Heading, SectionBody};
use crate::core::outline;
use crate::core::session::{DocumentRef, Selection, cache_key};
use crate::core::text;
use crate::ports::fetch::{FetchOptions, Fetcher};
use crate::usecases::App;

/// 文書取得の結果。
#[derive(Debug, Clone)]
pub struct Fetched {
    pub document: DocumentRef,
    pub new: bool,
    pub warnings: Vec<Diagnostic>,
}

/// 取得済み文書（本文付き）。
pub struct LoadedDocument {
    pub document: DocumentRef,
    pub markdown: String,
    /// キャッシュを使わずに取り直したか。
    pub refetched: bool,
}

/// 取得してセッションへ登録する。
pub fn fetch(app: &mut App, url: &url::Url) -> Result<Fetched, AppError> {
    let fetcher = crate::adapters::extract::HttpFetcher::new(
        std::sync::Arc::clone(&app.http),
        Some(std::sync::Arc::clone(&app.documents)),
        std::sync::Arc::clone(&app.clock),
        std::sync::Arc::clone(&app.reporter),
    );

    let id_hint = app.ensure_session()?;
    app.set_session(id_hint.clone());
    let mut guard = app.guard()?;
    guard.check(Metric::Fetches, 1)?;

    let options = FetchOptions {
        refresh: app.refresh,
        no_cache: app.no_cache,
        extract_mode: crate::core::convert::ExtractMode::Density,
        ttl_hours: app.settings.cache.ttl_hours,
        profile: app.network.clone(),
        max_chars: None,
        content_filters: app.filters.content_filters().cloned(),
        tracking: app.tracking.clone(),
    };

    let fetched = fetcher.fetch(url, &options)?;
    let chars = text::chars(&fetched.markdown);

    if let Some(max) = app.settings.cache.max_size_mb.checked_mul(1024 * 1024)
        && app.documents.total_bytes().unwrap_or(0) > max
    {
        app.reporter.warn(&Diagnostic::new(
            WarningCode::W5002,
            "状態ディレクトリの容量が上限を超えている（古いキャッシュを削除してください）"
                .to_string(),
        ));
    }

    guard.consume_all(Usage {
        fetches: if fetched.cached { 0 } else { 1 },
        chars: chars as u64,
        ..Default::default()
    })?;

    let mut session = app
        .sessions
        .load(&id_hint)?
        .unwrap_or_else(|| crate::core::session::SessionState::new(&id_hint, app.clock.now_utc()));

    let document = DocumentRef {
        id: 0,
        url: url.to_string(),
        final_url: fetched.meta.final_url.clone(),
        title: fetched.meta.title.clone(),
        fetched_at: fetched.meta.fetched_at.clone(),
        chars: fetched.meta.chars,
        headings: fetched.meta.headings,
        warnings: fetched.meta.warnings.clone(),
        cache_key: cache_key(url.as_str()),
    };
    let new = session.find_document(0).is_none();
    let id = session.register_document(document.clone());
    let mut document = document;
    document.id = id;
    session.touch(app.clock.now_utc());
    app.sessions.save(&session)?;

    // 履歴
    let entry = crate::core::history::HistoryEntry::new(
        app.history.next_id().unwrap_or_else(|_| "H?".to_string()),
        app.clock.now_utc(),
        id_hint,
        crate::core::history::EventType::Fetch,
        serde_json::json!({
            "url": crate::core::url::redact(url, &[]),
            "document": format!("D{id}"),
            "chars": chars,
            "cached": fetched.cached,
            "robotsSkipped": !app.network.robots.respect,
        }),
    );
    let _ = app.history.append(&entry);

    let warnings: Vec<Diagnostic> = fetched.warnings.clone();
    Ok(Fetched {
        document,
        new,
        warnings,
    })
}

/// セッション内の文書を読み込む（キャッシュ失効時は再取得）。
pub fn load(app: &mut App, document: &DocumentRef) -> Result<LoadedDocument, AppError> {
    let url = crate::core::url::parse(&document.url)?;
    let cached = app.documents.get(&url)?;
    if let Some((_, markdown)) = cached
        && !app.refresh
    {
        return Ok(LoadedDocument {
            document: document.clone(),
            markdown,
            refetched: false,
        });
    }
    let fetched = fetch(app, &url)?;
    let stored = app.documents.get(&url)?;
    let markdown = match stored {
        Some((_, md)) => md,
        None => {
            return Err(AppError::with_detail(
                ErrorCode::E3001,
                "本文を保存できなかった".to_string(),
            ));
        }
    };
    Ok(LoadedDocument {
        document: fetched.document,
        markdown,
        refetched: true,
    })
}

/// アウトライン（見出し一覧）。
#[derive(Debug, Clone)]
pub struct OutlineView {
    pub document: DocumentRef,
    pub headings: Vec<Heading>,
    pub depth: u8,
    pub hidden: usize,
    pub total_chars: usize,
    /// 見出しが 1 つも無い場合に本文全体を指す `S0` を出すか。
    pub whole_body: bool,
}

/// 見出しを抽出して表示用にまとめる。
pub fn outline(app: &mut App, document: &DocumentRef, depth: u8) -> Result<OutlineView, AppError> {
    let loaded = load(app, document)?;
    let markdown = filter_content(app, &loaded.markdown);
    let mut headings = outline::extract_headings(&markdown);
    // 再取得で見出し構成が変わった場合は知らせる（節 ID のずれに気付けるように）
    if loaded.refetched && loaded.document.headings != headings.len() {
        app.reporter.warn(&Diagnostic::new(
            WarningCode::W3004,
            format!(
                "D{} を再取得したところ見出し数が {} から {} に変化した",
                loaded.document.id,
                loaded.document.headings,
                headings.len()
            ),
        ));
    }
    let hidden = headings.iter().filter(|h| h.level > depth).count();
    for h in headings.iter_mut() {
        if h.level > depth {
            h.hidden = true;
        }
    }
    let total_chars = text::chars(&markdown);
    let whole_body = headings.is_empty();
    let entry = crate::core::history::HistoryEntry::new(
        app.history.next_id().unwrap_or_else(|_| "H?".to_string()),
        app.clock.now_utc(),
        app.current_session_id(),
        crate::core::history::EventType::Outline,
        serde_json::json!({
            "document": format!("D{}", document.id),
            "depth": depth,
            "headings": headings.len(),
            "hidden": hidden,
        }),
    );
    let _ = app.history.append(&entry);
    Ok(OutlineView {
        document: loaded.document,
        headings,
        depth,
        hidden,
        total_chars,
        whole_body,
    })
}

/// 節を選んで本文を組み立てる。
#[derive(Debug, Clone)]
pub struct ReadOutcome {
    pub sections: Vec<SectionBody>,
    pub total_chars: usize,
    pub budget: usize,
    pub truncated: bool,
}

/// 節を読む。
pub fn read(
    app: &mut App,
    document: &DocumentRef,
    section_ids: &[u32],
    include_children: bool,
) -> Result<ReadOutcome, AppError> {
    let loaded = load(app, document)?;
    let markdown = filter_content(app, &loaded.markdown);
    let headings = outline::extract_headings(&markdown);

    let budget = app.settings.context.max_chars;
    let mut bodies: Vec<SectionBody> = Vec::new();

    if headings.is_empty() || section_ids.contains(&0) {
        let raw = outline::whole_body(&markdown);
        let (text_out, truncated, total) = context::truncate_to_budget(&raw, budget);
        bodies.push(SectionBody {
            heading_id: 0,
            level: 1,
            heading: loaded.document.title.clone(),
            text: text_out,
            chars: total,
            truncated,
            next_heading_id: None,
            omitted_children: 0,
        });
    } else {
        // 包含関係の解決: 親が選ばれたら子は省略
        let mut selected: Vec<&Heading> = Vec::new();
        for id in section_ids {
            if let Some(h) = headings.iter().find(|h| h.id == *id) {
                selected.push(h);
            }
        }
        if selected.is_empty() {
            return Err(AppError::with_detail(
                ErrorCode::E1006,
                format!(
                    "節が見つからない（利用可能: {}）",
                    headings
                        .iter()
                        .take(10)
                        .map(|h| h.label())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ));
        }
        selected.sort_by_key(|h| h.start_line);
        let mut covered: Vec<(usize, usize)> = Vec::new();
        let mut omit: std::collections::HashMap<u32, usize> = Default::default();
        for h in &headings {
            if h.hidden {
                continue;
            }
        }
        for h in &selected {
            if covered
                .iter()
                .any(|(s, e)| h.start_line >= *s && h.start_line <= *e)
            {
                *omit.entry(h.id).or_insert(0) += 1;
                continue;
            }
            covered.push((h.start_line, h.end_line));
        }

        let sizes: Vec<usize> = selected
            .iter()
            .filter(|h| !omit.contains_key(&h.id))
            .map(|h| {
                let raw = outline::section_text(&markdown, h, include_children);
                text::chars(&raw)
            })
            .collect();
        let alloc = context::allocate(budget, &sizes);

        let mut alloc_iter = alloc.iter();
        for h in &selected {
            if omit.contains_key(&h.id) {
                continue;
            }
            let raw = outline::section_text(&markdown, h, include_children);
            let limit = alloc_iter.next().copied().unwrap_or(budget);
            let (body, truncated, total) = context::truncate_to_budget(&raw, limit);
            let children = outline::child_count(&headings, h);
            let omitted = if include_children { 0 } else { children };
            let next = next_heading_id(&headings, h);
            bodies.push(SectionBody {
                heading_id: h.id,
                level: h.level,
                heading: h.text.clone(),
                text: body,
                chars: total,
                truncated,
                next_heading_id: next,
                omitted_children: omitted,
            });
        }
    }

    let total_chars: usize = bodies.iter().map(|b| text::chars(&b.text)).sum();
    let truncated = bodies.iter().any(|b| b.truncated);

    if let Some(first) = bodies.first() {
        let entry = crate::core::history::HistoryEntry::new(
            app.history.next_id().unwrap_or_else(|_| "H?".to_string()),
            app.clock.now_utc(),
            app.current_session_id(),
            crate::core::history::EventType::Read,
            serde_json::json!({
                "document": format!("D{}", document.id),
                "section": format!("D{}:S{}", document.id, first.heading_id),
                "heading": first.heading.clone(),
                "chars": total_chars,
                "truncated": truncated,
            }),
        );
        let _ = app.history.append(&entry);
    }

    let mut guard = app.guard()?;
    guard.consume_all(Usage {
        chars: total_chars as u64,
        ..Default::default()
    })?;

    Ok(ReadOutcome {
        sections: bodies,
        total_chars,
        budget,
        truncated,
    })
}

fn next_heading_id(headings: &[Heading], current: &Heading) -> Option<u32> {
    headings
        .iter()
        .find(|h| h.start_line > current.start_line && h.level <= current.level && !h.hidden)
        .map(|h| h.id)
}

fn filter_content(app: &App, markdown: &str) -> String {
    let (text_out, _stats) = apply_content(&app.filters, markdown, app.no_filter);
    text_out
}

/// 選択を記録する。
pub fn record_selection(app: &mut App, selections: &[(u32, u32)]) -> Result<(), AppError> {
    let id = app.ensure_session()?;
    let mut session = app
        .sessions
        .load(&id)?
        .unwrap_or_else(|| crate::core::session::SessionState::new(&id, app.clock.now_utc()));
    let mut order = session
        .selections
        .iter()
        .map(|s| s.order)
        .max()
        .unwrap_or(0);
    for (document, section) in selections {
        order += 1;
        session.selections.push(Selection {
            document: *document,
            section: *section,
            order,
        });
    }
    session.touch(app.clock.now_utc());
    app.sessions.save(&session)?;
    let entry = crate::core::history::HistoryEntry::new(
        app.history.next_id().unwrap_or_else(|_| "H?".to_string()),
        app.clock.now_utc(),
        id,
        crate::core::history::EventType::Select,
        serde_json::json!({
            "items": selections
                .iter()
                .map(|(d, s)| format!("D{d}:S{s}"))
                .collect::<Vec<_>>(),
        }),
    );
    let _ = app.history.append(&entry);
    Ok(())
}

/// 節本文の出力ヘッダ。
pub fn section_header(document: &DocumentRef, body: &SectionBody) -> String {
    format!(
        "--- D{}:S{} | {} | h{} | {} 字 ---",
        document.id, body.heading_id, body.heading, body.level, body.chars
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_contains_ids() {
        let doc = DocumentRef {
            id: 1,
            title: "t".into(),
            ..Default::default()
        };
        let body = SectionBody {
            heading_id: 3,
            level: 2,
            heading: "見出し".into(),
            text: "x".into(),
            chars: 10,
            truncated: false,
            next_heading_id: None,
            omitted_children: 0,
        };
        let header = section_header(&doc, &body);
        assert!(header.contains("D1:S3"));
        assert!(header.contains("h2"));
    }

    #[test]
    fn next_heading_walks_siblings() {
        let md = "# a\n\n## b\n\ntext\n\n## c\n";
        let headings = outline::extract_headings(md);
        let next = next_heading_id(&headings, &headings[1]);
        assert_eq!(next, Some(3));
    }
}
