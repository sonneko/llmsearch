//! サブコマンドの実行（非対話モード）。

use serde_json::json;

use crate::core::config::quota::Metric;
use crate::core::errors::{AppError, ErrorCode};
use crate::ui::cli::{
    Command, ConfigAction, EnginesAction, FiltersAction, HistoryAction, NetworkAction, QuotaAction,
};
use crate::ui::render::Renderer;
use crate::usecases::{
    App, doctor, document, engines as engines_uc, history as history_uc, search as search_uc,
    target,
};

/// 1 コマンドを実行して終了コードを返す。
///
/// `resolved` は設定の読み込み結果で、`config explain` の由来表示に使う。
pub fn run(
    app: &mut App,
    renderer: &Renderer,
    resolved: &crate::usecases::config::Resolved,
    command: &Command,
) -> Result<i32, AppError> {
    // 出力境界で URL・既知の secret を伏せる。実プロセス以外からの呼び出しでも同じ。
    renderer.set_secrets(app.redaction_list());
    // 共通枠の `session` を、設定読み込み後の現行セッションに合わせる。
    renderer.set_session(app.current_session_id());
    match command {
        Command::Search { query, limit, page } => {
            if let Some(limit) = limit {
                if !(1..=30).contains(limit) {
                    return Err(AppError::with_detail(
                        ErrorCode::E1004,
                        format!("--limit = {limit} (allowed range 1..=30)"),
                    )
                    .hint("choose a value from 1 to 30 for `--limit`"));
                }
                app.settings.search.limit = *limit;
            }
            let limit = app.settings.search.limit.max(1);
            if *page == 0 {
                return Err(AppError::with_detail(
                    ErrorCode::E1004,
                    "--page = 0 (allowed range starts at 1)".to_string(),
                )
                .hint("use `--page 1` or a later page within the engine's result limit"));
            }
            let (max_results, supports_paging, page_step) = if app.settings.search.merge {
                (
                    app.settings.search.max_results,
                    app.settings
                        .search
                        .engines
                        .iter()
                        .any(|id| app.engines.get(id).is_some_and(|def| def.paging.is_some())),
                    limit,
                )
            } else if let Some(def) = app.engines.get(&app.default_engine) {
                let page_step = match def.paging.as_ref() {
                    Some(paging) if paging.style == crate::core::engine::PagingStyle::Offset => {
                        let request_limit = def.limits.max_page_size.min(limit as u32).max(1);
                        let first =
                            crate::core::template::paging_values(Some(paging), 1, request_limit).0;
                        let second =
                            crate::core::template::paging_values(Some(paging), 2, request_limit).0;
                        second.saturating_sub(first).max(1) as usize
                    }
                    _ => limit,
                };
                (
                    def.max_results(app.settings.search.max_results),
                    def.paging.is_some(),
                    page_step,
                )
            } else {
                (app.settings.search.max_results, true, limit)
            };
            let max_pages = if supports_paging {
                max_results.div_ceil(page_step).max(1)
            } else {
                1
            };
            if *page as usize > max_pages {
                return Err(AppError::with_detail(
                    ErrorCode::E1004,
                    format!("--page = {page} (allowed range 1..={max_pages})"),
                )
                .hint(format!("choose a page from 1 to {max_pages}")));
            }

            let outcome = search_uc::run(app, query, *page)?;
            let session = search_uc::apply_to_session(app, &outcome, query)?;
            app.set_session(&session.session_id);
            renderer.set_session(&session.session_id);
            for warning in &outcome.warnings {
                renderer.warn(warning);
            }
            if renderer.json {
                renderer.event(&renderer.envelope(
                    "search",
                    json!({
                        "query": outcome.query,
                        "engine": outcome.engine,
                        "page": outcome.page.page,
                        "hasMore": outcome.has_more,
                        "filtered": filtered_json(&outcome.filtered, outcome.truncated_by_limit),
                        "results": hits_json(&outcome.hits, &secret_values(app)),
                        "warnings": crate::ui::render::warnings_json(&outcome.warnings),
                        "quota": quota_json(app),
                    }),
                ));
            } else {
                renderer.info(&format!(
                    "session {} (resume with --session {})",
                    session.session_id, session.session_id
                ));
                renderer.out(&format!(
                    "Q: {query}    engine: {}    showing: {}",
                    outcome.engine,
                    result_range(&outcome.hits)
                ));
                let secrets = secret_values(app);
                for hit in &outcome.hits {
                    renderer.out(&renderer.hit_line_with(hit, &display_url(&hit.url, &secrets)));
                }
                renderer.out(
                    &outcome
                        .filtered
                        .summary_line_for(outcome.hits.len(), outcome.truncated_by_limit),
                );
                if outcome.has_more {
                    let next_id = outcome
                        .hits
                        .last()
                        .map(|hit| hit.id.saturating_add(1))
                        .unwrap_or_else(|| (*page).saturating_sub(1) * limit as u32 + 1);
                    renderer.out(&format!(
                        "(continue with `next` for results from R{next_id} onward)"
                    ));
                }
            }
            if renderer.json {
                done(
                    renderer,
                    "search",
                    format!("{} results", outcome.hits.len()),
                );
            }
            Ok(0)
        }
        Command::Next { query } => {
            let resume_was_implicit = app.current_session_id().is_empty();
            let session = app.require_existing_session()?;
            if resume_was_implicit {
                renderer.info(&format!("resumed session {}", session.session_id));
            }
            renderer.set_session(&session.session_id);
            let query = query
                .clone()
                .or_else(|| session.queries.last().map(|q| q.query.clone()))
                .or_else(|| (!session.last_query.is_empty()).then(|| session.last_query.clone()))
                .ok_or_else(|| {
                    AppError::with_detail(ErrorCode::E5001, "the session has no query")
                        .hint("run `search` to start a session before using `next`")
                })?;

            // セッションで直近に使ったエンジン（merge の場合は全エンジン）を継承する。
            let engine_was_explicit =
                resolved.argv.engine.is_some() || app.env.var("LLMSEARCH_ENGINE").is_some();
            inherit_session_engine(app, &session, engine_was_explicit);

            let outcome = search_uc::next(app, &query, &session.page)?;
            let session = search_uc::apply_to_session(app, &outcome, &query)?;
            renderer.set_session(&session.session_id);
            for warning in &outcome.warnings {
                renderer.warn(warning);
            }
            if renderer.json {
                renderer.event(&renderer.envelope(
                    "next",
                    json!({
                        "query": outcome.query,
                        "engine": outcome.engine,
                        "page": outcome.page.page,
                        "hasMore": outcome.has_more,
                        "terminated": !outcome.has_more,
                        "terminationReason": outcome.termination_reason.clone(),
                        "filtered": filtered_json(&outcome.filtered, outcome.truncated_by_limit),
                        "results": hits_json(&outcome.hits, &secret_values(app)),
                        "warnings": crate::ui::render::warnings_json(&outcome.warnings),
                        "quota": quota_json(app),
                    }),
                ));
            } else {
                renderer.out(&format!(
                    "Q: {query}    engine: {}    showing: {}",
                    outcome.engine,
                    result_range(&outcome.hits)
                ));
                let secrets = secret_values(app);
                for hit in &outcome.hits {
                    renderer.out(&renderer.hit_line_with(hit, &display_url(&hit.url, &secrets)));
                }
                renderer.out(
                    &outcome
                        .filtered
                        .summary_line_for(outcome.hits.len(), outcome.truncated_by_limit),
                );
                if outcome.has_more {
                    let next_id = outcome
                        .hits
                        .last()
                        .map(|hit| hit.id.saturating_add(1))
                        .unwrap_or_else(|| hit_next_id(&session));
                    renderer.out(&format!(
                        "(continue with `next` for results from R{next_id} onward)"
                    ));
                } else {
                    renderer.out("No more results.");
                }
            }
            if renderer.json {
                done(renderer, "next", format!("+{} results", outcome.hits.len()));
            }
            Ok(0)
        }
        Command::Refine { query } => {
            let resume_was_implicit = app.current_session_id().is_empty();
            let session_before = app.require_existing_session()?;
            if resume_was_implicit {
                renderer.info(&format!("resumed session {}", session_before.session_id));
            }
            let previous = if !session_before.last_query.is_empty() {
                session_before.last_query.clone()
            } else {
                session_before
                    .queries
                    .last()
                    .map(|record| record.query.clone())
                    .ok_or_else(|| {
                        AppError::with_detail(ErrorCode::E5001, "`refine` needs the previous query")
                            .hint("run `search` before using `refine`")
                    })?
            };
            let engine_was_explicit =
                resolved.argv.engine.is_some() || app.env.var("LLMSEARCH_ENGINE").is_some();
            inherit_session_engine(app, &session_before, engine_was_explicit);
            renderer.set_session(&session_before.session_id);
            let outcome = search_uc::refine(app, &previous, query)?;
            let session = search_uc::apply_to_session(app, &outcome, query)?;
            renderer.set_session(&session.session_id);
            for warning in &outcome.warnings {
                renderer.warn(warning);
            }
            if renderer.json {
                renderer.event(&renderer.envelope(
                    "refine",
                    json!({
                        "query": outcome.query,
                        "previousQuery": previous,
                        "results": hits_json(&outcome.hits, &secret_values(app)),
                        "warnings": crate::ui::render::warnings_json(&outcome.warnings),
                        "quota": quota_json(app),
                    }),
                ));
                done(
                    renderer,
                    "refine",
                    format!("{} results", outcome.hits.len()),
                );
            } else {
                let secrets = secret_values(app);
                for hit in &outcome.hits {
                    renderer.out(&renderer.hit_line_with(hit, &display_url(&hit.url, &secrets)));
                }
            }
            Ok(0)
        }
        Command::Outline { target: t, depth } => {
            let session_was_implicit = app.current_session_id().is_empty();
            let (session, url) = resolve_target(app, t)?;
            if session_was_implicit {
                renderer.info(&format!("using session {session}"));
            }
            let fetched = document::fetch(app, &url)?;
            renderer.set_session(&session);
            if renderer.json {
                document_event(app, renderer, &fetched);
            }
            for warning in &fetched.warnings {
                renderer.warn(warning);
            }
            let depth = depth
                .unwrap_or(app.settings.context.outline_depth)
                .clamp(2, 4);
            let view = document::outline(app, &fetched.document, depth)?;
            if renderer.json {
                renderer.event(&renderer.envelope(
                    "outline",
                    json!({
                        "documentId": view.document.id,
                        "warnings": crate::ui::render::warnings_json(&fetched.warnings),
                        "depth": view.depth,
                        "totalChars": view.total_chars,
                        "hidden": view.hidden,
                        "sections": view.headings.iter().map(|h| json!({
                            "id": h.id,
                            "level": h.level,
                            "text": h.text,
                            "chars": h.chars,
                            "hasChildren": h.has_children,
                            "line": h.start_line,
                            "hidden": h.hidden,
                        })).collect::<Vec<_>>(),
                    }),
                ));
                done(
                    renderer,
                    "outline",
                    format!("{} headings", view.headings.len()),
                );
            } else {
                let secrets = secret_values(app);
                let shown = display_document(&view.document, &secrets);
                renderer.out(&renderer.outline_lines(
                    &shown,
                    &shown.final_url,
                    &view.headings,
                    view.depth,
                ));
            }
            Ok(0)
        }
        Command::Read {
            target: t,
            sections,
            children: _,
            no_children,
            max_chars,
        } => {
            let session_was_implicit = app.current_session_id().is_empty();
            let (session, url) = resolve_target(app, t)?;
            if session_was_implicit {
                renderer.info(&format!("using session {session}"));
            }
            let fetched = document::fetch(app, &url)?;
            renderer.set_session(&session);
            if renderer.json {
                document_event(app, renderer, &fetched);
            }
            for warning in &fetched.warnings {
                renderer.warn(warning);
            }
            let doc = fetched.document.clone();
            if let Some(max) = max_chars {
                app.settings.context.max_chars = (*max).clamp(500, 100_000);
            }
            let ids = parse_sections(&doc, sections)?;
            let outcome = document::read(app, &doc, &ids, !no_children)?;
            if renderer.json {
                for body in &outcome.sections {
                    renderer.event(&renderer.envelope(
                        "content",
                        json!({
                            "documentId": doc.id,
                            "sectionId": body.heading_id,
                            "heading": body.heading,
                            "level": body.level,
                            "text": body.text,
                            "untrusted": true,
                            "chars": body.chars,
                            "truncated": body.truncated,
                            "nextSectionId": body.next_heading_id,
                            "url": display_url(&doc.final_url, &secret_values(app)),
                            "quota": quota_json(app),
                        }),
                    ));
                }
                done(renderer, "read", format!("{} chars", outcome.total_chars));
            } else {
                let secrets = secret_values(app);
                let shown = display_document(&doc, &secrets);
                for body in &outcome.sections {
                    // `S0`（本文全体）の見出しは文書タイトルなので、表示用に伏字化する。
                    let mut display_body = body.clone();
                    display_body.heading = display_url(&body.heading, &secrets);
                    let header = document::section_header(&shown, &display_body);
                    let truncated = body
                        .truncated
                        .then(|| (crate::core::text::chars(&body.text), body.chars));
                    renderer.out(&renderer.section_block(
                        &header,
                        &shown.final_url,
                        &body.text,
                        truncated,
                    ));
                }
                renderer.out(&format!(
                    "(output {} chars / budget {} chars)",
                    outcome.total_chars, outcome.budget
                ));
            }
            Ok(0)
        }
        Command::History {
            action,
            limit,
            query,
            kinds,
            before,
            yes,
        } => run_history(
            app,
            renderer,
            HistoryArgs {
                action: action.clone(),
                limit: *limit,
                query: query.clone(),
                kinds: kinds.clone(),
                before: before.clone(),
                yes: *yes,
            },
        ),
        Command::Engines { action } => run_engines(app, renderer, action),
        Command::Network { action } => run_network(app, renderer, action),
        Command::Filters { action } => run_filters(app, renderer, action),
        Command::Quota { action } => run_quota(app, renderer, action),
        Command::Config { action } => run_config(app, renderer, resolved, action),
        Command::Doctor { no_network } => {
            let results = doctor::run(app, *no_network);
            let failed = results.iter().any(|r| r.check.is_fail());
            if renderer.json {
                renderer.event(&renderer.envelope(
                    "diagnostics",
                    json!({
                        "checks": results.iter().map(|r| json!({
                            "name": r.name,
                            "status": r.check.label(),
                            "text": r.check.text(),
                        })).collect::<Vec<_>>(),
                    }),
                ));
                done(
                    renderer,
                    "doctor",
                    if failed { "issues found" } else { "no issues" }.to_string(),
                );
            } else {
                renderer.out(&renderer.doctor_lines(&results));
            }
            Ok(if failed { 1 } else { 0 })
        }
    }
}

