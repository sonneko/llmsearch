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
use crate::usecases::guard::Guard;

/// 1 回の検索結果。
#[derive(Debug, Clone)]
pub struct SearchOutcome {
    /// 正規化済みの検索語（JSON の `query` に使う）。
    pub query: String,
    pub hits: Vec<SearchHit>,
    pub engine: String,
    pub fallback_from: Option<String>,
    pub filtered: crate::core::config::filter::FilterStats,
    pub truncated_by_limit: usize,
    pub page: PageState,
    pub has_more: bool,
    /// `next` が終了した理由（`maxResults` / `emptyPage` / `filtered` など）。
    pub termination_reason: Option<String>,
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
                "query exceeded the length limit and was truncated to {} chars (input was {} chars)",
                crate::core::text::chars(&query),
                crate::core::text::chars(input)
            ),
        ));
    }
    Ok((query, warnings))
}

/// 検索を実行する（ページ指定あり）。
///
/// `search.merge=true` のときは全エンジンへ送ってラウンドロビン統合し、
/// そうでなければフォールバックの連鎖として実行する。
pub fn run(app: &mut App, query: &str, page: u32) -> Result<SearchOutcome, AppError> {
    run_with_event(
        app,
        query,
        page,
        crate::core::history::EventType::Search,
        true,
    )
}

/// 明示的なセッションを持たない診断検索（`engines test`）を実行する。
///
/// 既存セッションは引き継ぐが、存在しない場合に架空のセッション ID は作らない。
pub fn run_without_session(
    app: &mut App,
    query: &str,
    page: u32,
) -> Result<SearchOutcome, AppError> {
    run_with_event(
        app,
        query,
        page,
        crate::core::history::EventType::Search,
        false,
    )
}

/// 1 つのエンジン定義を試す。診断操作では fallback / merge や架空セッションを使わない。
pub fn run_engine_test(
    app: &mut App,
    selector: &str,
    query: &str,
) -> Result<SearchOutcome, AppError> {
    if app.engines.get(selector).is_none() {
        return Err(AppError::with_detail(
            ErrorCode::E4002,
            format!("engine '{selector}' was not found"),
        ));
    }

    let original_search = app.settings.search.clone();
    let original_engine = app.default_engine.clone();
    app.settings.search.merge = false;
    app.settings.search.engines.clear();
    app.settings.search.fallback.clear();
    app.default_engine = selector.to_string();
    let result = run_without_session(app, query, 1);
    app.settings.search = original_search;
    app.default_engine = original_engine;
    result
}

fn run_with_event(
    app: &mut App,
    query: &str,
    page: u32,
    event: crate::core::history::EventType,
    create_session: bool,
) -> Result<SearchOutcome, AppError> {
    let (query, warnings) = prepare_query(query)?;

    if create_session && app.current_session_id().is_empty() {
        // `search` は `--session` なしなら新しいセッションを開始する。
        let session_id = app.ensure_session()?;
        app.set_session(session_id);
    }

    let result = if app.settings.search.merge {
        run_merge(app, &query, page, warnings, event)
    } else {
        run_fallback(app, &query, page, warnings, event)
    };
    if event == crate::core::history::EventType::Next
        && result
            .as_ref()
            .is_err_and(|error| error.code == ErrorCode::E5005)
    {
        record_next_termination(app, &query, page, "quota");
    }
    result
}

