//! 検索のユースケース。

use crate::core::config::quota::{Metric, Usage};
use crate::core::engine::{EngineDef, PagingStyle};
use crate::core::errors::{AppError, Diagnostic, ErrorCode, WarningCode};
use crate::core::model::SearchHit;
use crate::core::search as search_core;
use crate::core::secret::Secrets;
use crate::core::session::{PageState, QueryRecord};
use crate::core::template::{self, TmplCtx};
use crate::core::text;
use crate::ports::state::HistoryQuery;
use crate::usecases::App;

/// 1 回の検索結果。
#[derive(Debug, Clone)]
pub struct SearchOutcome {
    pub hits: Vec<SearchHit>,
    pub engine: String,
    pub fallback_from: Option<String>,
    pub filtered: crate::core::config::filter::FilterStats,
    pub truncated_by_limit: usize,
    pub page: PageState,
    pub has_more: bool,
    pub warnings: Vec<Diagnostic>,
}

/// 検索語を正規化し、空を弾く。
pub fn prepare_query(input: &str) -> Result<(String, Vec<Diagnostic>), AppError> {
    let (query, truncated) = text::normalize_query(input);
    if query.is_empty() {
        return Err(AppError::new(ErrorCode::E1007));
    }
    let mut warnings = Vec::new();
    if truncated {
        warnings.push(Diagnostic::new(
            WarningCode::W1003,
            format!(
                "検索語が上限を超えたため {} 文字に切り詰めた（入力 {} 文字）",
                crate::core::text::chars(&query),
                crate::core::text::chars(input)
            ),
        ));
    }
    Ok((query, warnings))
}

/// 検索を実行する（ページ指定あり）。
pub fn run(app: &mut App, query: &str, page: u32) -> Result<SearchOutcome, AppError> {
    let (query, mut warnings) = prepare_query(query)?;
    let limit = app.settings.search.limit.max(1);
    let limit_u32 = limit as u32;

    // クォータ確認（送信前に判定し、成功後に消費する）
    let mut guard = app.guard()?;
    guard.check(Metric::Searches, 1)?;

    let candidates = candidate_engines(app)?;
    let mut last_error: Option<AppError> = None;
    let mut used_fallback: Option<String> = None;

    for (index, selector) in candidates.iter().enumerate() {
        let Some(def) = app.engines.get(selector) else {
            continue;
        };
        match call_engine(app, def, &query, page, limit_u32) {
            Ok(call) => {
                let engine_call = call;
                let hits = engine_call.outcome.hits;
                if index > 0 {
                    used_fallback = Some(candidates[0].clone());
                }
                if engine_call.outcome.items_seen == 0 && hits.is_empty() {
                    return Err(AppError::with_detail(
                        ErrorCode::E4001,
                        format!("{query} / {}", def.selector()),
                    ));
                }

                let (deduped, removed) = search_core::dedup(hits);
                let raw_hits = deduped;
                if removed > 0 {
                    warnings.push(Diagnostic::new(
                        WarningCode::W4005,
                        format!("重複 URL を {removed} 件除去した"),
                    ));
                }

                let now = app.clock.today_local();
                let engines: Vec<String> = vec![def.selector(); raw_hits.len()];
                let assigned = search_core::assign_ids(raw_hits, 1, &engines);
                let (kept, filtered) = crate::core::config::filter::apply_results(
                    &app.filters,
                    assigned,
                    now,
                    app.no_filter,
                );
                let truncated_by_limit = kept.len().saturating_sub(limit);
                let page_hits: Vec<SearchHit> = kept.into_iter().take(limit).collect();

                let has_more = page_hits.len() == limit
                    && (page as usize) * limit < def.max_results(app.settings.search.max_results);

                // 消費量の確定（クォータ）
                let usage = Usage {
                    searches: 1,
                    results: page_hits.len() as u64,
                    chars: page_hits
                        .iter()
                        .map(|h| {
                            (text::chars(&h.title)
                                + h.snippet.as_deref().map(text::chars).unwrap_or(0))
                                as u64
                        })
                        .sum(),
                    ..Default::default()
                };
                guard.consume_all(usage)?;

                // 履歴
                let entry = crate::core::history::HistoryEntry::new(
                    app.history.next_id().unwrap_or_else(|_| "H?".to_string()),
                    app.clock.now_utc(),
                    app.current_session_id(),
                    crate::core::history::EventType::Search,
                    serde_json::json!({
                        "query": query,
                        "page": page,
                        "results": page_hits.len(),
                        "filtered": filtered.removed,
                    }),
                );
                let _ = app.history.append(&entry);

                let page_state = PageState {
                    page,
                    next_page: page + 1,
                    exhausted: !has_more,
                };

                if let Some(from) = &used_fallback {
                    warnings.push(Diagnostic::new(
                        WarningCode::W4003,
                        format!("{from} の代わりに {} を使用した", def.selector()),
                    ));
                }
                for w in &engine_call.outcome.warnings {
                    warnings.push(Diagnostic::new(WarningCode::W4001, w.clone()));
                }
                if engine_call.truncated {
                    warnings.push(Diagnostic::new(
                        WarningCode::W3003,
                        "応答サイズの上限で打ち切った".to_string(),
                    ));
                }

                return Ok(SearchOutcome {
                    hits: page_hits,
                    engine: def.selector(),
                    fallback_from: used_fallback,
                    filtered,
                    truncated_by_limit,
                    page: page_state,
                    has_more,
                    warnings,
                });
            }
            Err(err) => {
                let class = crate::ports::classify(&err);
                if class == crate::ports::FallbackClass::RetryOtherEngine
                    && index + 1 < candidates.len()
                {
                    last_error = Some(err);
                    continue;
                }
                return Err(err);
            }
        }
    }
    Err(last_error.unwrap_or_else(|| AppError::new(ErrorCode::E4001)))
}