/// `outline` / `read` の対象を解決する。
///
/// URL は指定だけで取得できるため、必要なら新しいセッションを用意する。
/// 識別子（`R1` / `D1` / 番号）は現行または直近の既存セッションから解決する。
fn resolve_target(app: &App, t: &str) -> Result<(String, url::Url), AppError> {
    let trimmed = t.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        let session_id = app.ensure_session()?;
        let session = app.sessions.load(&session_id)?;
        let resolved = target::resolve(t, session.as_ref())?;
        let url = target::to_url(&resolved, session.as_ref())?;
        return Ok((session_id, url));
    }
    let session = app.require_existing_session()?;
    let session_id = session.session_id.clone();
    let resolved = target::resolve(t, Some(&session))?;
    let url = target::to_url(&resolved, Some(&session))?;
    Ok((session_id, url))
}

fn parse_sections(
    doc: &crate::core::session::DocumentRef,
    sections: &[String],
) -> Result<Vec<u32>, AppError> {
    if sections.is_empty() {
        return Ok(vec![0]);
    }
    let mut out = Vec::new();
    for s in sections {
        let parsed = crate::core::select::parse(s).map_err(|err| {
            if err.code == ErrorCode::E1005 {
                err.hint("use section IDs such as `S0`, `S3`, or `D1:S3`")
            } else {
                err
            }
        })?;
        for selection in parsed {
            for r in &selection.refs {
                match r {
                    crate::core::select::Ref::Id { prefix: 'S', id } => out.push(*id),
                    crate::core::select::Ref::Id { prefix: 'D', id } => {
                        if *id != doc.id && !doc.url.is_empty() {
                            // 別文書の指定は受け付けない
                        }
                        out.push(0);
                    }
                    _ => {
                        return Err(AppError::with_detail(
                            ErrorCode::E1005,
                            format!("{s} is not a section reference"),
                        )
                        .hint("use section IDs such as `S0`, `S3`, or `D1:S3`"));
                    }
                }
            }
        }
    }
    out.dedup();
    Ok(out)
}

