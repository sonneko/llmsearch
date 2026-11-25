//! M14: M13 dogfooding regressions (#37–#44). All provider responses are local fakes.

mod support;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use clap::{CommandFactory, Parser};
use llmsearch::core::errors::{Check, ErrorCode};
use llmsearch::core::history::EventType;
use llmsearch::ui::cli::Cli;
use llmsearch::ui::commands;
use llmsearch::ui::render::Renderer;
use llmsearch::usecases::{self, search};
use serde_json::{Value, json};

fn renderer(h: &support::Harness, json_mode: bool) -> Renderer {
    Renderer::new(json_mode, false, true, 100, Arc::clone(&h.app.clock))
}

fn result_html(first: usize, count: usize) -> String {
    let items: Vec<(usize, String, String)> = (first..first + count)
        .map(|i| {
            (
                i,
                format!("Result {i}"),
                format!("https://host{i}.example.org/article/{i}"),
            )
        })
        .collect();
    let refs: Vec<(usize, &str, &str)> = items
        .iter()
        .map(|(n, title, url)| (*n, title.as_str(), url.as_str()))
        .collect();
    support::duckduckgo_html(&refs)
}

fn page_engine_definition() -> String {
    json!({
        "schemaVersion": 1,
        "id": "page-test",
        "name": "Page test",
        "request": {
            "method": "GET",
            "url": "https://page.example/search",
            "query": {"q": "{{query}}", "p": "{{page}}"}
        },
        "response": {
            "kind": "html",
            "items": {"selector": "li"},
            "fields": {
                "title": {"selector": "a"},
                "url": {
                    "selector": "a",
                    "attr": "href",
                    "transform": ["absolutize"]
                }
            }
        },
        "paging": {
            "style": "page",
            "pageParam": "p",
            "pageBase": 1,
            "pageSize": 10,
            "maxResults": 25
        },
        "limits": {"maxResults": 25, "maxPageSize": 10, "minIntervalMs": 0}
    })
    .to_string()
}

fn page_html(first: usize, count: usize) -> String {
    let mut html = String::from("<ul>");
    for i in first..first + count {
        html.push_str(&format!(
            "<li><a href=\"https://host{i}.example.org/article/{i}\">Result {i}</a></li>"
        ));
    }
    html.push_str("</ul>");
    html
}

fn labels(hits: &[llmsearch::core::model::SearchHit]) -> Vec<u32> {
    hits.iter().map(|hit| hit.id).collect()
}

/// IDs shown by `next` continue the same sequence stored in the session.
#[test]
fn offset_next_ids_match_session_ids_and_record_next_termination() {
    let mut h = support::harness(None);
    h.app.default_engine = "duckduckgo".into();
    h.app.settings.search.limit = 7;
    h.http
        .route("s=0", 200, "text/html; charset=utf-8", &result_html(1, 10));
    h.http
        .route("s=7", 200, "text/html; charset=utf-8", &result_html(8, 10));
    h.http
        .route("s=14", 200, "text/html; charset=utf-8", &result_html(15, 6));

    let first = search::run(&mut h.app, "rust", 1).unwrap();
    assert_eq!(labels(&first.hits), (1..=7).collect::<Vec<_>>());
    assert!(first.has_more);
    let session = search::apply_to_session(&mut h.app, &first, "rust").unwrap();

    let second = search::next(&mut h.app, "rust", &session.page).unwrap();
    assert_eq!(labels(&second.hits), (8..=14).collect::<Vec<_>>());
    let session = search::apply_to_session(&mut h.app, &second, "rust").unwrap();
    assert_eq!(session.results[7].id, second.hits[0].id);
    assert_eq!(session.find_result(8).unwrap().url, second.hits[0].url);

    let third = search::next(&mut h.app, "rust", &session.page).unwrap();
    assert_eq!(labels(&third.hits), (15..=20).collect::<Vec<_>>());
    assert!(!third.has_more);
    assert_eq!(third.termination_reason.as_deref(), Some("emptyPage"));
    let session = search::apply_to_session(&mut h.app, &third, "rust").unwrap();
    assert_eq!(labels(&session.results), (1..=20).collect::<Vec<_>>());

    let entries = h.app.history.query(&Default::default()).unwrap();
    let next = entries
        .iter()
        .find(|entry| entry.kind == EventType::Next && entry.payload["page"] == 3)
        .unwrap();
    assert_eq!(next.engine.as_deref(), Some("duckduckgo"));
    assert_eq!(next.payload["offset"], 14);
    assert_eq!(next.payload["added"], 6);
    assert_eq!(next.payload["terminationReason"], "emptyPage");
}

