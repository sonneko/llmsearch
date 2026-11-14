//! M5: CLI を操作するエージェント向けの回帰テスト。外部通信は行わない。
mod support;

use clap::Parser;
use llmsearch::core::errors::{ErrorCode, Locale};
use llmsearch::core::history::EventType;
use llmsearch::ui::{cli::Cli, commands, render::Renderer};
use llmsearch::usecases::{document, engines};
use serde_json::{Value, json};
use std::sync::Arc;

fn renderer(h: &support::Harness) -> Renderer {
    Renderer::new(false, false, Locale::Ja, true, 80, Arc::clone(&h.app.clock))
}

fn execute(h: &mut support::Harness, args: &[&str]) {
    let cli =
        Cli::try_parse_from(std::iter::once("llmsearch").chain(args.iter().copied())).unwrap();
    let r = renderer(h);
    commands::run(&mut h.app, &r, &h.resolved, cli.command.as_ref().unwrap()).unwrap();
}

fn route_results(h: &support::Harness) {
    let html = support::duckduckgo_html(&[
        (1, "First", "https://one.example/a"),
        (2, "Second", "https://two.example/a"),
        (3, "Third", "https://three.example/a"),
    ]);
    h.http.route("html.duckduckgo.com", 200, "text/html", &html);
}

#[test]
fn cli_search_preserves_configured_limit_and_allows_override() {
    let mut h = support::harness(None);
    route_results(&h);
    h.app.settings.search.limit = 2;
    execute(&mut h, &["search", "first"]);
    let id = h.app.current_session_id();
    assert_eq!(h.app.sessions.load(&id).unwrap().unwrap().results.len(), 2);
    execute(&mut h, &["search", "second", "--limit", "1"]);
    assert_eq!(h.app.sessions.load(&id).unwrap().unwrap().results.len(), 1);
}

#[test]
fn cli_refine_uses_previous_query_and_records_history() {
    let mut h = support::harness(None);
    route_results(&h);
    execute(&mut h, &["search", "previous query"]);
    execute(&mut h, &["refine", "new query"]);
    let session = h
        .app
        .sessions
        .load(&h.app.current_session_id())
        .unwrap()
        .unwrap();
    assert_eq!(session.last_query, "new query");
    assert_eq!(session.queries.len(), 2);
    let entries = h.app.history.query(&Default::default()).unwrap();
    let entry = entries
        .iter()
        .find(|e| e.kind == EventType::Refine)
        .unwrap();
    assert_eq!(entry.payload["previous"], "previous query");
    assert_eq!(entry.payload["query"], "new query");
}

#[test]
fn unknown_engine_selectors_fail_instead_of_disappearing() {
    let h = support::harness(None);
    for selectors in [
        vec!["missing".into()],
        vec!["duckduckgo".into(), "missing".into()],
    ] {
        let checks = engines::validate(&h.app, &selectors);
        assert_eq!(checks.len(), selectors.len());
        assert!(
            checks
                .iter()
                .any(|(id, checks)| id == "missing" && checks.iter().any(|c| c.is_fail()))
        );
        assert!(
            checks
                .iter()
                .any(|(_, checks)| checks.iter().any(|c| c.text().contains("E4002")))
        );
    }
}

// CLI プロセスが HTTP を呼ばず読めるよう、期限を固定したキャッシュを用意する。
fn cached_document(h: &mut support::Harness) -> String {
    let url = url::Url::parse("https://article.example/doc").unwrap();
    h.http
        .route("article.example", 200, "text/html", support::ARTICLE);
    let fetched = document::fetch(&mut h.app, &url).unwrap();
    let (mut meta, markdown) = h.app.documents.get(&url).unwrap().unwrap();
    meta.expires_at = "2099-01-01T00:00:00Z".into();
    h.app.documents.put(&meta, &markdown).unwrap();
    format!("D{}", fetched.document.id)
}

fn cli_json(h: &support::Harness, args: &[&str]) -> (i32, Vec<Value>) {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_llmsearch"))
        .args(args)
        .arg("--json")
        .arg("--state-dir")
        .arg(&h.app.paths.root)
        .env_remove("LLMSEARCH_E2E_LIVE")
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let events: Vec<Value> = stdout
        .lines()
        .map(|line| serde_json::from_str(line).expect("JSON Lines"))
        .collect();
    assert!(
        !events.is_empty(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    for event in &events {
        assert_eq!(event["v"], 1);
        assert!(event["session"].is_string());
        let at = event["at"].as_str().expect("at string");
        assert_eq!(at.len(), 20);
        assert!(at.ends_with('Z'));
        chrono::DateTime::parse_from_rfc3339(at).unwrap();
    }
    assert!(matches!(
        events.last().unwrap()["event"].as_str(),
        Some("done" | "error")
    ));
    (output.status.code().unwrap(), events)
}

#[test]
fn cli_session_document_events_and_no_children_work_across_processes() {
    let mut h = support::harness(None);
    let doc = cached_document(&mut h);
    let session = h.app.current_session_id();
    let args = [
        "read",
        doc.as_str(),
        "--session",
        session.as_str(),
        "--section",
        "S3",
    ];
    let (code, events) = cli_json(&h, &args);
    assert_eq!(code, 0, "{events:?}");
    assert_eq!(events[0]["event"], "document");
    assert_eq!(events[0]["title"], "適応策の手引き");
    assert!(events[0]["warnings"].is_array());
    for event in &events {
        assert_eq!(event["session"], session);
        if matches!(event["event"].as_str(), Some("document" | "content")) {
            assert!(event["quota"]["metrics"].is_array());
        }
    }
    assert!(
        events[1]["text"]
            .as_str()
            .unwrap()
            .contains("地域ごとの計画")
    );
    let mut no_children = args.to_vec();
    no_children.push("--no-children");
    let (code, events) = cli_json(&h, &no_children);
    assert_eq!(code, 0);
    assert!(
        !events[1]["text"]
            .as_str()
            .unwrap()
            .contains("地域ごとの計画")
    );
}

#[test]
fn cli_outline_uses_settings_depth_unless_overridden() {
    let mut h = support::harness(None);
    let doc = cached_document(&mut h);
    let config = h.dir.path().join("settings.json");
    std::fs::write(
        &config,
        json!({"schemaVersion":2,"context":{"outlineDepth":2}}).to_string(),
    )
    .unwrap();
    let session = h.app.current_session_id();
    let args = [
        "outline",
        doc.as_str(),
        "--session",
        session.as_str(),
        "--config",
        config.to_str().unwrap(),
    ];
    let (code, events) = cli_json(&h, &args);
    assert_eq!(code, 0, "{events:?}");
    assert_eq!(events[1]["event"], "outline");
    assert_eq!(events[1]["depth"], 2);
    let mut args = args.to_vec();
    args.extend(["--depth", "4"]);
    let (code, events) = cli_json(&h, &args);
    assert_eq!(code, 0);
    assert_eq!(events[1]["depth"], 4);
}

#[test]
fn cli_errors_include_session_and_catalog_hint() {
    let h = support::harness(None);
    let (code, events) = cli_json(&h, &["read", "D1", "--session", "missing-session"]);
    assert_eq!(code, 5);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["code"], ErrorCode::E5001.as_str());
    assert_eq!(events[0]["session"], "missing-session");
    assert!(!events[0]["hint"].as_str().unwrap().is_empty());
}