/// 履歴サブコマンドの引数。
struct HistoryArgs {
    action: Option<HistoryAction>,
    limit: usize,
    query: Option<String>,
    kinds: Vec<String>,
    before: Option<String>,
    yes: bool,
}

fn run_history(app: &mut App, renderer: &Renderer, args: HistoryArgs) -> Result<i32, AppError> {
    let HistoryArgs {
        action,
        limit,
        query,
        kinds,
        before,
        yes,
    } = args;
    let action = &action;
    let query = query.clone();
    let kinds = kinds.clone();
    let before = before.clone();
    // Without `--session`, history operations cover all sessions. An explicitly
    // selected session narrows list/grep/export consistently.
    let selected_session = app.current_session_id();
    let history_session = (!selected_session.is_empty()).then_some(selected_session.as_str());
    match action {
        None | Some(HistoryAction::List) => {
            let entries = history_uc::search(
                app,
                query.as_deref(),
                &kinds,
                history_session,
                before.as_deref(),
                Some(limit),
            )?;
            if renderer.json {
                renderer.event(&renderer.envelope(
                    "history",
                    json!({
                        "entries": entries.iter().map(entry_json).collect::<Vec<_>>(),
                        "total": entries.len(),
                    }),
                ));
                done(renderer, "history", format!("{} entries", entries.len()));
            } else {
                for entry in &entries {
                    renderer.out(&renderer.history_line(entry));
                }
                if entries.is_empty() {
                    renderer.out("No history entries.");
                }
            }
            Ok(0)
        }
        Some(HistoryAction::Show { id }) => {
            let entry = history_uc::get(app, id)?
                .ok_or_else(|| AppError::with_detail(ErrorCode::E1006, id.clone()))?;
            if renderer.json {
                renderer.event(&renderer.envelope(
                    "history",
                    json!({
                        "entries": [entry_json(&entry)],
                        "total": 1,
                    }),
                ));
                done(renderer, "history", entry.id.clone());
            } else {
                renderer.out(&format!(
                    "{}  {}  {}",
                    entry.id,
                    entry.at,
                    entry.kind.as_str()
                ));
                renderer.out(&format!("  session: {}", entry.session));
                if let Some(engine) = &entry.engine {
                    renderer.out(&format!("  engine: {engine}"));
                }
                renderer.out(&format!("  {}", entry.summary()));
                let payload = renderer.redacted_value(&entry.payload);
                renderer.out(&format!(
                    "  payload: {}",
                    serde_json::to_string_pretty(&payload).unwrap_or_default()
                ));
            }
            Ok(0)
        }
        Some(HistoryAction::Grep { query }) => {
            let entries =
                history_uc::search(app, Some(query), &[], history_session, None, Some(limit))?;
            if renderer.json {
                renderer.event(&renderer.envelope(
                    "history",
                    json!({
                        "entries": entries.iter().map(entry_json).collect::<Vec<_>>(),
                        "total": entries.len(),
                    }),
                ));
                done(
                    renderer,
                    "history grep",
                    format!("{} entries", entries.len()),
                );
            } else {
                for entry in &entries {
                    renderer.out(&renderer.history_line(entry));
                }
            }
            Ok(0)
        }
        Some(HistoryAction::Export { format }) => {
            let entries = history_uc::list(app, Some(200), history_session)?;
            if format == "json" || format == "jsonl" {
                if renderer.json {
                    // `--json` のときは履歴もイベント封筒に入れる（生の JSON Lines を出さない）。
                    let ordered: Vec<_> = entries.iter().rev().cloned().collect();
                    renderer.event(&renderer.envelope(
                        "history",
                        json!({
                            "entries": ordered.iter().map(entry_json).collect::<Vec<_>>(),
                            "total": ordered.len(),
                        }),
                    ));
                    done(
                        renderer,
                        "history export",
                        format!("{} entries", ordered.len()),
                    );
                } else {
                    for entry in entries.iter().rev() {
                        let value = serde_json::to_value(entry).unwrap_or_default();
                        let safe = renderer.redacted_value(&value);
                        renderer.out(&serde_json::to_string(&safe).unwrap_or_default());
                    }
                }
            } else if renderer.json {
                // Markdown 書き出しでも `--json` のときは生テキストを出さずイベントに包む。
                let ordered: Vec<_> = entries.iter().rev().cloned().collect();
                renderer.event(&renderer.envelope(
                    "history",
                    json!({
                        "format": "markdown",
                        "entries": ordered.iter().map(entry_json).collect::<Vec<_>>(),
                        "total": ordered.len(),
                    }),
                ));
                done(
                    renderer,
                    "history export",
                    format!("{} entries", ordered.len()),
                );
            } else {
                let mut ordered = entries;
                ordered.reverse();
                renderer.out(&crate::core::history::export_markdown(&ordered));
            }
            Ok(0)
        }
        Some(HistoryAction::Clear { yes: action_yes }) => {
            if !(yes || *action_yes) {
                return Err(AppError::with_detail(
                    ErrorCode::E1004,
                    "--yes is required".to_string(),
                ));
            }
            let removed = history_uc::clear(app, None)?;
            if renderer.json {
                renderer.event(&renderer.envelope(
                    "history",
                    json!({
                        "removed": removed,
                        "entries": [],
                        "total": 0,
                    }),
                ));
                done(renderer, "history clear", format!("{removed} entries"));
            } else {
                renderer.out(&format!("Cleared the history ({removed} entries)"));
            }
            Ok(0)
        }
    }
}