/// Page-style providers are re-read and sliced by the logical `search.limit`, not native page size.
#[test]
fn page_style_next_slices_across_provider_pages() {
    let definition = page_engine_definition();
    let mut h = support::harness(Some(&definition));
    h.app.default_engine = "page-test".into();
    h.app.settings.search.limit = 7;
    h.http
        .route("p=1", 200, "text/html; charset=utf-8", &page_html(1, 10));
    h.http
        .route("p=2", 200, "text/html; charset=utf-8", &page_html(11, 10));

    let first = search::run(&mut h.app, "rust", 1).unwrap();
    assert_eq!(labels(&first.hits), (1..=7).collect::<Vec<_>>());
    let session = search::apply_to_session(&mut h.app, &first, "rust").unwrap();
    let second = search::next(&mut h.app, "rust", &session.page).unwrap();
    assert_eq!(labels(&second.hits), (8..=14).collect::<Vec<_>>());
    assert!(second.has_more);

    let requests = h.http.requests.lock().unwrap();
    let pages: Vec<String> = requests
        .iter()
        .map(|request| {
            request
                .url
                .query_pairs()
                .find(|(key, _)| key == "p")
                .unwrap()
                .1
                .into_owned()
        })
        .collect();
    assert_eq!(pages, ["1", "1", "2"]);
}

/// Each provider-page request is checked against the remaining search quota before sending.
#[test]
fn page_style_provider_requests_are_quota_checked_individually() {
    let definition = page_engine_definition();
    let mut h = support::harness(Some(&definition));
    h.app.default_engine = "page-test".into();
    h.app.settings.search.limit = 7;
    h.app.quota.session.searches = Some(2);
    h.http
        .route("p=1", 200, "text/html; charset=utf-8", &page_html(1, 10));
    h.http
        .route("p=2", 200, "text/html; charset=utf-8", &page_html(11, 10));

    let first = search::run(&mut h.app, "rust", 1).unwrap();
    let session = search::apply_to_session(&mut h.app, &first, "rust").unwrap();
    let requests_before_next = h.http.request_count();

    let error = search::next(&mut h.app, "rust", &session.page).unwrap_err();
    assert_eq!(error.code, ErrorCode::E5005);
    assert_eq!(h.http.request_count(), requests_before_next + 1);
    let saved = h.app.sessions.load(&session.session_id).unwrap().unwrap();
    assert_eq!(saved.page.page, session.page.page);
    assert_eq!(saved.page.next_page, session.page.next_page);

    let pages: Vec<String> = h
        .http
        .requests
        .lock()
        .unwrap()
        .iter()
        .map(|request| {
            request
                .url
                .query_pairs()
                .find(|(key, _)| key == "p")
                .unwrap()
                .1
                .into_owned()
        })
        .collect();
    assert_eq!(pages, ["1", "1"]);
}