fn record_next_termination(app: &App, query: &str, page: u32, reason: &str) {
    let limit = app.settings.search.limit.max(1);
    let logical_offset = (page.saturating_sub(1) as usize).saturating_mul(limit);
    let offset = if app.settings.search.merge {
        logical_offset
    } else {
        app.engines
            .get(&app.default_engine)
            .map(|def| match def.paging.as_ref() {
                Some(paging) if paging.style == PagingStyle::Offset => {
                    let request_limit = def.limits.max_page_size.min(limit as u32).max(1);
                    template::paging_values(Some(paging), page, request_limit).0 as usize
                }
                _ => logical_offset,
            })
            .unwrap_or(logical_offset)
    };
    let engine = if app.settings.search.merge {
        app.settings.search.engines.join("+")
    } else {
        app.default_engine.clone()
    };
    let mut entry = crate::core::history::HistoryEntry::new(
        app.history.next_id().unwrap_or_else(|_| "H?".to_string()),
        app.clock.now_utc(),
        app.current_session_id(),
        crate::core::history::EventType::Next,
        serde_json::json!({
            "query": query,
            "page": page,
            "offset": offset,
            "added": 0,
            "filtered": 0,
            "engine": engine,
            "hasMore": false,
            "terminationReason": reason,
        }),
    );
    entry.engine = Some(engine);
    let _ = app.history.append(&entry);
}

/// 1 エンジン分の検索（正規化・重複除去まで済ませた生の結果）。
struct EngineOutcome {
    def: EngineDef,
    /// エンジン定義の累積上限（`limits.maxResults` / `paging.maxResults`）。
    max_results: usize,
    hits: Vec<search_core::RawHit>,
    /// フィルタ前の取得件数。
    obtained: usize,
    /// 取得範囲の先頭位置（0 始まり）。
    page_start: usize,
    /// URL 重複として正規化段階で取り除いた件数。
    deduplicated: usize,
    /// 最後に取得した provider page が満杯で、次の provider page がありうるか。
    has_more: bool,
    warnings: Vec<Diagnostic>,
}

/// エンジンを呼び、重複除去・リダイレクト展開を行う。
///
/// `page` は利用者が見る論理ページ。page-style エンジンでは `pageSize` と
/// `search.limit` が異なる場合に隣接する provider page を必要な分だけ取得し、
/// 論理ページの範囲を切り出す。各 provider request と抽出 retry は個別に quota-check する。
fn search_once(
    app: &App,
    def: &EngineDef,
    query: &str,
    page: u32,
    limit: u32,
    guard: &Guard,
    pending_searches: &mut u64,
) -> Result<EngineOutcome, AppError> {
    let max_results = def.max_results(app.settings.search.max_results);
    let mut warnings = Vec::new();
    let mut hits = Vec::new();
    let retrieved;
    let page_start;
    let deduplicated;
    let mut has_more;

    if let Some(paging) = def.paging.as_ref().filter(|p| p.style == PagingStyle::Page) {
        let native_size = paging
            .page_size
            .unwrap_or(limit)
            .min(def.limits.max_page_size)
            .max(1) as usize;
        let start = (page.saturating_sub(1) as usize).saturating_mul(limit as usize);
        let end = start.saturating_add(limit as usize).min(max_results);
        let relative_start = start % native_size;
        let mut provider_has_more = false;

        if end > start {
            let first_provider_page = start / native_size + 1;
            let last_provider_page = end.saturating_sub(1) / native_size + 1;
            for provider_page in first_provider_page..=last_provider_page {
                let call = call_engine(
                    app,
                    def,
                    query,
                    provider_page as u32,
                    native_size as u32,
                    guard,
                    pending_searches,
                )?;
                warnings.extend(call.outcome.warnings.iter().cloned());
                let items_seen = call.outcome.items_seen;
                if items_seen == 0 && call.outcome.hits.is_empty() {
                    if hits.is_empty() && page == 1 {
                        return Err(AppError::with_detail(
                            ErrorCode::E4001,
                            format!("{query} / {}", def.selector()),
                        ));
                    }
                    provider_has_more = false;
                    break;
                }
                hits.extend(call.outcome.hits);
                if items_seen < native_size {
                    // A short provider page is the end of this engine's result list.
                    provider_has_more = false;
                    break;
                }
                provider_has_more = provider_page == last_provider_page;
            }
        }

        let (normalized, mut normalization_warnings, duplicates) = normalize_hits(hits);
        warnings.append(&mut normalization_warnings);
        deduplicated = duplicates;
        retrieved = normalized.len();
        hits = normalized
            .into_iter()
            .skip(relative_start)
            .take(limit as usize)
            .collect();
        page_start = start;
        has_more = provider_has_more;
    } else {
        let request_limit = def.limits.max_page_size.min(limit).max(1);
        let call = call_engine(
            app,
            def,
            query,
            page,
            request_limit,
            guard,
            pending_searches,
        )?;
        warnings.extend(call.outcome.warnings.iter().cloned());
        if call.outcome.items_seen == 0 && call.outcome.hits.is_empty() && page == 1 {
            return Err(AppError::with_detail(
                ErrorCode::E4001,
                format!("{query} / {}", def.selector()),
            ));
        }
        let requested_size = request_limit as usize;
        has_more = def.paging.is_some() && call.outcome.items_seen >= requested_size;
        let (normalized, mut normalization_warnings, duplicates) =
            normalize_hits(call.outcome.hits);
        warnings.append(&mut normalization_warnings);
        deduplicated = duplicates;
        retrieved = normalized.len();
        hits = normalized;
        page_start = match def.paging.as_ref() {
            Some(paging) if paging.style == PagingStyle::Offset => {
                crate::core::template::paging_values(Some(paging), page, request_limit).0 as usize
            }
            _ => (page.saturating_sub(1) as usize).saturating_mul(limit as usize),
        };
    }

    // Enforce this engine's cumulative cap before merge combines it with other engines.
    hits.truncate(max_results.saturating_sub(page_start));
    has_more = has_more && !hits.is_empty() && page_start.saturating_add(hits.len()) < max_results;

    Ok(EngineOutcome {
        def: def.clone(),
        max_results,
        obtained: retrieved,
        page_start,
        deduplicated,
        has_more,
        hits,
        warnings,
    })
}