fn run_engines(
    app: &mut App,
    renderer: &Renderer,
    action: &Option<EnginesAction>,
) -> Result<i32, AppError> {
    match action {
        None | Some(EnginesAction::List) => {
            let rows = engines_uc::list(app);
            if renderer.json {
                renderer.event(&renderer.envelope(
                    "engines",
                    json!({
                        // エージェントが解釈できる安定した識別子にする（人間向けの文言は出さない）
                        "policy": "first-wins",
                        "engines": rows.iter().map(|r| json!({
                            "id": r.id,
                            "variant": r.variant,
                            "kind": r.kind,
                            "enabled": r.enabled,
                            "tags": r.tags,
                            "requiresApiKey": r.requires_api_key,
                            "apiKey": r.api_key_state,
                            "apiKeyEnvs": r.api_key_envs,
                            "description": r.description,
                        })).collect::<Vec<_>>(),
                    }),
                ));
                done(renderer, "engines", format!("{} engines", rows.len()));
            } else {
                for r in &rows {
                    renderer.out(&format!(
                        "{:<16} {:<8} {:<5} {:<6} {:<24} {}",
                        r.id,
                        r.variant,
                        r.kind,
                        if r.enabled { "enabled" } else { "disabled" },
                        r.tags.join(","),
                        api_key_label(r)
                    ));
                }
            }
            Ok(0)
        }
        Some(EnginesAction::Show { id, raw }) => {
            let def = engines_uc::get(app, id)?;
            let mut value = serde_json::to_value(&def).unwrap_or_default();
            // 値を直接書いた定義（`TmplValue::Explicit`）が表示に漏れないようにする。
            // `--raw`（定義のダンプ）でも同じ基準で伏せる。
            crate::core::secret::redact_json(&mut value, &app.secrets);
            if renderer.json {
                renderer.event(&renderer.envelope(
                    "engine",
                    json!({
                        "id": def.id,
                        "variant": def.variant(),
                        "kind": match def.response.kind {
                            crate::core::engine::ResponseKind::Html => "html",
                            crate::core::engine::ResponseKind::Json => "json",
                        },
                        "definition": value,
                    }),
                ));
                done(renderer, "engines show", def.id.clone());
            } else if *raw {
                renderer.out(&serde_json::to_string_pretty(&value).unwrap_or_default());
            } else {
                for (k, v) in engines_uc::describe(&def) {
                    renderer.out(&format!("{k:<14} {v}"));
                }
            }
            Ok(0)
        }
        Some(EnginesAction::Validate { ids }) => {
            let results = engines_uc::validate(app, ids);
            let failed = results
                .iter()
                .any(|(_, checks)| checks.iter().any(|c| c.is_fail()));
            if renderer.json {
                renderer.event(&renderer.envelope(
                    "diagnostics",
                    json!({
                        "checks": results.iter().flat_map(|(id, checks)| {
                            checks.iter().map(move |c| json!({
                                "name": id,
                                "status": c.label(),
                                "text": c.text(),
                            }))
                        }).collect::<Vec<_>>(),
                    }),
                ));
                done(
                    renderer,
                    "engines validate",
                    format!("{} engines", results.len()),
                );
            } else {
                for (id, checks) in &results {
                    renderer.out(&format!("[{id}]"));
                    for check in checks {
                        renderer.check(check);
                    }
                }
            }
            Ok(if failed { 1 } else { 0 })
        }
        Some(EnginesAction::Test {
            id,
            query,
            dry_run,
            raw,
        }) => {
            let def = engines_uc::get(app, id)?;
            if *dry_run {
                let mut run = engines_uc::dry_run(app, &def, query, 1)?;
                // 送信内容の確認が目的なので、認証ヘッダは伏せて出す。
                crate::core::secret::redact_headers(&mut run.headers);
                if renderer.json {
                    renderer.event(&renderer.envelope(
                        "diagnostics",
                        json!({
                            "checks": [{
                                "name": def.selector(),
                                "status": "ok",
                                "text": format!("{} {}", run.method, run.url),
                            }],
                            "request": {
                                "method": run.method,
                                "url": run.url,
                                "headers": run.headers,
                                "body": run.body,
                            },
                            "sent": false,
                        }),
                    ));
                    done(renderer, "engines test", "--dry-run".to_string());
                    return Ok(0);
                }
                renderer.out(&format!("{} {}", run.method, run.url));
                for (k, v) in &run.headers {
                    renderer.out(&format!("{k}: {v}"));
                }
                if let Some(body) = &run.body {
                    renderer.out(body);
                }
                renderer.info("--dry-run: the request was not sent");
                return Ok(0);
            }
            let outcome = search_uc::run_engine_test(app, &def.selector(), query)?;
            if renderer.json {
                let secrets = secret_values(app);
                renderer.event(&renderer.envelope(
                    "search",
                    json!({
                        "query": outcome.query,
                        "engine": outcome.engine,
                        "page": outcome.page.page,
                        "hasMore": outcome.has_more,
                        "filtered": filtered_json(&outcome.filtered, outcome.truncated_by_limit),
                        "results": hits_json(&outcome.hits, &secrets),
                        "warnings": crate::ui::render::warnings_json(&outcome.warnings),
                        "quota": quota_json(app),
                    }),
                ));
                done(
                    renderer,
                    "engines test",
                    format!("{} results", outcome.hits.len()),
                );
                return Ok(0);
            }
            let secrets = secret_values(app);
            if *raw {
                for hit in outcome.hits.iter().take(3) {
                    renderer.out(&format!(
                        "{}\t{}\t{}",
                        hit.title,
                        display_url(&hit.url, &secrets),
                        hit.snippet.clone().unwrap_or_default()
                    ));
                }
            } else {
                renderer.out(&format!(
                    "{}: {} results ({} discarded as unextractable)",
                    def.selector(),
                    outcome.hits.len(),
                    outcome.filtered.removed
                ));
                for hit in outcome.hits.iter().take(3) {
                    renderer.out(&format!(
                        "- {} <{}>",
                        hit.title,
                        display_url(&hit.url, &secrets)
                    ));
                }
            }
            Ok(0)
        }
    }
}