/// Transport-level HTTP retries each get their own quota preflight before a request is sent.
#[test]
fn transport_retry_attempts_are_quota_checked_before_send() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let sends = Arc::new(AtomicUsize::new(0));
    let server_sends = Arc::clone(&sends);
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let mut request = Vec::new();
        let mut byte = [0u8; 1];
        while !request.ends_with(b"\r\n\r\n") {
            match stream.read(&mut byte) {
                Ok(1) => request.push(byte[0]),
                _ => break,
            }
        }
        server_sends.fetch_add(1, Ordering::SeqCst);
        stream
            .write_all(
                b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
    });

    let definition = json!({
        "schemaVersion": 1,
        "id": "retry-quota",
        "request": {"url": format!("http://{address}/search")},
        "response": {
            "kind": "html",
            "items": {"selector": "li"},
            "fields": {
                "title": {"selector": "a"},
                "url": {"selector": "a", "attr": "href", "transform": ["absolutize"]}
            }
        },
        "limits": {"maxResults": 10, "maxPageSize": 10}
    })
    .to_string();
    let mut h = support::harness(Some(&definition));
    h.app.default_engine = "retry-quota".into();
    h.app.quota.session.searches = Some(1);
    h.app.network.tls.verify = false;
    h.app.network.retry.max_attempts = 2;
    h.app.set_http(Arc::new(
        llmsearch::adapters::http::ReqwestClient::new(Arc::clone(&h.app.clock))
            .with_env(Arc::clone(&h.app.env))
            .with_reporter(Arc::clone(&h.app.reporter)),
    ));

    let error = search::run(&mut h.app, "quota retry", 1).unwrap_err();
    server.join().unwrap();
    assert_eq!(error.code, ErrorCode::E5005);
    assert_eq!(sends.load(Ordering::SeqCst), 1);
}

/// `next` restores the engine used by the saved session instead of the current default.
#[test]
fn next_inherits_the_saved_session_engine() {
    let definition = page_engine_definition();
    let mut h = support::harness(Some(&definition));
    h.app.default_engine = "page-test".into();
    h.app.settings.search.limit = 7;
    h.http
        .route("p=1", 200, "text/html; charset=utf-8", &page_html(1, 10));
    h.http
        .route("p=2", 200, "text/html; charset=utf-8", &page_html(11, 10));

    let first = search::run(&mut h.app, "rust", 1).unwrap();
    let session = search::apply_to_session(&mut h.app, &first, "rust").unwrap();
    h.app.default_engine = "duckduckgo".into();
    h.app.session_id.lock().unwrap().clear();
    h.resolved.argv.engine = None;

    let cli = Cli::try_parse_from(["llmsearch", "next"]).unwrap();
    let renderer = renderer(&h, false);
    commands::run(
        &mut h.app,
        &renderer,
        &h.resolved,
        cli.command.as_ref().unwrap(),
    )
    .unwrap();

    let saved = h.app.sessions.load(&session.session_id).unwrap().unwrap();
    assert_eq!(saved.page.page, 2);
    assert_eq!(h.app.default_engine, "page-test");
    let requests = h.http.requests.lock().unwrap();
    let pages: Vec<String> = requests
        .iter()
        .map(|request| {
            request
                .url
                .query_pairs()
                .find(|(key, _)| key == "p")
                .unwrap()
                .1
                .into_owned()
        })
        .collect();
    assert_eq!(pages, ["1", "1", "2"]);
    drop(requests);

    let next = h
        .app
        .history
        .query(&Default::default())
        .unwrap()
        .into_iter()
        .find(|entry| entry.kind == EventType::Next)
        .unwrap();
    assert_eq!(next.engine.as_deref(), Some("page-test"));
}

/// Reaching the cumulative provider cap closes pagination even if a filter removes rows.
#[test]
fn max_results_cap_is_not_reopened_by_filtering() {
    let definition = page_engine_definition();
    let mut h = support::harness(Some(&definition));
    h.app.default_engine = "page-test".into();
    h.app.settings.search.limit = 7;
    h.app.settings.search.max_results = 10;
    let filter_set: llmsearch::core::config::FilterSet = serde_json::from_value(json!({
        "schemaVersion": 2,
        "id": "last-result-denied",
        "results": {"deny": [{"host": "host10.example.org"}]}
    }))
    .unwrap();
    h.app.filters = llmsearch::core::config::filter::CompiledFilters::compile(filter_set).unwrap();
    h.http
        .route("p=1", 200, "text/html; charset=utf-8", &page_html(1, 10));
    h.http
        .route("p=2", 200, "text/html; charset=utf-8", &page_html(11, 10));

    let first = search::run(&mut h.app, "rust", 1).unwrap();
    assert!(first.has_more);
    let session = search::apply_to_session(&mut h.app, &first, "rust").unwrap();
    let second = search::next(&mut h.app, "rust", &session.page).unwrap();
    assert_eq!(labels(&second.hits), [8, 9]);
    assert!(!second.has_more);
    assert_eq!(second.termination_reason.as_deref(), Some("maxResults"));
    assert_eq!(
        h.http.request_count(),
        2,
        "do not fetch past the cumulative cap"
    );
}

