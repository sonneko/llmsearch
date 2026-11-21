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
    /// 正規化済みの検索語（JSON の `query` に使う）。
    pub query: String,
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
///
/// `search.merge=true` のときは全エンジンへ送ってラウンドロビン統合し、
/// そうでなければフォールバックの連鎖として実行する。
pub fn run(app: &mut App, query: &str, page: u32) -> Result<SearchOutcome, AppError> {
    let (query, warnings) = prepare_query(query)?;

    // セッションを確定してからクォータを判定する（セッション単位の上限のため）
    let session_id = app.ensure_session()?;
    app.set_session(session_id);

    if app.settings.search.merge {
        run_merge(app, &query, page, warnings)
    } else {
        run_fallback(app, &query, page, warnings)
    }
}

/// 1 エンジン分の検索（正規化・重複除去まで済ませた生の結果）。
struct EngineOutcome {
    def: EngineDef,
    /// エンジン定義の累積上限（`limits.maxResults` / `paging.maxResults`）。
    max_results: usize,
    hits: Vec<search_core::RawHit>,
    /// フィルタ前の取得件数。
    obtained: usize,
    /// 送信した回数（抽出結果 0 件による再送を含む）。
    requests: u32,
    warnings: Vec<Diagnostic>,
}

/// エンジンを 1 つ呼び、結果を正規化する（重複除去・リダイレクト展開まで）。
fn search_once(
    app: &App,
    def: &EngineDef,
    query: &str,
    page: u32,
    limit: u32,
) -> Result<EngineOutcome, AppError> {
    let call = call_engine(app, def, query, page, limit)?;
    let mut warnings: Vec<Diagnostic> = Vec::new();
    let hits = call.outcome.hits;
    if call.outcome.items_seen == 0 && hits.is_empty() {
        return Err(AppError::with_detail(
            ErrorCode::E4001,
            format!("{query} / {}", def.selector()),
        ));
    }

    // 抽出でリダイレクトを解けなかった結果をここで解く（取得時に
    // エンジン側のページを読んでしまうのを防ぐ）
    let (hits, unwrapped, example) = search_core::unwrap_redirects(hits);
    if unwrapped > 0 {
        warnings.push(Diagnostic::new(
            WarningCode::W4006,
            match &example {
                Some(first) => {
                    format!("エンジンのリダイレクト URL を {unwrapped} 件展開した（例: {first}）")
                }
                None => format!("エンジンのリダイレクト URL を {unwrapped} 件展開した"),
            },
        ));
    }
    let (deduped, removed) = search_core::dedup(hits);
    if removed > 0 {
        warnings.push(Diagnostic::new(
            WarningCode::W4005,
            format!("重複 URL を {removed} 件除去した"),
        ));
    }
    warnings.extend(call.outcome.warnings.iter().cloned());
    Ok(EngineOutcome {
        def: def.clone(),
        max_results: def.max_results(app.settings.search.max_results),
        obtained: deduped.len(),
        hits: deduped,
        requests: call.requests,
        warnings,
    })
}