fn run_network(
    app: &mut App,
    renderer: &Renderer,
    action: &Option<NetworkAction>,
) -> Result<i32, AppError> {
    let dirs = crate::adapters::config::resolve_resource_dir(
        app.env.as_ref(),
        &app.paths.root.join("config"),
        "network",
        &app.bundled,
    );
    match action {
        None | Some(NetworkAction::List) => {
            let ids = resource_ids(&dirs);

            if renderer.json {
                renderer.event(&renderer.envelope(
                    "network",
                    json!({
                        "ids": ids,
                        "active": app.network.id,
                    }),
                ));
                done(renderer, "network", format!("{} profiles", ids.len()));
            } else {
                for id in &ids {
                    renderer.out(id);
                }
            }
            Ok(0)
        }
        Some(NetworkAction::Show { id }) => {
            let path = crate::adapters::config::find_resource(&dirs, id)
                .ok_or_else(|| resource_not_found("network profile", id, &dirs, "network list"))?;
            let mut value = crate::adapters::config::read_json_file(&path)?;
            crate::core::secret::redact_json(&mut value, &app.secrets);
            if renderer.json {
                renderer.event(&renderer.envelope(
                    "network",
                    json!({
                        "id": id,
                        "path": path.display().to_string(),
                        "definition": value,
                    }),
                ));
                done(renderer, "network show", id.clone());
            } else {
                renderer.out(&serde_json::to_string_pretty(&value).unwrap_or_default());
            }
            Ok(0)
        }
        Some(NetworkAction::Validate { ids }) => {
            let mut failed = false;
            let targets: Vec<String> = if ids.is_empty() {
                vec![app.network.id.clone()]
            } else {
                ids.clone()
            };
            let mut checks: Vec<(String, crate::core::errors::Check)> = Vec::new();
            for id in targets {
                let path = match crate::adapters::config::find_resource(&dirs, &id) {
                    Some(p) => p,
                    None => {
                        checks.push((
                            id.clone(),
                            crate::core::errors::Check::Fail(format!("{id} not found")),
                        ));
                        failed = true;
                        continue;
                    }
                };
                let value = crate::adapters::config::read_json_file(&path)?;
                match serde_json::from_value::<crate::core::config::NetworkProfile>(value) {
                    Ok(profile) => {
                        for check in profile.validate() {
                            if check.is_fail() {
                                failed = true;
                            }
                            checks.push((id.clone(), check));
                        }
                    }
                    Err(e) => {
                        failed = true;
                        checks.push((
                            id.clone(),
                            crate::core::errors::Check::Fail(format!("{id}: {e}")),
                        ));
                    }
                }
            }
            if renderer.json {
                renderer.event(&renderer.envelope(
                    "diagnostics",
                    json!({
                        "checks": checks.iter().map(|(id, c)| json!({
                            "name": id,
                            "status": c.label(),
                            "text": c.text(),
                        })).collect::<Vec<_>>(),
                    }),
                ));
                done(
                    renderer,
                    "network validate",
                    format!("{} checks", checks.len()),
                );
            } else {
                let mut last = String::new();
                for (id, check) in &checks {
                    if *id != last {
                        renderer.out(&format!("[{id}]"));
                        last.clone_from(id);
                    }
                    renderer.check(check);
                }
            }
            Ok(if failed { 1 } else { 0 })
        }
    }
}

fn run_filters(
    app: &mut App,
    renderer: &Renderer,
    action: &Option<FiltersAction>,
) -> Result<i32, AppError> {
    let dirs = crate::adapters::config::resolve_resource_dir(
        app.env.as_ref(),
        &app.paths.root.join("config"),
        "filters",
        &app.bundled,
    );
    match action {
        None | Some(FiltersAction::List) => {
            let ids = resource_ids(&dirs);

            if renderer.json {
                renderer.event(&renderer.envelope(
                    "filters",
                    json!({
                        "ids": ids,
                        "active": app.filters.set.id,
                    }),
                ));
                done(renderer, "filters", format!("{} filter sets", ids.len()));
            } else {
                for id in &ids {
                    renderer.out(id);
                }
            }
            Ok(0)
        }
        Some(FiltersAction::Show { id, raw }) => {
            let path = crate::adapters::config::find_resource(&dirs, id)
                .ok_or_else(|| resource_not_found("filter set", id, &dirs, "filters list"))?;
            let value = crate::adapters::config::read_json_file(&path)?;
            let mut value = if *raw {
                value
            } else {
                let set: crate::core::config::FilterSet =
                    serde_json::from_value(value).map_err(|error| {
                        AppError::with_detail(
                            ErrorCode::E1003,
                            format!("{}: {error}", path.display()),
                        )
                    })?;
                serde_json::to_value(set).map_err(|error| {
                    AppError::with_detail(ErrorCode::E1003, format!("{}: {error}", path.display()))
                })?
            };
            crate::core::secret::redact_json(&mut value, &app.secrets);
            if renderer.json {
                renderer.event(&renderer.envelope(
                    "filters",
                    json!({
                        "id": id,
                        "path": path.display().to_string(),
                        "definition": value,
                    }),
                ));
                done(renderer, "filters show", id.clone());
            } else {
                renderer.out(&serde_json::to_string_pretty(&value).unwrap_or_default());
            }
            Ok(0)
        }
        Some(FiltersAction::Validate { ids }) => {
            let mut failed = false;
            let targets: Vec<String> = if ids.is_empty() {
                vec![app.filters.set.id.clone()]
            } else {
                ids.clone()
            };
            let mut checks: Vec<(String, crate::core::errors::Check)> = Vec::new();
            for id in targets {
                let path = match crate::adapters::config::find_resource(&dirs, &id) {
                    Some(p) => p,
                    None => {
                        checks.push((
                            id.clone(),
                            crate::core::errors::Check::Fail(format!("{id} not found")),
                        ));
                        failed = true;
                        continue;
                    }
                };
                let value = crate::adapters::config::read_json_file(&path)?;
                match serde_json::from_value::<crate::core::config::FilterSet>(value) {
                    Ok(set) => {
                        let check = match crate::core::config::filter::CompiledFilters::compile(set)
                        {
                            Ok(compiled) => crate::core::errors::Check::Ok(format!(
                                "deny {} / allow {} / require {}",
                                compiled.deny.len(),
                                compiled.allow.len(),
                                compiled.require.len()
                            )),
                            Err(e) => {
                                failed = true;
                                crate::core::errors::Check::Fail(format!("{id}: {}", e.message()))
                            }
                        };
                        checks.push((id, check));
                    }
                    Err(e) => {
                        failed = true;
                        checks.push((
                            id.clone(),
                            crate::core::errors::Check::Fail(format!("{id}: {e}")),
                        ));
                    }
                }
            }
            if renderer.json {
                renderer.event(&renderer.envelope(
                    "diagnostics",
                    json!({
                        "checks": checks.iter().map(|(id, c)| json!({
                            "name": id,
                            "status": c.label(),
                            "text": c.text(),
                        })).collect::<Vec<_>>(),
                    }),
                ));
                done(
                    renderer,
                    "filters validate",
                    format!("{} checks", checks.len()),
                );
            } else {
                for (id, check) in &checks {
                    renderer.out(&format!("[{id}]"));
                    renderer.check(check);
                }
            }
            Ok(if failed { 1 } else { 0 })
        }
    }
}