/// A partial/short provider page closes pagination instead of advertising a phantom next page.
#[test]
fn short_provider_page_marks_results_exhausted() {
    let mut h = support::harness(None);
    h.app.default_engine = "duckduckgo".into();
    h.app.settings.search.limit = 7;
    h.http
        .route("s=0", 200, "text/html; charset=utf-8", &result_html(1, 5));
    let outcome = search::run(&mut h.app, "rust", 1).unwrap();
    assert_eq!(outcome.hits.len(), 5);
    assert!(!outcome.has_more);
    assert!(outcome.page.exhausted);
    assert_eq!(outcome.termination_reason.as_deref(), Some("emptyPage"));
}

/// A diagnostic engine test accounts daily usage without assigning it to a fabricated session.
#[test]
fn engines_test_without_session_preserves_session_key_and_usage() {
    let mut h = support::harness(None);
    h.app.default_engine = "duckduckgo".into();
    h.app.settings.search.limit = 7;
    h.http
        .route("s=0", 200, "text/html; charset=utf-8", &result_html(1, 10));
    let first = search::run(&mut h.app, "first", 1).unwrap();
    search::apply_to_session(&mut h.app, &first, "first").unwrap();
    let before = h.app.counters.load().unwrap();
    assert!(!before.session_key.is_empty());

    h.app.session_id.lock().unwrap().clear();
    let outcome = search::run_engine_test(&mut h.app, "duckduckgo", "probe").unwrap();
    assert_eq!(outcome.engine, "duckduckgo");
    assert!(h.app.current_session_id().is_empty());
    let after = h.app.counters.load().unwrap();
    assert_eq!(after.session_key, before.session_key);
    assert_eq!(after.session.searches, before.session.searches);
    assert_eq!(after.day.searches, before.day.searches + 1);
}

/// A next request blocked by quota is still recorded as a terminal `next` history event.
#[test]
fn quota_blocked_next_records_quota_termination_without_sending() {
    let mut h = support::harness(None);
    h.app.default_engine = "duckduckgo".into();
    h.app.settings.search.limit = 7;
    h.app.quota.session.searches = Some(1);
    h.http
        .route("s=0", 200, "text/html; charset=utf-8", &result_html(1, 10));
    let first = search::run(&mut h.app, "rust", 1).unwrap();
    let session = search::apply_to_session(&mut h.app, &first, "rust").unwrap();
    let request_count = h.http.request_count();

    let error = search::next(&mut h.app, "rust", &session.page).unwrap_err();
    assert_eq!(error.code, ErrorCode::E5005);
    assert_eq!(h.http.request_count(), request_count);
    let entries = h.app.history.query(&Default::default()).unwrap();
    let next = entries
        .iter()
        .find(|entry| entry.kind == EventType::Next)
        .unwrap();
    assert_eq!(next.payload["terminationReason"], "quota");
    assert_eq!(next.payload["added"], 0);
    assert_eq!(next.engine.as_deref(), Some("duckduckgo"));
}

/// `quota show` resumes the valid session referenced by the persisted quota counters.
#[test]
fn quota_show_resumes_the_session_without_changing_its_counter() {
    let mut h = support::harness(None);
    h.app.default_engine = "duckduckgo".into();
    h.http
        .route("s=0", 200, "text/html; charset=utf-8", &result_html(1, 10));
    let outcome = search::run(&mut h.app, "rust", 1).unwrap();
    let session = search::apply_to_session(&mut h.app, &outcome, "rust").unwrap();
    let before = h.app.counters.load().unwrap();
    assert_eq!(before.session_key, session.session_id);
    assert_eq!(before.session.searches, 1);

    h.app.session_id.lock().unwrap().clear();
    let cli = Cli::try_parse_from(["llmsearch", "quota", "show"]).unwrap();
    let renderer = renderer(&h, false);
    commands::run(
        &mut h.app,
        &renderer,
        &h.resolved,
        cli.command.as_ref().unwrap(),
    )
    .unwrap();

    assert_eq!(h.app.current_session_id(), session.session_id);
    let after = h.app.counters.load().unwrap();
    assert_eq!(after.session_key, before.session_key);
    assert_eq!(after.session.searches, before.session.searches);
    let guard = h.app.guard().unwrap();
    assert_eq!(
        guard
            .metrics()
            .into_iter()
            .find(|metric| metric.name == "session.searches")
            .unwrap()
            .used,
        1
    );
}