fn normalize_hits(
    hits: Vec<search_core::RawHit>,
) -> (Vec<search_core::RawHit>, Vec<Diagnostic>, usize) {
    let mut warnings = Vec::new();
    // 抽出でリダイレクトを解けなかった結果をここで解く（取得時に
    // エンジン側のページを読んでしまうのを防ぐ）。
    let (hits, unwrapped, example) = search_core::unwrap_redirects(hits);
    if unwrapped > 0 {
        warnings.push(Diagnostic::new(
            WarningCode::W4006,
            match &example {
                Some(first) => {
                    format!("unwrapped {unwrapped} engine redirect URLs (e.g. {first})")
                }
                None => format!("unwrapped {unwrapped} engine redirect URLs"),
            },
        ));
    }
    let (deduped, removed) = search_core::dedup(hits);
    if removed > 0 {
        warnings.push(Diagnostic::new(
            WarningCode::W4005,
            format!("removed {removed} duplicate URLs"),
        ));
    }
    (deduped, warnings, removed)
}

/// フィルタを適用し、1 ページ分に切り出して結果を組み立てる。
#[allow(clippy::too_many_arguments)]
fn finish(
    app: &App,
    query: &str,
    page: u32,
    limit: usize,
    page_start: usize,
    supports_paging: bool,
    provider_has_more: bool,
    deduplicated: usize,
    raw: Vec<search_core::RawHit>,
    owners: Vec<String>,
    max_results: usize,
    engine: String,
    fallback_from: Option<String>,
    mut warnings: Vec<Diagnostic>,
) -> SearchOutcome {
    let now = app.clock.today_local();
    // `maxResults` is a cumulative provider cap. A page starting near that cap
    // must not expose rows beyond it even if the provider returned a full page.
    let raw: Vec<_> = raw
        .into_iter()
        .take(max_results.saturating_sub(page_start))
        .collect();
    // Filtering may reduce the displayed count, but must not reopen a page after the
    // provider result cap has been consumed.
    let consumed = page_start.saturating_add(raw.len());
    let assigned = search_core::assign_ids(raw, 1, &owners);
    let (mut kept, mut filtered) =
        crate::core::config::filter::apply_results(&app.filters, assigned, now, app.no_filter);
    filtered.record_count("duplicateUrl", deduplicated);

    let session_id = app.current_session_id();
    let current_session = if session_id.is_empty() {
        None
    } else {
        app.sessions.load(&session_id).ok().flatten()
    };
    let same_query = current_session
        .as_ref()
        .and_then(|s| s.queries.last())
        .is_some_and(|record| record.query == query);
    let start_id = if page == 1 {
        1
    } else if same_query {
        current_session
            .as_ref()
            .map(|s| s.next_result_id())
            .unwrap_or_else(|| page_start.saturating_add(1) as u32)
    } else {
        page_start.saturating_add(1) as u32
    };
    for (index, hit) in kept.iter_mut().enumerate() {
        let id = start_id.saturating_add(index as u32);
        hit.id = id;
        hit.rank = id;
    }

    let truncated_by_limit = kept.len().saturating_sub(limit);
    let page_hits: Vec<SearchHit> = kept.into_iter().take(limit).collect();
    let has_more =
        supports_paging && provider_has_more && !page_hits.is_empty() && consumed < max_results;
    let termination_reason = if has_more {
        None
    } else if !supports_paging {
        Some("pagingUnsupported".to_string())
    } else if consumed >= max_results {
        Some("maxResults".to_string())
    } else if page_hits.is_empty()
        && (filtered.removed > 0 || filtered.truncated_by_max_results > 0)
    {
        Some("filtered".to_string())
    } else if page_hits.is_empty() || !provider_has_more {
        Some("emptyPage".to_string())
    } else {
        Some("maxResults".to_string())
    };

    if let Some(from) = &fallback_from {
        warnings.push(Diagnostic::new(
            WarningCode::W4003,
            format!("used {engine} instead of {from}"),
        ));
    }

    SearchOutcome {
        query: query.to_string(),
        hits: page_hits,
        engine,
        fallback_from,
        filtered,
        truncated_by_limit,
        page: PageState {
            page,
            next_page: page.saturating_add(1),
            exhausted: !has_more,
        },
        has_more,
        termination_reason,
        warnings,
    }
}