fn run_quota(
    app: &mut App,
    renderer: &Renderer,
    action: &Option<QuotaAction>,
) -> Result<i32, AppError> {
    match action {
        None | Some(QuotaAction::Show) => {
            let resume_was_implicit = app.current_session_id().is_empty();
            let resumed = app.resume_latest_session()?;
            if let Some(session) = resumed {
                renderer.set_session(&session.session_id);
                if resume_was_implicit {
                    renderer.info(&format!("resumed session {}", session.session_id));
                }
            }
            let guard = app.guard()?;
            let mut metrics = guard.metrics();
            if app.current_session_id().is_empty() {
                // セッションが選択されていないとき、0 を実測値のように見せない。
                metrics.retain(|metric| !metric.name.starts_with("session."));
            }
            if renderer.json {
                renderer.event(&renderer.envelope(
                    "quota",
                    json!({
                        "policy": app.quota.id,
                        "metrics": metrics.iter().map(|m| json!({
                            "name": m.name,
                            "used": m.used,
                            "limit": m.limit,
                            "remaining": m.remaining(),
                            "resetsAt": m.resets_at,
                        })).collect::<Vec<_>>(),
                    }),
                ));
                done(renderer, "quota show", String::new());
            } else {
                for m in &metrics {
                    renderer.out(&format!(
                        "{:<24} {:>8} / {:<8} (remaining {:>8}) {}",
                        m.name,
                        m.used,
                        m.limit.map(|l| l.to_string()).unwrap_or_else(|| "∞".into()),
                        m.remaining()
                            .map(|n| n.to_string())
                            .unwrap_or_else(|| "∞".into()),
                        m.resets_at.clone().unwrap_or_default()
                    ));
                }
                if metrics.is_empty() {
                    renderer.out("No applicable quota limits are configured.");
                }
            }
            Ok(0)
        }
        Some(QuotaAction::Reset { scope, yes }) => {
            if !yes {
                return Err(AppError::with_detail(
                    ErrorCode::E1004,
                    "--yes is required".to_string(),
                )
                .hint("add `--yes` to confirm the quota reset"));
            }
            // 未知の範囲は全消去になってしまうため、指定できる語を閉じておく。
            if !matches!(
                scope.as_str(),
                "session" | "day" | "window" | "total" | "all"
            ) {
                return Err(AppError::with_detail(
                    ErrorCode::E1004,
                    format!("--scope must be session, day, window, total, or all (got {scope})"),
                )
                .hint("choose one: `--scope session`, `--scope day`, `--scope window`, `--scope total`, or `--scope all`"));
            }
            let before = app.counters.load().ok();
            app.counters.reset(scope)?;
            let after = app.counters.load().ok();
            // 仕様 quotas.md §6.1: 誰がいつ何をリセットしたかを履歴に残す。
            if let Err(err) = record_quota_reset(app, scope, before, after) {
                app.reporter.warn(&crate::core::errors::Diagnostic::new(
                    crate::core::errors::WarningCode::W5001,
                    format!("could not record the quota reset in the history: {err}"),
                ));
            }
            if renderer.json {
                renderer.event(&renderer.envelope(
                    "quota",
                    json!({
                        "action": "reset",
                        "scope": scope,
                        "policy": app.quota.id,
                        // リセット直後の残量をそのまま返す（`quota show` と同じ形）。
                        "metrics": quota_json(app).get("metrics").cloned().unwrap_or(json!([])),
                    }),
                ));
                done(renderer, "quota reset", format!("scope={scope}"));
            } else {
                renderer.out(&format!("Reset the quota (scope={scope})"));
            }
            Ok(0)
        }
        Some(QuotaAction::Check { kind, amount }) => {
            let resume_was_implicit = app.current_session_id().is_empty();
            if let Some(session) = app.resume_latest_session()? {
                renderer.set_session(&session.session_id);
                if resume_was_implicit {
                    renderer.info(&format!("resumed session {}", session.session_id));
                }
            }
            let metric = match kind.as_str() {
                "search" | "searches" => Metric::Searches,
                "fetch" | "fetches" => Metric::Fetches,
                "result" | "results" => Metric::Results,
                "char" | "chars" => Metric::Chars,
                other => {
                    return Err(AppError::with_detail(ErrorCode::E1002, other.to_string()));
                }
            };
            let guard = app.guard()?;
            match guard.check(metric, *amount) {
                Ok(()) => {
                    let metrics = guard.metrics();
                    let usage: Vec<String> = metrics
                        .iter()
                        .filter(|entry| entry.name.ends_with(&format!(".{}", metric.as_str())))
                        .map(|entry| {
                            let limit = entry
                                .limit
                                .map(|limit| limit.to_string())
                                .unwrap_or_else(|| "unlimited".to_string());
                            let remaining = entry
                                .remaining()
                                .map(|n| n.to_string())
                                .unwrap_or_else(|| "unlimited".to_string());
                            format!(
                                "{} {}/{} (remaining {})",
                                entry.name, entry.used, limit, remaining
                            )
                        })
                        .collect();
                    let usage_summary = if usage.is_empty() {
                        "no applicable limit configured".to_string()
                    } else {
                        usage.join("; ")
                    };
                    if renderer.json {
                        renderer.event(&renderer.envelope(
                            "quota",
                            json!({
                                "kind": kind,
                                "amount": amount,
                                "allowed": true,
                                "policy": app.quota.id,
                                "usage": usage,
                                "metrics": metrics.iter().map(|m| json!({
                                    "name": m.name,
                                    "used": m.used,
                                    "limit": m.limit,
                                    "remaining": m.remaining(),
                                    "resetsAt": m.resets_at,
                                })).collect::<Vec<_>>(),
                            }),
                        ));
                        done(renderer, "quota check", format!("allowed; {usage_summary}"));
                    } else {
                        renderer.out(&format!(
                            "Allowed: {} {} (amount {}); {}; policy {}.",
                            metric.as_str(),
                            if *amount == 1 { "request" } else { "requests" },
                            amount,
                            usage_summary,
                            app.quota.id
                        ));
                    }
                    Ok(0)
                }
                Err(err) => {
                    if renderer.json {
                        renderer.event(&renderer.envelope(
                            "quota",
                            json!({
                                "kind": kind,
                                "amount": amount,
                                "allowed": false,
                                "policy": app.quota.id,
                                "metrics": guard.metrics().iter().map(|m| json!({
                                    "name": m.name,
                                    "used": m.used,
                                    "limit": m.limit,
                                    "remaining": m.remaining(),
                                    "resetsAt": m.resets_at,
                                })).collect::<Vec<_>>(),
                            }),
                        ));
                    }
                    renderer.error(&err);
                    Ok(err.exit_code())
                }
            }
        }
    }
}