/// Explicit `--state-dir` participates in settings search before loading app paths.
#[test]
fn state_dir_settings_file_is_a_load_candidate() {
    let dir = tempfile::tempdir().unwrap();
    let default_state = dir.path().join("default-state");
    let cli_state = dir.path().join("cli-state");
    std::fs::create_dir_all(&cli_state).unwrap();
    std::fs::write(
        cli_state.join("settings.json"),
        r#"{"schemaVersion":2,"search":{"limit":2}}"#,
    )
    .unwrap();
    let env = Arc::new(support::FakeEnv::new(default_state));
    let bootstrap = usecases::config::Bootstrap {
        state_dir: Some(cli_state.clone()),
        engine: Some("duckduckgo".into()),
        quiet: true,
        ..Default::default()
    };
    let (app, resolved) = usecases::config::build(
        &bootstrap,
        Arc::new(support::FixedClock::new("2026-11-07T05:00:00Z")),
        env,
        Arc::new(support::SeqIds),
        Arc::new(support::SilentReporter::new()),
    )
    .unwrap();
    assert_eq!(app.settings.search.limit, 2);
    assert!(
        resolved
            .candidates
            .contains(&cli_state.join("settings.json"))
    );
    assert!(resolved.sources.contains(&cli_state.join("settings.json")));
}

fn command_error(h: &mut support::Harness, args: &[&str]) -> llmsearch::core::errors::AppError {
    let cli =
        Cli::try_parse_from(std::iter::once("llmsearch").chain(args.iter().copied())).unwrap();
    let renderer = renderer(h, false);
    commands::run(
        &mut h.app,
        &renderer,
        &h.resolved,
        cli.command.as_ref().unwrap(),
    )
    .unwrap_err()
}

#[test]
fn cli_limit_and_page_ranges_are_rejected() {
    let mut h = support::harness(None);
    for args in [
        vec!["search", "rust", "--limit", "0"],
        vec!["search", "rust", "--limit", "31"],
        vec!["search", "rust", "--page", "0"],
        vec!["search", "rust", "--page", "99"],
    ] {
        let error = command_error(&mut h, &args);
        assert_eq!(error.code, ErrorCode::E1004, "{args:?}: {error:?}");
    }
    assert_eq!(h.http.request_count(), 0);
}

#[test]
fn offset_page_range_uses_the_effective_provider_stride() {
    let mut h = support::harness(None);
    h.app.settings.search.max_results = 30;
    h.http.route(
        "s=20",
        200,
        "text/html; charset=utf-8",
        &result_html(21, 10),
    );
    let cli = Cli::try_parse_from([
        "llmsearch",
        "search",
        "rust",
        "--limit",
        "15",
        "--page",
        "3",
    ])
    .unwrap();
    let renderer = renderer(&h, false);
    commands::run(
        &mut h.app,
        &renderer,
        &h.resolved,
        cli.command.as_ref().unwrap(),
    )
    .unwrap();

    assert_eq!(h.http.request_count(), 1);
    let requests = h.http.requests.lock().unwrap();
    let offset = requests[0]
        .url
        .query_pairs()
        .find(|(key, _)| key == "s")
        .unwrap()
        .1
        .into_owned();
    assert_eq!(offset, "20");
}