/// フィルタを適用し、1 ページ分に切り出して結果を組み立てる。
#[allow(clippy::too_many_arguments)]
fn finish(
    app: &App,
    query: &str,
    page: u32,
    limit: usize,
    raw: Vec<search_core::RawHit>,
    owners: Vec<String>,
    _obtained: usize,
    max_results: usize,
    engine: String,
    fallback_from: Option<String>,
    mut warnings: Vec<Diagnostic>,
) -> SearchOutcome {
    let now = app.clock.today_local();
    let assigned = search_core::assign_ids(raw, 1, &owners);
    let (kept, filtered) =
        crate::core::config::filter::apply_results(&app.filters, assigned, now, app.no_filter);
    let truncated_by_limit = kept.len().saturating_sub(limit);
    let page_hits: Vec<SearchHit> = kept.into_iter().take(limit).collect();

    let max_results = max_results.max(limit);
    let has_more = page_hits.len() == limit && (page as usize) * limit < max_results;

    if let Some(from) = &fallback_from {
        warnings.push(Diagnostic::new(
            WarningCode::W4003,
            format!("{from} の代わりに {engine} を使用した"),
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
            next_page: page + 1,
            exhausted: !has_more,
        },
        has_more,
        warnings,
    }
}

/// フォールバックの連鎖（`search.merge=false`）。
fn run_fallback(
    app: &mut App,
    query: &str,
    page: u32,
    mut warnings: Vec<Diagnostic>,
) -> Result<SearchOutcome, AppError> {
    let limit = app.settings.search.limit.max(1);
    let limit_u32 = limit as u32;
    let mut guard = app.guard()?;
    guard.check(Metric::Searches, 1)?;

    let candidates = candidate_engines(app)?;
    let mut last_error: Option<AppError> = None;
    let mut used_fallback: Option<String> = None;
    // 送信した回数（フォールバックで試したエンジン数だけ増える）。
    // 操作が成功したときだけ `consume_all` で永続化する（仕様 quotas.md §3.2）。
    let mut pending = Usage::default();

    for (index, selector) in candidates.iter().enumerate() {
        let Some(def) = app.engines.get(selector) else {
            continue;
        };
        // これから送る 1 回分を含めて判定する（送った分は未消費のため）。
        guard.check(Metric::Searches, pending.searches + 1)?;
        match search_once(app, def, query, page, limit_u32) {
            Ok(call) => {
                if index > 0 {
                    used_fallback = Some(candidates[0].clone());
                }
                // 消費量の確定（クォータ）
                // `searches` は送信した回数（同一エンジン内の再送も合算）。
                // `results` はフィルタ前の取得件数。`chars` は `read` でのみ
                // 消費するため、ここでは加算しない（仕様 quotas.md §2.2 / §3.1）。
                pending.searches += call.requests.max(1) as u64;
                pending.results += call.obtained as u64;
                guard.consume_all(pending)?;

                warnings.extend(call.warnings.iter().cloned());
                let engine = call.def.selector();
                let owners: Vec<String> = vec![engine.clone(); call.hits.len()];
                let outcome = finish(
                    app,
                    query,
                    page,
                    limit,
                    call.hits,
                    owners,
                    call.obtained,
                    call.max_results,
                    engine.clone(),
                    used_fallback.clone(),
                    warnings,
                );

                // 履歴
                let entry = crate::core::history::HistoryEntry::new(
                    app.history.next_id().unwrap_or_else(|_| "H?".to_string()),
                    app.clock.now_utc(),
                    app.current_session_id(),
                    crate::core::history::EventType::Search,
                    serde_json::json!({
                        "query": query,
                        "page": page,
                        "results": outcome.hits.len(),
                        "filtered": outcome.filtered.removed,
                        "engine": engine,
                    }),
                );
                let _ = app.history.append(&entry);
                return Ok(outcome);
            }
            Err(err) => {
                // 送信しているので 1 回として数える（成功時にまとめて消費する）。
                pending.searches += 1;
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
) -> Result<SearchOutcome, AppError> {
    let limit = app.settings.search.limit.max(1);
    let limit_u32 = limit as u32;
    let selectors = app.settings.search.engines.clone();
    if selectors.is_empty() {
        // `Settings::validate()` が先に `E1004` とするが、防御的に弾く
        return Err(AppError::with_detail(
            ErrorCode::E4002,
            "search.merge = true だが search.engines が空".to_string(),
        ));
    }

    let mut guard = app.guard()?;
    guard.check(Metric::Searches, 1)?;

    let mut pending = Usage::default();
    let mut lists: Vec<Vec<search_core::RawHit>> = Vec::new();
    let mut owners: Vec<String> = Vec::new();
    // フィルタ前の取得件数（各エンジンの合計。クォータの `results` 用）
    let mut obtained = 0usize;
    let mut first_error: Option<AppError> = None;

    for selector in &selectors {
        let Some(def) = app.engines.get(selector) else {
            warnings.push(Diagnostic::new(
                WarningCode::W4004,
                format!("エンジン '{selector}' が見つからないため統合から除いた"),
            ));
            continue;
        };
        guard.check(Metric::Searches, pending.searches + 1)?;
        match search_once(app, def, query, page, limit_u32) {
            Ok(call) => {
                pending.searches += call.requests.max(1) as u64;
                pending.results += call.obtained as u64;
                obtained += call.obtained;
                warnings.extend(call.warnings.iter().cloned());
                owners.push(call.def.selector());
                lists.push(call.hits);
            }
            Err(err) => {
                pending.searches += 1;
                warnings.push(Diagnostic::new(
                    WarningCode::W4004,
                    format!(
                        "{} は失敗したため統合から除いた（{}: {}）",
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
    let merged_len = merged.len();
    if removed > 0 {
        warnings.push(Diagnostic::new(
            WarningCode::W4005,
            format!("マージ時に重複 URL を {removed} 件除去した"),
        ));
    }

    guard.consume_all(pending)?;

    let engine = selectors.join("+");
    let outcome = finish(
        app,
        query,
        page,
        limit,
        merged,
        merged_owners,
        obtained + merged_len,
        app.settings.search.max_results,
        engine.clone(),
        None,
        warnings,
    );

    let entry = crate::core::history::HistoryEntry::new(
        app.history.next_id().unwrap_or_else(|_| "H?".to_string()),
        app.clock.now_utc(),
        app.current_session_id(),
        crate::core::history::EventType::Search,
        serde_json::json!({
            "query": query,
            "page": page,
            "results": outcome.hits.len(),
            "filtered": outcome.filtered.removed,
            "engine": engine,
            "merged": true,
            "engines": selectors,
        }),
    );
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
        app.clock.as_ref(),
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
