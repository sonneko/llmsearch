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
    // 共通枠の `session` を、設定読み込み後の現行セッションに合わせる。
    renderer.set_session(app.current_session_id());
    match command {
        Command::Search { query, limit, page } => {
            // `--limit` が指定された場合のみ設定を上書きする（省略時は設定値）
            if let Some(limit) = limit {
                app.settings.search.limit = (*limit).clamp(1, 30);
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
                let limit = app.settings.search.limit;
                let start = (outcome.page.page as usize - 1) * limit + 1;
                let end = start + outcome.hits.len().saturating_sub(1);
                renderer.out(&format!(
                    "Q: {query}    engine: {}    表示: {start}-{end} 件目",
                    outcome.engine
                ));
                let secrets = secret_values(app);
                for hit in &outcome.hits {
                    renderer.out(&renderer.hit_line_with(hit, &display_url(&hit.url, &secrets)));
                }
                renderer.out(&outcome.filtered.summary_line());
                if outcome.has_more {
                    renderer.out("（続きは `next` または 3) 8 件目以降を見る）");
                }
            }
            if renderer.json {
                done(renderer, "search", format!("{} 件", outcome.hits.len()));
            }
            Ok(0)
        }
        Command::Next { query } => {
            // `next` は「直前の検索の続き」なので、セッションが無い場合は
            // 新規に作らず E5001 で search から始めるよう促す。
            let session_id = app.current_session_id();
            if session_id.is_empty() {
                return Err(AppError::new(ErrorCode::E5001)
                    .detail("`next` は継続する検索が無い")
                    .hint("`search` で新しいセッションを開始する"));
            }
            let session = app
                .sessions
                .load(&session_id)?
                .ok_or_else(|| AppError::with_detail(ErrorCode::E5001, session_id.clone()))?;
            let query = query
                .clone()
                .or_else(|| session.queries.last().map(|q| q.query.clone()))
                .ok_or_else(|| {
                    AppError::with_detail(ErrorCode::E5001, "セッションに検索語が無い")
                        .hint("`search` で新しいセッションを開始する")
                })?;
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
                        "page": outcome.page.page,
                        "hasMore": outcome.has_more,
                        "terminated": !outcome.has_more,
                        "filtered": filtered_json(&outcome.filtered, outcome.truncated_by_limit),
                        "results": hits_json(&outcome.hits, &secret_values(app)),
                        "warnings": crate::ui::render::warnings_json(&outcome.warnings),
                        "quota": quota_json(app),
                    }),
                ));
            } else {
                let secrets = secret_values(app);
                for hit in &outcome.hits {
                    renderer.out(&renderer.hit_line_with(hit, &display_url(&hit.url, &secrets)));
                }
                if !outcome.has_more {
                    renderer.out("これ以上の結果はありません");
                }
            }
            if renderer.json {
                done(renderer, "next", format!("+{} 件", outcome.hits.len()));
            }
            Ok(0)
        }
        Command::Refine { query, previous } => {
            let previous = match previous {
                Some(query) => query.clone(),
                None => app
                    .sessions
                    .load(&app.current_session_id())?
                    .map(|s| s.last_query)
                    .filter(|q| !q.is_empty())
                    .ok_or_else(|| AppError::new(ErrorCode::E5001))?,
            };
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
                done(renderer, "refine", format!("{} 件", outcome.hits.len()));
            } else {
                let secrets = secret_values(app);
                for hit in &outcome.hits {
                    renderer.out(&renderer.hit_line_with(hit, &display_url(&hit.url, &secrets)));
                }
            }
            Ok(0)
        }
        Command::Outline { target: t, depth } => {
            let (session, url) = resolve_target(app, t)?;
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
                    format!("{} 見出し", view.headings.len()),
                );
            } else {
                renderer.out(&renderer.outline_lines(&view.document, &view.headings, view.depth));
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
            let (session, url) = resolve_target(app, t)?;
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
                            "chars": body.chars,
                            "truncated": body.truncated,
                            "nextSectionId": body.next_heading_id,
                            "url": doc.final_url,
                            "quota": quota_json(app),
                        }),
                    ));
                }
                done(renderer, "read", format!("{} 字", outcome.total_chars));
            } else {
                for body in &outcome.sections {
                    let header = document::section_header(&doc, body);
                    let truncated = body
                        .truncated
                        .then(|| (crate::core::text::chars(&body.text), body.chars));
                    renderer.out(&renderer.section_block(
                        &header,
                        &doc.final_url,
                        &body.text,
                        truncated,
                    ));
                }
                renderer.out(&format!(
                    "（出力 {} 字 / 予算 {} 字）",
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
        Command::Doctor => {
            let results = doctor::run(app);
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
                    if failed {
                        "失敗あり"
                    } else {
                        "問題なし"
                    }
                    .to_string(),
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
/// 識別子（`R1` / `D1` / 番号）は「現行セッション」への参照なので、セッションが
/// 無い・期限切れの場合は新しいセッションを作らず `E5001` を返す
/// （仕様 cli-interface.md §3.2 / errors.md §2.5。解決の失敗で状態を変えない）。
fn resolve_target(app: &App, t: &str) -> Result<(String, url::Url), AppError> {
    let trimmed = t.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        let session_id = app.ensure_session()?;
        let session = app.sessions.load(&session_id)?;
        let resolved = target::resolve(t, session.as_ref())?;
        let url = target::to_url(&resolved, session.as_ref())?;
        return Ok((session_id, url));
    }
    let session_id = app.current_session_id();
    if session_id.is_empty() {
        return Err(AppError::new(ErrorCode::E5001)
            .detail("セッションが無いため識別子を解決できない")
            .hint("`search` を実行してから識別子を指定する"));
    }
    let session = app
        .sessions
        .load(&session_id)?
        .ok_or_else(|| AppError::with_detail(ErrorCode::E5001, session_id.clone()))?;
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
        let parsed = crate::core::select::parse(s)?;
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
                    crate::core::select::Ref::Index(n) => out.push(*n),
                    _ => {
                        return Err(AppError::with_detail(
                            ErrorCode::E1005,
                            format!("{s} は節の指定ではない"),
                        ));
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
    match action {
        None | Some(HistoryAction::List) => {
            let entries = history_uc::search(
                app,
                query.as_deref(),
                &kinds,
                None,
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
                done(renderer, "history", format!("{} 件", entries.len()));
            } else {
                for entry in &entries {
                    renderer.out(&renderer.history_line(entry));
                }
                if entries.is_empty() {
                    renderer.out("履歴はありません");
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
                renderer.out(&format!(
                    "  payload: {}",
                    serde_json::to_string_pretty(&entry.payload).unwrap_or_default()
                ));
            }
            Ok(0)
        }
        Some(HistoryAction::Grep { query }) => {
            let entries = history_uc::search(app, Some(query), &[], None, None, Some(limit))?;
            if renderer.json {
                renderer.event(&renderer.envelope(
                    "history",
                    json!({
                        "entries": entries.iter().map(entry_json).collect::<Vec<_>>(),
                        "total": entries.len(),
                    }),
                ));
                done(renderer, "history grep", format!("{} 件", entries.len()));
            } else {
                for entry in &entries {
                    renderer.out(&renderer.history_line(entry));
                }
            }
            Ok(0)
        }
        Some(HistoryAction::Export { format }) => {
            let entries = history_uc::list(app, Some(200), None)?;
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
                    done(renderer, "history export", format!("{} 件", ordered.len()));
                } else {
                    for entry in entries.iter().rev() {
                        renderer.out(&serde_json::to_string(entry).unwrap_or_default());
                    }
                }
            } else {
                renderer.out(&history_uc::export(app, Some(200))?);
            }
            Ok(0)
        }
        Some(HistoryAction::Clear { yes: action_yes }) => {
            if !(yes || *action_yes) {
                return Err(AppError::with_detail(
                    ErrorCode::E1004,
                    "--yes が必要".to_string(),
                ));
            }
            let removed = history_uc::clear(app, None)?;
            renderer.out(&format!("履歴を削除した（{removed} 件）"));
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
                            "description": r.description,
                        })).collect::<Vec<_>>(),
                    }),
                ));
                done(renderer, "engines", format!("{} 件", rows.len()));
            } else {
                for r in &rows {
                    renderer.out(&format!(
                        "{:<16} {:<8} {:<5} {:<6} {:<24} {}",
                        r.id,
                        r.variant,
                        r.kind,
                        if r.enabled { "有効" } else { "無効" },
                        r.tags.join(","),
                        r.api_key_state
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
                    format!("{} 件", results.len()),
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
                renderer.info("--dry-run のため送信していません");
                return Ok(0);
            }
            let guard = app.guard()?;
            guard.check(Metric::Searches, 1)?;
            app.default_engine = def.selector();
            let outcome = search_uc::run(app, query, 1)?;
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
                    format!("{} 件", outcome.hits.len()),
                );
                return Ok(0);
            }
            if *raw {
                for hit in outcome.hits.iter().take(3) {
                    renderer.out(&format!(
                        "{}\t{}\t{}",
                        hit.title,
                        hit.url,
                        hit.snippet.clone().unwrap_or_default()
                    ));
                }
            } else {
                renderer.out(&format!(
                    "{}: {} 件（抽出できず破棄 {} 件）",
                    def.selector(),
                    outcome.hits.len(),
                    outcome.filtered.removed
                ));
                for hit in outcome.hits.iter().take(3) {
                    renderer.out(&format!("- {} <{}>", hit.title, hit.url));
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
            let mut ids: Vec<String> = Vec::new();
            for dir in &dirs {
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        if let Some(stem) = entry.path().file_stem() {
                            let id = stem.to_string_lossy().to_string();
                            if !ids.contains(&id) {
                                ids.push(id);
                            }
                        }
                    }
                }
            }
            ids.sort();
            if renderer.json {
                renderer.event(&renderer.envelope(
                    "network",
                    json!({
                        "ids": ids,
                        "active": app.network.id,
                    }),
                ));
                done(renderer, "network", format!("{} 件", ids.len()));
            } else {
                for id in &ids {
                    renderer.out(id);
                }
            }
            Ok(0)
        }
        Some(NetworkAction::Show { id }) => {
            let path = crate::adapters::config::find_resource(&dirs, id)
                .ok_or_else(|| AppError::with_detail(ErrorCode::E1003, id.clone()))?;
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
                            crate::core::errors::Check::Fail(format!("{id} が見つからない")),
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
                done(renderer, "network validate", format!("{} 件", checks.len()));
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
            let mut ids: Vec<String> = Vec::new();
            for dir in &dirs {
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        if let Some(stem) = entry.path().file_stem() {
                            let id = stem.to_string_lossy().to_string();
                            if !ids.contains(&id) {
                                ids.push(id);
                            }
                        }
                    }
                }
            }
            ids.sort();
            if renderer.json {
                renderer.event(&renderer.envelope(
                    "filters",
                    json!({
                        "ids": ids,
                        "active": app.filters.set.id,
                    }),
                ));
                done(renderer, "filters", format!("{} 件", ids.len()));
            } else {
                for id in &ids {
                    renderer.out(id);
                }
            }
            Ok(0)
        }
        Some(FiltersAction::Show { id, raw }) => {
            let _ = raw;
            let path = crate::adapters::config::find_resource(&dirs, id)
                .ok_or_else(|| AppError::with_detail(ErrorCode::E1003, id.clone()))?;
            let value = crate::adapters::config::read_json_file(&path)?;
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
                            crate::core::errors::Check::Fail(format!("{id} が見つからない")),
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
                                crate::core::errors::Check::Fail(format!(
                                    "{id}: {}",
                                    e.message(app.locale())
                                ))
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
                done(renderer, "filters validate", format!("{} 件", checks.len()));
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
            let guard = app.guard()?;
            let metrics = guard.metrics();
            if renderer.json {
                renderer.event(&renderer.envelope(
                    "quota",
                    json!({
                        "policy": app.quota.id,
                        "metrics": metrics.iter().map(|m| json!({
                            "name": m.name,
                            "used": m.used,
                            "limit": m.limit,
                            "resetsAt": m.resets_at,
                        })).collect::<Vec<_>>(),
                    }),
                ));
                done(renderer, "quota show", String::new());
            } else {
                for m in &metrics {
                    renderer.out(&format!(
                        "{:<24} {:>8} / {:<8} {}",
                        m.name,
                        m.used,
                        m.limit.map(|l| l.to_string()).unwrap_or_else(|| "∞".into()),
                        m.resets_at.clone().unwrap_or_default()
                    ));
                }
                if metrics.is_empty() {
                    renderer.out("上限は設定されていません");
                }
            }
            Ok(0)
        }
        Some(QuotaAction::Reset { scope, yes }) => {
            if !yes {
                return Err(AppError::with_detail(
                    ErrorCode::E1004,
                    "--yes が必要".to_string(),
                ));
            }
            app.counters.reset(scope)?;
            if renderer.json {
                done(renderer, "quota reset", format!("scope={scope}"));
            } else {
                renderer.out(&format!("クォータを初期化した（scope={scope}）"));
            }
            Ok(0)
        }
        Some(QuotaAction::Check { kind, amount }) => {
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
                    if renderer.json {
                        renderer.event(&renderer.envelope(
                            "quota",
                            json!({
                                "kind": kind,
                                "amount": amount,
                                "allowed": true,
                                "policy": app.quota.id,
                                "metrics": guard.metrics().iter().map(|m| json!({
                                    "name": m.name,
                                    "used": m.used,
                                    "limit": m.limit,
                                    "resetsAt": m.resets_at,
                                })).collect::<Vec<_>>(),
                            }),
                        ));
                        done(renderer, "quota check", "実行できます".to_string());
                    } else {
                        renderer.out("実行できます");
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

fn run_config(
    app: &mut App,
    renderer: &Renderer,
    resolved: &crate::usecases::config::Resolved,
    action: &Option<ConfigAction>,
) -> Result<i32, AppError> {
    match action {
        None | Some(ConfigAction::Show) => {
            let rows = crate::usecases::config::describe(app);
            if renderer.json {
                renderer.event(&renderer.envelope(
                    "config",
                    json!({
                        "values": rows.iter().map(|(k, v)| json!({"key": k, "value": v}))
                            .collect::<Vec<_>>(),
                    }),
                ));
                done(renderer, "config show", format!("{} 件", rows.len()));
            } else {
                for (k, v) in rows {
                    renderer.out(&format!("{k:<20} {v}"));
                }
            }
            Ok(0)
        }
        Some(ConfigAction::Explain { key }) => {
            let (value, origin) =
                crate::usecases::config::explain(app, resolved, app.env.as_ref(), key)?;
            if renderer.json {
                renderer.event(&renderer.envelope(
                    "config",
                    json!({
                        "key": key,
                        "value": value,
                        "origin": origin.label(),
                    }),
                ));
                done(renderer, "config explain", key.clone());
            } else {
                renderer.out(&format!("{key} = {value} ({})", origin.label()));
            }
            Ok(0)
        }
        Some(ConfigAction::Path) => {
            let paths = [
                app.paths.root.display().to_string(),
                app.bundled
                    .join("config/settings.json")
                    .display()
                    .to_string(),
            ];
            if renderer.json {
                renderer.event(&renderer.envelope(
                    "config",
                    json!({
                        "paths": paths,
                    }),
                ));
                done(renderer, "config path", paths.len().to_string());
            } else {
                for path in &paths {
                    renderer.out(path);
                }
            }
            Ok(0)
        }
        Some(ConfigAction::Init { force }) => {
            let target = app.paths.root.join("config/settings.json");
            crate::usecases::config::write_settings(&target, &app.settings, *force)?;
            if renderer.json {
                done(renderer, "config init", target.display().to_string());
            } else {
                renderer.out(&format!("{} を書き出した", target.display()));
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
                    if failed {
                        "失敗あり"
                    } else {
                        "問題なし"
                    }
                    .to_string(),
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
fn display_url(url: &str, secrets: &[String]) -> String {
    match url::Url::parse(url) {
        Ok(parsed) => crate::core::url::redact(&parsed, secrets),
        Err(_) => crate::core::secret::redact_text(url, secrets),
    }
}

/// 秘匿値の伏字化に使う値の一覧。
fn secret_values(app: &App) -> Vec<String> {
    crate::core::secret::redaction_list(&app.secrets)
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
        "truncatedByLimit": truncated,
    })
}

fn quota_json(app: &App) -> serde_json::Value {
    match app.guard() {
        Ok(guard) => json!({"metrics": guard.metrics().iter().map(|m| json!({
            "name": m.name,
            "used": m.used,
            "limit": m.limit,
            "resetsAt": m.resets_at,
        })).collect::<Vec<_>>()}),
        Err(_) => json!({"metrics": []}),
    }
}

fn document_event(app: &App, renderer: &Renderer, fetched: &document::Fetched) {
    let doc = &fetched.document;
    renderer.event(&renderer.envelope("document", json!({
        "documentId": doc.id,
        "url": doc.url,
        "finalUrl": doc.final_url,
        "title": doc.title,
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