#[test]
fn cli_help_documents_target_and_limit_constraints() {
    let mut cli = Cli::command();
    let search = cli.find_subcommand_mut("search").unwrap();
    assert!(search.render_long_help().to_string().contains("1-30"));
    let read = cli.find_subcommand_mut("read").unwrap();
    let read_help = read.render_long_help().to_string();
    assert!(read_help.contains("Result (`R1`), document (`D1`), or absolute HTTP(S) URL"));
    let outline = cli.find_subcommand_mut("outline").unwrap();
    assert!(
        outline
            .render_long_help()
            .to_string()
            .contains("Result (`R1`), document (`D1`), or absolute HTTP(S) URL")
    );
    let doctor = cli.find_subcommand_mut("doctor").unwrap();
    assert!(
        doctor
            .render_long_help()
            .to_string()
            .contains("--no-network")
    );
}

#[test]
fn missing_resource_errors_list_available_ids_and_recovery_command() {
    let mut h = support::harness(None);
    let network = command_error(&mut h, &["network", "show", "not-a-profile"]);
    assert_eq!(network.code, ErrorCode::E1003);
    assert!(
        network
            .detail
            .unwrap()
            .contains("network profile 'not-a-profile' was not found")
    );
    let network_hint = network.hint.unwrap();
    assert!(network_hint.contains("available:"));
    assert!(network_hint.contains("default"));
    assert!(network_hint.contains("network list"));

    let filters = command_error(&mut h, &["filters", "show", "not-a-filter"]);
    assert_eq!(filters.code, ErrorCode::E1003);
    assert!(
        filters
            .detail
            .unwrap()
            .contains("filter set 'not-a-filter' was not found")
    );
    let filters_hint = filters.hint.unwrap();
    assert!(filters_hint.contains("available:"));
    assert!(filters_hint.contains("default"));
    assert!(filters_hint.contains("filters list"));
}

fn seed_article_result(h: &mut support::Harness) {
    let url = "https://article.example.org/guide";
    let mut session =
        llmsearch::core::session::SessionState::new("article-session", h.app.clock.now_utc());
    session.results.push(llmsearch::core::model::SearchHit {
        id: 1,
        rank: 1,
        title: "Guide".into(),
        url: url.into(),
        site: Some("article.example.org".into()),
        snippet: None,
        published: None,
        engine: "stub".into(),
        language: None,
    });
    h.app.sessions.save(&session).unwrap();
    h.app.set_session(&session.session_id);
    h.http.route(
        "article.example.org",
        200,
        "text/html; charset=utf-8",
        support::ARTICLE,
    );
}

#[test]
fn read_section_errors_use_section_specific_recovery_hints() {
    let mut h = support::harness(None);
    seed_article_result(&mut h);
    let error = command_error(&mut h, &["read", "R1", "--section", "0"]);
    assert_eq!(error.code, ErrorCode::E1005);
    let hint = error.hint.unwrap();
    assert!(hint.contains("S3") && hint.contains("D1:S3") && hint.contains("S0"));
    assert!(!hint.contains("1,3"));

    let mut h = support::harness(None);
    seed_article_result(&mut h);
    let error = command_error(&mut h, &["read", "R1", "--section", "S99"]);
    assert_eq!(error.code, ErrorCode::E1006);
    assert!(error.detail.unwrap().contains("available: S"));
    assert!(error.hint.unwrap().contains("outline"));
}

#[test]
fn doctor_no_network_skips_probe_and_reports_build_version() {
    let h = support::harness(None);
    let results = usecases::doctor::run(&h.app, true);
    let reachability = results
        .iter()
        .find(|result| result.name == "network.reachability")
        .unwrap();
    assert!(matches!(&reachability.check, Check::Warn(text) if text.contains("--no-network")));
    let version = results
        .iter()
        .find(|result| result.name == "version")
        .unwrap();
    assert!(version.check.text().contains("rustc "));
    assert!(!version.check.text().contains("unknown"));
}