fn candidate_engines(app: &App) -> Result<Vec<String>, AppError> {
    let mut out = Vec::new();
    if app.settings.search.merge {
        out.extend(app.settings.search.engines.clone());
        if out.is_empty() {
            out.push(app.default_engine.clone());
        }
    } else {
        out.push(app.default_engine.clone());
        for fb in &app.settings.search.fallback {
            if !out.contains(fb) {
                out.push(fb.clone());
            }
        }
    }
    if out.is_empty() {
        return Err(AppError::new(ErrorCode::E4002));
    }
    Ok(out)
}

fn call_engine(
    app: &App,
    def: &EngineDef,
    query: &str,
    page: u32,
    limit: u32,
) -> Result<crate::adapters::engine::eval::EngineCall, AppError> {
    let (offset, page_number) = template::paging_values(def.paging.as_ref(), page, limit);
    let ctx = TmplCtx {
        query: query.to_string(),
        query_raw: query.to_string(),
        offset,
        page: page_number,
        limit: def.limits.max_page_size.min(limit),
        locale: def
            .defaults
            .locale
            .clone()
            .unwrap_or_else(|| app.settings.ui.locale.clone()),
        region: def.defaults.region.clone().unwrap_or_default(),
        api_key: None,
        date_from: String::new(),
        date_to: String::new(),
    };
    // API キーは定義の `value_env` から解決する（値はここでは保持しない）
    let secrets: Secrets = app.secrets.clone();
    let profile = engine_profile(app, def);
    crate::adapters::engine::evaluate(
        app.http.as_ref(),
        &profile,
        def,
        &ctx,
        &secrets,
        &app.tracking,
    )
}

/// エンジンが参照するネットワークプロファイル。
pub fn engine_profile(app: &App, def: &EngineDef) -> crate::core::config::NetworkProfile {
    match &def.network {
        Some(id) if id != &app.network.id => {
            let dirs = crate::adapters::config::resolve_resource_dir(
                app.env.as_ref(),
                &app.paths.root.join("config"),
                "network",
                &app.bundled,
            );
            match crate::adapters::config::find_resource(&dirs, id) {
                Some(path) => crate::adapters::config::read_json_file(&path)
                    .ok()
                    .and_then(|v| serde_json::from_value(v).ok())
                    .unwrap_or_else(|| app.network.clone()),
                None => app.network.clone(),
            }
        }
        _ => app.network.clone(),
    }
}