/// フォールバックの連鎖（`search.merge=false`）。
fn run_fallback(
    app: &mut App,
    query: &str,
    page: u32,
    mut warnings: Vec<Diagnostic>,
    event: crate::core::history::EventType,
) -> Result<SearchOutcome, AppError> {
    let limit = app.settings.search.limit.max(1);
    let limit_u32 = limit as u32;
    let mut guard = app.guard()?;

    let candidates = candidate_engines(app)?;
    let mut last_error: Option<AppError> = None;
    let mut used_fallback: Option<String> = None;
    // 各ネットワーク送信前に quota-check して回数を積む。
    // 操作が成功したときだけ `consume_all` で永続化する（仕様 quotas.md §3.2）。
    let mut pending = Usage::default();

    for (index, selector) in candidates.iter().enumerate() {
        let Some(def) = app.engines.get(selector) else {
            continue;
        };
        match search_once(
            app,
            def,
            query,
            page,
            limit_u32,
            &guard,
            &mut pending.searches,
        ) {
            Ok(call) => {
                if index > 0 {
                    used_fallback = Some(candidates[0].clone());
                }
                // `searches` は送信直前の preflight で回数を積み、
                // `results` は成功した取得件数（フィルタ前）を記録する。
                pending.results += call.obtained as u64;
                guard.consume_all(pending)?;

                warnings.extend(call.warnings.iter().cloned());
                let engine = call.def.selector();
                let page_start = call.page_start;
                let supports_paging = call.def.paging.is_some();
                let owners: Vec<String> = vec![engine.clone(); call.hits.len()];
                let outcome = finish(
                    app,
                    query,
                    page,
                    limit,
                    page_start,
                    supports_paging,
                    call.has_more,
                    call.deduplicated,
                    call.hits,
                    owners,
                    call.max_results,
                    engine.clone(),
                    used_fallback.clone(),
                    warnings,
                );

                // 履歴には Search と Next を別種のイベントとして記録する。
                let payload = if event == crate::core::history::EventType::Next {
                    serde_json::json!({
                        "query": query,
                        "page": page,
                        "offset": page_start,
                        "added": outcome.hits.len(),
                        "filtered": outcome.filtered.removed,
                        "engine": engine.clone(),
                        "hasMore": outcome.has_more,
                        "terminationReason": outcome.termination_reason.clone(),
                    })
                } else {
                    serde_json::json!({
                        "query": query,
                        "page": page,
                        "results": outcome.hits.len(),
                        "filtered": outcome.filtered.removed,
                        "engine": engine.clone(),
                    })
                };
                let mut entry = crate::core::history::HistoryEntry::new(
                    app.history.next_id().unwrap_or_else(|_| "H?".to_string()),
                    app.clock.now_utc(),
                    app.current_session_id(),
                    event,
                    payload,
                );
                entry.engine = Some(engine.clone());
                let _ = app.history.append(&entry);
                return Ok(outcome);
            }
            Err(err) => {
                let class = crate::ports::classify(&err);
                if class == crate::ports::FallbackClass::RetryOtherEngine
                    && index + 1 < candidates.len()
                {
                    last_error = Some(err);
                    continue;
                }
                // 操作全体が失敗したので `pending` は捨てる（消費しない）。
                return Err(err);
            }
        }
    }
    Err(last_error.unwrap_or_else(|| AppError::new(ErrorCode::E4001)))
}