/// `quota reset` を履歴に記録する（仕様 quotas.md §6.1）。
///
/// `cost` にリセット前、`remaining` にリセット後の消費量を入れる。値は
/// リセットした単位（`session` / `day` / `window` / `total`）のものを取り、
/// 単位が違う計測値を混ぜて二重に数えない。
fn record_quota_reset(
    app: &App,
    scope: &str,
    before: Option<crate::core::config::quota::Counters>,
    after: Option<crate::core::config::quota::Counters>,
) -> Result<(), AppError> {
    use crate::core::config::quota::{Counters, Usage};
    use crate::core::history::{Cost, EventType, HistoryEntry};

    /// スコープに対応する消費量。
    fn usage_for(counters: &Counters, scope: &str) -> Usage {
        match scope {
            "session" => counters.session,
            "day" => counters.day,
            "window" => {
                let mut sum = Usage::default();
                for w in &counters.window {
                    sum.searches += w.used.searches;
                    sum.fetches += w.used.fetches;
                    sum.results += w.used.results;
                    sum.chars += w.used.chars;
                }
                sum
            }
            // `total` と `all` は累積値で代表させる
            _ => counters.total,
        }
    }
    fn to_cost(u: Usage) -> Cost {
        Cost {
            searches: u.searches,
            fetches: u.fetches,
            results: u.results,
            chars: u.chars,
        }
    }

    let before = before.unwrap_or_default();
    let after = after.unwrap_or_default();
    let mut entry = HistoryEntry::new(
        app.history.next_id().unwrap_or_else(|_| "H?".to_string()),
        app.clock.now_utc(),
        app.current_session_id(),
        EventType::Quota,
        json!({
            "action": "reset",
            "scope": scope,
            "policy": app.quota.id,
            "note": format!("quota reset ({scope})"),
        }),
    );
    entry.cost = Some(to_cost(usage_for(&before, scope)));
    entry.remaining = Some(to_cost(usage_for(&after, scope)));
    app.append_history(&entry)
}

fn run_config(
    app: &mut App,
    renderer: &Renderer,
    resolved: &crate::usecases::config::Resolved,
    action: &Option<ConfigAction>,
) -> Result<i32, AppError> {
    use crate::usecases::config as config_uc;
    match action {
        None | Some(ConfigAction::Show) => {
            let rows = config_uc::describe(app, resolved);
            if renderer.json {
                renderer.event(&renderer.envelope(
                    "config",
                    json!({
                        "values": rows.iter().map(|row| json!({
                            "key": row.key,
                            "value": row.value,
                            "origin": row.origin.kind(),
                            "source": row.source,
                        })).collect::<Vec<_>>(),
                        "warnings": crate::ui::render::warnings_json(&resolved.warnings),
                    }),
                ));
                done(renderer, "config show", format!("{} keys", rows.len()));
            } else {
                for row in &rows {
                    // 埋め込み既定から解決した資源は出所を隠さない（M11）。
                    let value = if row.origin == crate::usecases::config::Origin::Default
                        && !row.source.is_empty()
                    {
                        format!("{} ({})", row.value, row.source)
                    } else {
                        row.value.clone()
                    };
                    renderer.out(&format!(
                        "{:<26} {:<10} {}",
                        row.key,
                        row.origin.label(),
                        value
                    ));
                }
            }
            Ok(0)
        }
        Some(ConfigAction::Explain { key }) => {
            let (value, origin) = config_uc::explain(app, resolved, app.env.as_ref(), key)?;
            if renderer.json {
                renderer.event(&renderer.envelope(
                    "config",
                    json!({
                        "key": key,
                        "value": value,
                        "origin": origin.kind(),
                        "source": origin.label(),
                    }),
                ));
                done(renderer, "config explain", key.clone());
            } else {
                renderer.out(&format!("{key} = {value} ({})", origin.label()));
            }
            Ok(0)
        }
        Some(ConfigAction::Path) => {
            let rows = config_uc::paths(app, resolved);
            if renderer.json {
                renderer.event(&renderer.envelope(
                    "config",
                    json!({
                        "paths": rows.iter().map(|row| json!({
                            "label": row.label,
                            "path": row.path.display().to_string(),
                            "used": row.used,
                        })).collect::<Vec<_>>(),
                    }),
                ));
                done(renderer, "config path", rows.len().to_string());
            } else {
                for row in &rows {
                    let mark = if row.used { "[used]" } else { "[candidate]" };
                    renderer.out(&format!("{mark} {:<10} {}", row.label, row.path.display()));
                }
            }
            Ok(0)
        }
        Some(ConfigAction::Init { force }) => {
            let report = config_uc::init(&app.paths.root, *force)?;
            if renderer.json {
                renderer.event(&renderer.envelope(
                    "config",
                    json!({
                        "written": report.written.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
                        "skipped": report.skipped.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
                    }),
                ));
                done(
                    renderer,
                    "config init",
                    format!("{} files", report.written.len()),
                );
            } else {
                for path in &report.written {
                    renderer.out(&format!("wrote {}", path.display()));
                }
                for path in &report.skipped {
                    renderer.out(&format!(
                        "{} already exists; not written (use --force to overwrite)",
                        path.display()
                    ));
                }
            }
            Ok(0)
        }
        Some(ConfigAction::Validate) => {
            let mut failed = false;
            let mut checks: Vec<crate::core::errors::Check> = Vec::new();
            for check in app
                .settings
                .validate()
                .into_iter()
                .chain(app.network.validate())
                .chain(crate::core::config::quota::validate(&app.quota))
            {
                if check.is_fail() {
                    failed = true;
                }
                checks.push(check);
            }
            if renderer.json {
                renderer.event(&renderer.envelope(
                    "diagnostics",
                    json!({
                        "checks": checks.iter().map(|c| json!({
                            "name": "config",
                            "status": c.label(),
                            "text": c.text(),
                        })).collect::<Vec<_>>(),
                    }),
                ));
                done(
                    renderer,
                    "config validate",
                    if failed { "issues found" } else { "no issues" }.to_string(),
                );
            } else {
                for check in &checks {
                    renderer.check(check);
                }
            }
            Ok(if failed { 1 } else { 0 })
        }
    }
}

