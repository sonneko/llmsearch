//! M6: JSON 出力の統一・抽出再試行・秘匿情報の伏字化の回帰テスト。外部通信は行わない。
mod support;

use std::sync::{Arc, Mutex};

use clap::Parser;
use llmsearch::core::errors::{ErrorCode, Locale};
use llmsearch::ports::{HttpClient, HttpRequest, HttpResponse};
use llmsearch::ui::{cli::Cli, commands, render::Renderer};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};

/// 応答本文を順番に返す偽 HTTP（抽出の揺れを再現する）。
struct SeqHttp {
    bodies: Mutex<std::collections::VecDeque<String>>,
    calls: AtomicUsize,
}

impl SeqHttp {
    fn new(bodies: &[&str]) -> Self {
        Self {
            bodies: Mutex::new(bodies.iter().map(|s| s.to_string()).collect()),
            calls: AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl HttpClient for SeqHttp {
    fn send_blocking(
        &self,
        _profile: &llmsearch::core::config::NetworkProfile,
        request: &HttpRequest,
    ) -> Result<HttpResponse, llmsearch::core::errors::AppError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let body = {
            let mut queue = self.bodies.lock().unwrap();
            match queue.pop_front() {
                Some(next) => next,
                // 使い切ったら最後の応答を繰り返す
                None => "<html><body><p>no results</p></body></html>".to_string(),
            }
        };
        Ok(HttpResponse {
            status: 200,
            final_url: request.url.clone(),
            headers: vec![("content-type".to_string(), "text/html".to_string())],
            body: body.into_bytes(),
            truncated: false,
            redirects: 0,
        })
    }

    fn describe(&self, _profile: &llmsearch::core::config::NetworkProfile) -> String {
        "seq".to_string()
    }
}

fn renderer(h: &support::Harness, json: bool) -> Renderer {
    Renderer::new(json, false, Locale::Ja, true, 80, Arc::clone(&h.app.clock))
}

fn parse(args: &[&str]) -> Cli {
    Cli::try_parse_from(std::iter::once("llmsearch").chain(args.iter().copied())).unwrap()
}

fn try_run(
    h: &mut support::Harness,
    args: &[&str],
) -> Result<i32, llmsearch::core::errors::AppError> {
    let cli = parse(args);
    let r = renderer(h, false);
    commands::run(&mut h.app, &r, &h.resolved, cli.command.as_ref().unwrap())
}

fn run(h: &mut support::Harness, args: &[&str]) -> i32 {
    try_run(h, args).unwrap()
}

fn route_results(h: &support::Harness) {
    let html = support::duckduckgo_html(&[
        (1, "First", "https://one.example/a?token=PLAINSECRET"),
        (2, "Second", "https://two.example/a"),
    ]);
    h.http.route("html.duckduckgo.com", 200, "text/html", &html);
}

const EMPTY: &str = "<html><body><p>no results</p></body></html>";

/// セッションが無い状態で識別子を指定したら E5001（仕様 cli-interface.md §3.2）。
#[test]
fn cli_identifier_without_session_is_e5001() {
    let mut h = support::harness(None);
    let err = try_run(&mut h, &["outline", "R1"]).unwrap_err();
    assert_eq!(err.code, ErrorCode::E5001, "{:?}", err);

    let err = try_run(&mut h, &["next"]).unwrap_err();
    assert_eq!(err.code, ErrorCode::E5001, "{:?}", err);
}

/// セッションはあるが識別子が解決できない場合は E1006（E5001 ではない）。
#[test]
fn cli_unknown_identifier_with_session_is_e1006() {
    let mut h = support::harness(None);
    route_results(&h);
    run(&mut h, &["search", "first"]);
    let err = try_run(&mut h, &["read", "R99"]).unwrap_err();
    assert_eq!(err.code, ErrorCode::E1006, "{:?}", err);
}

/// URL を直接指定した場合は、セッションが無くても取得できる。
#[test]
fn cli_url_target_works_without_session() {
    let mut h = support::harness(None);
    h.http
        .route("one.example", 200, "text/html", support::ARTICLE);
    assert_eq!(run(&mut h, &["outline", "https://one.example/a"]), 0);
}

/// 抽出結果が 0 件の応答は再試行し、成功したら警告を残す（Issue の E4004 対策）。
#[test]
fn cli_search_retries_unmatched_response() {
    let mut h = support::harness(None);
    let html = support::duckduckgo_html(&[(1, "First", "https://one.example/a")]);
    let http = Arc::new(SeqHttp::new(&[EMPTY, &html]));
    h.app.set_http(http.clone());
    run(&mut h, &["search", "first"]);
    assert_eq!(http.calls(), 2, "再試行で 2 回送信する");

    let id = h.app.current_session_id();
    let session = h.app.sessions.load(&id).unwrap().unwrap();
    assert_eq!(session.results.len(), 1);
}

/// 再試行しても一致しない場合は E4004 を返し、警告を失わない。
#[test]
fn cli_search_fails_with_e4004_after_retries() {
    let mut h = support::harness(None);
    let http = Arc::new(SeqHttp::new(&[EMPTY]));
    h.app.set_http(http.clone());
    let err = try_run(&mut h, &["search", "first"]).unwrap_err();
    assert_eq!(err.code, ErrorCode::E4004, "{:?}", err);
    assert_eq!(
        http.calls(),
        h.app.network.retry.max_attempts as usize,
        "maxAttempts の回数だけ試す"
    );
    assert!(
        err.warnings.iter().any(|w| w.code.as_str() == "W4004"),
        "再試行の警告をエラーに添える: {:?}",
        err.warnings
    );
    assert!(err.detail.is_some(), "理由を detail に含める");
}

/// `retry.enabled = false` のときは再試行しない。
#[test]
fn cli_search_does_not_retry_when_disabled() {
    let mut h = support::harness(None);
    h.app.network.retry.enabled = false;
    let http = Arc::new(SeqHttp::new(&[EMPTY]));
    h.app.set_http(http.clone());
    let err = try_run(&mut h, &["search", "first"]).unwrap_err();
    assert_eq!(err.code, ErrorCode::E4004);
    assert_eq!(http.calls(), 1, "再試行しない");
}

/// 検索結果の表示 URL は秘匿パラメータを伏せる（仕様 agent-integration.md §4.2）。
#[test]
fn cli_hides_secret_query_params_in_results() {
    let mut h = support::harness(None);
    route_results(&h);
    run(&mut h, &["search", "first"]);
    let id = h.app.current_session_id();
    let session = h.app.sessions.load(&id).unwrap().unwrap();
    let first = &session.results[0];
    // 取得用の URL は伏字化しない（セッション内の実体）
    assert!(first.url.contains("token=PLAINSECRET"));
    // 表示用は伏せる
    let secrets: Vec<String> = Vec::new();
    let parsed = url::Url::parse(&first.url).unwrap();
    let shown = llmsearch::adapters::http::redact_for_display(&parsed, &secrets);
    assert!(
        shown.contains("token=***") && !shown.contains("PLAINSECRET"),
        "表示 URL: {shown}"
    );
}

/// 定義に値を直接書いた場合も `engines show` では伏せる（Issue #12）。
#[test]
fn cli_engines_show_redacts_explicit_values() {
    let engine = json!({
        "schemaVersion": 1,
        "id": "withsecret",
        "request": {
            "method": "GET",
            "url": "https://example.test/search?token=PLAINSECRET&q={{query}}",
            "headers": {"X-Api-Key": {"value": "PLAINSECRET"}},
        },
        "response": {
            "kind": "html",
            "items": {"selector": "div.result"},
            "fields": {
                "title": {"selector": "a.result__a"},
                "url": {"selector": "a.result__a", "attr": "href", "absolutize": true},
            },
        },
    })
    .to_string();
    let h = support::harness(Some(&engine));
    let mut value =
        serde_json::to_value(llmsearch::usecases::engines::get(&h.app, "withsecret").unwrap())
            .unwrap();
    llmsearch::core::secret::redact_json(&mut value, &h.app.secrets);
    let text = value.to_string();
    assert!(!text.contains("PLAINSECRET"), "伏せられていない: {text}");
    assert!(text.contains("***"));
}

/// すべてのイベントが共通枠（`v` / `event` / `session` / `at`）を持つ。
#[test]
fn renderer_envelope_adds_common_fields() {
    let h = support::harness(None);
    let r = renderer(&h, true);
    r.set_session("2026-11-07-000000");
    let value = r.envelope("done", json!({"command": "config"}));
    assert_eq!(value["v"], 1);
    assert_eq!(value["event"], "done");
    assert_eq!(value["session"], "2026-11-07-000000");
    assert!(
        value["at"].as_str().unwrap().ends_with('Z'),
        "at: {:?}",
        value["at"]
    );
    assert_eq!(value["command"], "config");
}