#[test]
fn filters_show_serializes_normalized_keys_and_raw_preserves_legacy_keys() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let normalized = std::process::Command::new(env!("CARGO_BIN_EXE_llmsearch"))
        .args(["--state-dir"])
        .arg(&state)
        .args(["filters", "show", "default"])
        .output()
        .unwrap();
    assert!(normalized.status.success());
    let normalized = String::from_utf8(normalized.stdout).unwrap();
    assert!(normalized.contains("\"urlRegex\""));
    assert!(normalized.contains("\"titleRegex\""));
    assert!(!normalized.contains("\"url_regex\""));
    assert!(!normalized.contains("\"title_regex\""));

    let raw = std::process::Command::new(env!("CARGO_BIN_EXE_llmsearch"))
        .args(["--state-dir"])
        .arg(&state)
        .args(["filters", "show", "default", "--raw"])
        .output()
        .unwrap();
    assert!(raw.status.success());
    let raw = String::from_utf8(raw.stdout).unwrap();
    assert!(raw.contains("\"url_regex\""));
}

#[test]
fn history_list_restores_legacy_engine_from_payload() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let history = state.join("history/history.jsonl");
    std::fs::create_dir_all(history.parent().unwrap()).unwrap();
    std::fs::write(
        &history,
        concat!(
            "{\"id\":\"H1\",\"at\":\"2026-11-07T05:00:00Z\",",
            "\"session\":\"s1\",\"type\":\"search\",",
            "\"payload\":{\"query\":\"rust\",\"results\":1,\"engine\":\"stub\"}}\n"
        ),
    )
    .unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_llmsearch"))
        .args(["--json", "--state-dir"])
        .arg(&state)
        .args(["history", "list"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let events: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let history_event = events
        .iter()
        .find(|event| event["event"] == "history")
        .unwrap();
    assert_eq!(history_event["entries"][0]["engine"], "stub");
}

#[test]
fn color_json_warning_is_emitted_as_json_without_ansi() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_llmsearch"))
        .args(["--color", "--json", "--state-dir"])
        .arg(&state)
        .args(["doctor", "--no-network"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("W1001"));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains('\u{1b}'));
    let events: Vec<Value> = stdout
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let warning = events
        .iter()
        .find(|event| event["event"] == "warnings")
        .unwrap();
    assert!(
        warning["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["code"] == "W1001")
    );
}

#[test]
fn quiet_suppresses_color_json_warning_event_and_stderr() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_llmsearch"))
        .args(["--quiet", "--color", "--json", "--state-dir"])
        .arg(&state)
        .args(["doctor", "--no-network"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stderr).contains("W1001"));
    let events: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(!events.iter().any(|event| event["event"] == "warnings"));
    assert!(events.iter().any(|event| event["event"] == "diagnostics"));
}

#[test]
fn quota_reset_error_hint_lists_valid_scopes() {
    let mut h = support::harness(None);
    let error = command_error(&mut h, &["quota", "reset", "--scope", "bogus", "--yes"]);
    assert_eq!(error.code, ErrorCode::E1004);
    let hint = error.hint.unwrap();
    assert!(hint.contains("--scope session"));
    assert!(hint.contains("--scope day"));
    assert!(hint.contains("--scope window"));
    assert!(hint.contains("--scope total"));
    assert!(hint.contains("--scope all"));
}

#[test]
fn unknown_explicit_session_is_rejected_without_creating_state() {
    let h = support::harness(None);
    let error = h.app.select_session("missing-session").unwrap_err();
    assert_eq!(error.code, ErrorCode::E5001);
    assert!(h.app.sessions.load("missing-session").unwrap().is_none());
}

#[test]
fn filter_action_public_shape_accepts_camel_case_after_legacy_input() {
    let input = json!({
        "schema_version": 2,
        "id": "legacy",
        "results": {
            "deny": [{"host_regex": "example"}],
            "max_per_host": 2
        },
        "content": {
            "normalize": {"decode_entities": false}
        }
    });
    let set: llmsearch::core::config::FilterSet = serde_json::from_value(input).unwrap();
    let output = serde_json::to_value(set).unwrap();
    assert_eq!(output["results"]["deny"][0]["hostRegex"], "example");
    assert_eq!(output["results"]["maxPerHost"], 2);
    assert!(
        !output["content"]["normalize"]["decodeEntities"]
            .as_bool()
            .unwrap()
    );
    assert!(output["results"].get("max_per_host").is_none());
}