/// ラウンドロビン統合（`search.merge=true`。仕様 engine-definitions.md §7.4）。
fn run_merge(
    app: &mut App,
    query: &str,
    page: u32,
    mut warnings: Vec<Diagnostic>,
    event: crate::core::history::EventType,
) -> Result<SearchOutcome, AppError> {
    let limit = app.settings.search.limit.max(1);
    let limit_u32 = limit as u32;
    let selectors = app.settings.search.engines.clone();
    if selectors.is_empty() {
        // `Settings::validate()` が先に `E1004` とするが、防御的に弾く
        return Err(AppError::with_detail(
            ErrorCode::E4002,
            "search.merge = true but search.engines is empty".to_string(),
        ));
    }

    let mut guard = app.guard()?;

    let mut pending = Usage::default();
    let mut lists: Vec<Vec<search_core::RawHit>> = Vec::new();
    let mut owners: Vec<String> = Vec::new();
    let mut deduplicated = 0usize;
    let mut provider_has_more = false;
    let mut first_error: Option<AppError> = None;

    for selector in &selectors {
        let Some(def) = app.engines.get(selector) else {
            warnings.push(Diagnostic::new(
                WarningCode::W4004,
                format!("engine '{selector}' was not found and is excluded from the merge"),
            ));
            continue;
        };
        match search_once(
            app,
            def,
            query,
            page,
            limit_u32,
            &guard,
            &mut pending.searches,
        ) {
            Ok(call) => {
                pending.results += call.obtained as u64;
                warnings.extend(call.warnings.iter().cloned());
                deduplicated = deduplicated.saturating_add(call.deduplicated);
                provider_has_more |= call.has_more;
                owners.push(call.def.selector());
                lists.push(call.hits);
            }
            Err(err) => {
                if err.code == ErrorCode::E5005 {
                    return Err(err);
                }
                warnings.push(Diagnostic::new(
                    WarningCode::W4004,
                    format!(
                        "{} failed and is excluded from the merge ({}: {})",
                        def.selector(),
                        err.code.as_str(),
                        err.detail.clone().unwrap_or_default()
                    ),
                ));
                if first_error.is_none() {
                    first_error = Some(err);
                }
            }
        }
    }

    if lists.iter().all(|l| l.is_empty()) {
        // 操作全体が失敗したので消費しない（仕様 quotas.md §3.2）
        return Err(first_error.unwrap_or_else(|| AppError::new(ErrorCode::E4001)));
    }

    let (merged, merged_owners, removed) = search_core::merge_round_robin(lists, &owners);
    if removed > 0 {
        warnings.push(Diagnostic::new(
            WarningCode::W4005,
            format!("removed {removed} duplicate URLs while merging"),
        ));
        deduplicated = deduplicated.saturating_add(removed);
    }

    guard.consume_all(pending)?;

    let engine = selectors.join("+");
    let supports_paging = selectors.iter().any(|selector| {
        app.engines
            .get(selector)
            .is_some_and(|def| def.paging.is_some())
    });
    let page_start = (page.saturating_sub(1) as usize).saturating_mul(limit);
    let outcome = finish(
        app,
        query,
        page,
        limit,
        page_start,
        supports_paging,
        provider_has_more,
        deduplicated,
        merged,
        merged_owners,
        app.settings.search.max_results,
        engine.clone(),
        None,
        warnings,
    );

    let payload = if event == crate::core::history::EventType::Next {
        serde_json::json!({
            "query": query,
            "page": page,
            "offset": page_start,
            "added": outcome.hits.len(),
            "filtered": outcome.filtered.removed,
            "engine": engine.clone(),
            "hasMore": outcome.has_more,
            "terminationReason": outcome.termination_reason.clone(),
            "merged": true,
            "engines": selectors,
        })
    } else {
        serde_json::json!({
            "query": query,
            "page": page,
            "results": outcome.hits.len(),
            "filtered": outcome.filtered.removed,
            "engine": engine.clone(),
            "merged": true,
            "engines": selectors,
        })
    };
    let mut entry = crate::core::history::HistoryEntry::new(
        app.history.next_id().unwrap_or_else(|_| "H?".to_string()),
        app.clock.now_utc(),
        app.current_session_id(),
        event,
        payload,
    );
    entry.engine = Some(engine.clone());
    let _ = app.history.append(&entry);
    Ok(outcome)
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
    guard: &Guard,
    pending_searches: &mut u64,
) -> Result<crate::adapters::engine::eval::EngineCall, AppError> {
    let (offset, page_number) = template::paging_values(def.paging.as_ref(), page, limit);
    let ctx = TmplCtx {
        query: query.to_string(),
        query_raw: query.to_string(),
        offset,
        page: page_number,
        limit: def.limits.max_page_size.min(limit),
        language: app.language().primary.clone(),
        locale: app.language().tag.clone(),
        region: def.defaults.region.clone().unwrap_or_default(),
        api_key: None,
        date_from: String::new(),
        date_to: String::new(),
    };
    // API キーは定義の `value_env` から解決する（値はここでは保持しない）
    let secrets: Secrets = app.secrets.clone();
    let profile = engine_profile(app, def);
    crate::adapters::engine::evaluate_with_preflight(
        app.clock.as_ref(),
        app.http.as_ref(),
        &profile,
        def,
        &ctx,
        &secrets,
        &app.tracking,
        || {
            let requested = (*pending_searches).saturating_add(1);
            guard.check(Metric::Searches, requested)?;
            *pending_searches = requested;
            Ok(())
        },
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
            "no more results".to_string(),
        ));
    }
    run_with_event(
        app,
        query,
        current.next_page,
        crate::core::history::EventType::Next,
        true,
    )
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
        format!("changed the query from '{previous}' to '{new_query}'"),
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

    let new_query = outcome.page.page == 1
        || session
            .queries
            .last()
            .is_none_or(|record| record.query != query);
    if new_query {
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
    // `finish()` はページ内表示とセッション内参照に共通の ID をすでに採番している。
    session.results.extend(outcome.hits.iter().cloned());
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
        None => "none".to_string(),
        Some(p) => match p.style {
            PagingStyle::Offset => format!("offset ({} at a time)", p.page_size.unwrap_or(10)),
            PagingStyle::Page => format!("page ({} at a time)", p.page_size.unwrap_or(10)),
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
        let long = "a".repeat(600);
        let (query, warnings) = prepare_query(&long).unwrap();
        assert_eq!(crate::core::text::chars(&query), 512);
        assert_eq!(warnings.len(), 1);
    }
}