fn inherit_session_engine(
    app: &mut App,
    session: &crate::core::session::SessionState,
    explicit_engine: bool,
) {
    if explicit_engine {
        if app.settings.search.merge {
            app.settings.search.engines = vec![app.default_engine.clone()];
        }
        return;
    }
    if session.last_engine.is_empty() {
        return;
    }
    let engines: Vec<String> = session.last_engine.split('+').map(str::to_string).collect();
    if engines.len() > 1 {
        app.settings.search.merge = true;
        app.settings.search.engines = engines;
    } else if app.settings.search.merge {
        app.settings.search.engines = engines;
    } else {
        app.default_engine = engines[0].clone();
    }
}

fn resource_ids(dirs: &[std::path::PathBuf]) -> Vec<String> {
    let mut ids = std::collections::BTreeSet::new();
    for dir in dirs {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file()
                    && path
                        .extension()
                        .is_some_and(|extension| extension == "json")
                    && let Some(stem) = path.file_stem()
                {
                    ids.insert(stem.to_string_lossy().to_string());
                }
            }
        }
    }
    ids.into_iter().collect()
}

fn resource_not_found(
    kind: &str,
    id: &str,
    dirs: &[std::path::PathBuf],
    list_command: &str,
) -> AppError {
    let available = resource_ids(dirs);
    let candidates = if available.is_empty() {
        "none".to_string()
    } else {
        available.join(", ")
    };
    AppError::with_detail(ErrorCode::E1003, format!("{kind} '{id}' was not found")).hint(format!(
        "available: {candidates}; run `{list_command}` to list available IDs"
    ))
}

fn result_range(hits: &[crate::core::model::SearchHit]) -> String {
    match (hits.first(), hits.last()) {
        (Some(first), Some(last)) if first.id == last.id => first.id.to_string(),
        (Some(first), Some(last)) => format!("{}-{}", first.id, last.id),
        _ => "none".to_string(),
    }
}

fn hit_next_id(session: &crate::core::session::SessionState) -> u32 {
    session.next_result_id()
}

fn entry_json(entry: &crate::core::history::HistoryEntry) -> serde_json::Value {
    json!({
        "id": entry.id,
        "at": entry.at,
        "type": entry.kind.as_str(),
        "engine": entry.engine,
        "summary": entry.summary(),
        "payload": entry.payload,
    })
}

/// 表示用の伏字化した結果 URL（取得用の URL はセッション内に実体を保持する）。
/// `engines list` のテキスト表示に使う API キー状態の見出し。
fn api_key_label(row: &crate::usecases::engines::EngineRow) -> String {
    match row.api_key_state.as_str() {
        "not-required" => "not required".to_string(),
        "configured" => format!("configured ({})", row.api_key_envs.join(",")),
        _ => format!("not set ({})", row.api_key_envs.join(",")),
    }
}

/// 表示専用の文書情報。URL と、URL 由来のタイトルを伏字化する。
fn display_document(
    doc: &crate::core::session::DocumentRef,
    secrets: &[String],
) -> crate::core::session::DocumentRef {
    let mut out = doc.clone();
    out.title = display_url(&doc.title, secrets);
    out.url = display_url(&doc.url, secrets);
    out.final_url = display_url(&doc.final_url, secrets);
    out
}

fn display_url(url: &str, secrets: &[String]) -> String {
    crate::core::url::redact_url_str(url, secrets)
}

/// 秘匿値の伏字化に使う値の一覧。
fn secret_values(app: &App) -> Vec<String> {
    app.redaction_list()
}

fn hits_json(hits: &[crate::core::model::SearchHit], secrets: &[String]) -> Vec<serde_json::Value> {
    hits.iter()
        .map(|h| {
            json!({
                "id": h.id,
                "rank": h.rank,
                "title": h.title,
                "url": display_url(&h.url, secrets),
                "site": h.site,
                "snippet": h.snippet,
                "published": h.published,
                "engine": h.engine,
            })
        })
        .collect()
}

fn filtered_json(
    stats: &crate::core::config::filter::FilterStats,
    truncated: usize,
) -> serde_json::Value {
    json!({
        "kept": stats.kept,
        "removed": stats.removed,
        "reasons": stats.reasons,
        "disabled": stats.disabled,
        "truncatedByMaxPerHost": stats.truncated_by_max_per_host,
        "truncatedByMaxResults": stats.truncated_by_max_results,
        "truncatedByLimit": truncated,
    })
}

fn quota_json(app: &App) -> serde_json::Value {
    match app.guard() {
        Ok(guard) => json!({"metrics": guard.metrics().iter().map(|m| json!({
            "name": m.name,
            "used": m.used,
            "limit": m.limit,
            "remaining": m.remaining(),
            "resetsAt": m.resets_at,
        })).collect::<Vec<_>>()}),
        Err(_) => json!({"metrics": []}),
    }
}

fn document_event(app: &App, renderer: &Renderer, fetched: &document::Fetched) {
    let doc = &fetched.document;
    let secrets = secret_values(app);
    renderer.event(&renderer.envelope("document", json!({
        "documentId": doc.id,
        "url": display_url(&doc.url, &secrets),
        "finalUrl": display_url(&doc.final_url, &secrets),
        "untrusted": true,
        "title": display_url(&doc.title, &secrets),
        "site": url::Url::parse(&doc.final_url).ok().and_then(|u| u.host_str().map(str::to_owned)),
        "chars": doc.chars,
        "warnings": crate::ui::render::warnings_json(&fetched.warnings),
        "quota": quota_json(app),
    })));
}

fn done(renderer: &Renderer, command: &str, summary: String) {
    renderer.event(&renderer.envelope(
        "done",
        json!({
            "command": command,
            "summary": summary,
        }),
    ));
}