/// 次ページを取得する。
pub fn next(app: &mut App, query: &str, current: &PageState) -> Result<SearchOutcome, AppError> {
    if current.exhausted {
        return Err(AppError::with_detail(
            ErrorCode::E4001,
            "これ以上の結果はない".to_string(),
        ));
    }
    run(app, query, current.next_page)
}

/// 検索語を変更して再検索する。
pub fn refine(app: &mut App, previous: &str, new_query: &str) -> Result<SearchOutcome, AppError> {
    let mut outcome = run(app, new_query, 1)?;
    let entry = crate::core::history::HistoryEntry::new(
        app.history.next_id().unwrap_or_else(|_| "H?".to_string()),
        app.clock.now_utc(),
        app.current_session_id(),
        crate::core::history::EventType::Refine,
        serde_json::json!({"query": new_query, "previous": previous}),
    );
    let _ = app.history.append(&entry);
    outcome.warnings.push(Diagnostic::new(
        WarningCode::W4001,
        format!("検索語を '{previous}' から '{new_query}' に変更した"),
    ));
    Ok(outcome)
}

/// 検索結果をセッションへ反映する。
pub fn apply_to_session(
    app: &mut App,
    outcome: &SearchOutcome,
    query: &str,
) -> Result<crate::core::session::SessionState, AppError> {
    let id = app.ensure_session()?;
    let mut session = app
        .sessions
        .load(&id)?
        .unwrap_or_else(|| crate::core::session::SessionState::new(&id, app.clock.now_utc()));

    if outcome.page.page == 1 {
        session.results.clear();
        session.queries.push(QueryRecord {
            query: query.to_string(),
            engine: outcome.engine.clone(),
            at: app.clock.now_utc().to_rfc3339(),
            results: outcome.hits.len(),
            filtered: outcome.filtered.removed,
        });
        session.active_query = session.queries.len().saturating_sub(1);
    }
    let start = session.next_result_id();
    let engines: Vec<String> = outcome.hits.iter().map(|h| h.engine.clone()).collect();
    let mut hits = outcome.hits.clone();
    for (i, hit) in hits.iter_mut().enumerate() {
        hit.id = start + i as u32;
        hit.rank = start + i as u32;
    }
    let _ = engines;
    session.results.extend(hits);
    session.page = outcome.page.clone();
    session.last_query = query.to_string();
    session.last_engine = outcome.engine.clone();
    session.touch(app.clock.now_utc());
    app.sessions.save(&session)?;
    Ok(session)
}

/// 履歴から検索の再実行候補を出す。
pub fn recent_queries(app: &App, limit: usize) -> Vec<String> {
    let q = HistoryQuery {
        limit: Some(limit),
        kinds: vec!["search".into()],
        ..Default::default()
    };
    let mut out = Vec::new();
    if let Ok(entries) = app.history.query(&q) {
        for e in entries {
            if let Some(query) = e.payload.get("query").and_then(|v| v.as_str())
                && !out.contains(&query.to_string())
            {
                out.push(query.to_string());
            }
        }
    }
    out
}

/// ページング様式の表示名（`engines show` 用）。
pub fn paging_label(def: &EngineDef) -> String {
    match &def.paging {
        None => "なし".to_string(),
        Some(p) => match p.style {
            PagingStyle::Offset => format!("offset（{} 件ずつ）", p.page_size.unwrap_or(10)),
            PagingStyle::Page => format!("page（{} 件ずつ）", p.page_size.unwrap_or(10)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_is_an_error() {
        assert!(prepare_query("   ").is_err());
    }

    #[test]
    fn long_query_is_truncated_with_warning() {
        let long = "あ".repeat(600);
        let (query, warnings) = prepare_query(&long).unwrap();
        assert_eq!(crate::core::text::chars(&query), 512);
        assert_eq!(warnings.len(), 1);
    }
}
