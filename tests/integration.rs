//! 検索 → 取得 → アウトライン → 節読み出しの通し試験。

mod support;

use llmsearch::core::config::quota::Metric;
use llmsearch::core::errors::ErrorCode;
use llmsearch::usecases::{document, engines, history as history_uc, search};

fn harness_with_results(count: usize) -> (support::Harness, Vec<String>) {
    // `maxPerHost` の影響を受けないよう、結果ごとに別ホストにする
    let items: Vec<(usize, String, String)> = (1..=count)
        .map(|i| {
            (
                i,
                format!("結果 {i} のタイトル"),
                format!("https://site{i}.example.jp/page"),
            )
        })
        .collect();
    let refs: Vec<(usize, &str, &str)> = items
        .iter()
        .map(|(n, t, u)| (*n, t.as_str(), u.as_str()))
        .collect();
    let html = support::duckduckgo_html(&refs);
    let harness = support::harness(None);
    harness.http.route(
        "html.duckduckgo.com",
        200,
        "text/html; charset=utf-8",
        &html,
    );
    harness
        .http
        .route("site", 200, "text/html; charset=utf-8", support::ARTICLE);
    let urls = items.into_iter().map(|(_, _, u)| u).collect();
    (harness, urls)
}

/// 検索結果の URL がエンジンのリダイレクト URL のまま出てくると、取得時に
/// エンジン側のページしか読めない。実 URL に展開されていること。
#[test]
fn search_unwraps_engine_redirect_urls() {
    let harness = support::harness(None);
    let items = [
        (1usize, "A のタイトル", "https://a.example.jp/one"),
        (2, "B のタイトル", "https://b.example.jp/two?x=1"),
    ];
    let html = support::duckduckgo_redirect_html(&items);
    harness.http.route(
        "html.duckduckgo.com",
        200,
        "text/html; charset=utf-8",
        &html,
    );
    let mut h = harness;
    let outcome = search::run(&mut h.app, "気候変動", 1).expect("検索に失敗");
    let urls: Vec<&str> = outcome.hits.iter().map(|hit| hit.url.as_str()).collect();
    assert_eq!(
        urls,
        vec!["https://a.example.jp/one", "https://b.example.jp/two?x=1"],
        "リダイレクト URL が残っている: {urls:?}"
    );
    for hit in &outcome.hits {
        assert!(
            !hit.url.contains("duckduckgo.com"),
            "エンジンのホストが残っている: {}",
            hit.url
        );
    }
}

/// 結果 URL がリダイレクトページのまま残っていても、取得時に中継ページを
/// たどって実体の記事を読めること（`<meta refresh>` と JS の location）。
#[test]
fn fetch_follows_html_redirect_page() {
    let harness = support::harness(None);
    harness.http.route(
        "relay.example",
        200,
        "text/html; charset=utf-8",
        r#"<html><head><meta http-equiv="refresh" content="0; url=https://dest.example/article"></head><body>移動中…</body></html>"#,
    );
    harness.http.route(
        "jsrelay.example",
        200,
        "text/html; charset=utf-8",
        r#"<html><body><script>window.location.href = "https://dest.example/article";</script></body></html>"#,
    );
    harness.http.route(
        "dest.example",
        200,
        "text/html; charset=utf-8",
        support::ARTICLE,
    );
    let mut h = harness;

    for entry in [
        "https://relay.example/out?uddg=x",
        "https://jsrelay.example/go",
    ] {
        let url = llmsearch::core::url::parse(entry).unwrap();
        let fetched = document::fetch(&mut h.app, &url).expect("取得に失敗");
        assert!(
            fetched.document.final_url.contains("dest.example"),
            "リダイレクト先を追えていない: {}",
            fetched.document.final_url
        );
        assert!(
            fetched.document.title.contains("適応策"),
            "実体の記事を読めていない: {}",
            fetched.document.title
        );
    }
}

#[test]
fn search_returns_only_titles_and_snippets() {
    let (mut h, _) = harness_with_results(10);
    let outcome = search::run(&mut h.app, "気候変動 適応策", 1).expect("検索に失敗");
    assert_eq!(outcome.hits.len(), 7, "既定の limit=7 を守る");
    for hit in &outcome.hits {
        assert!(hit.snippet.is_some());
        assert!(hit.url.contains(".example.jp/page"));
    }
    assert!(h.app.secrets.names().count() == 0);
}

#[test]
fn search_consumes_quota_and_records_history() {
    let (mut h, _) = harness_with_results(10);
    let outcome = search::run(&mut h.app, "適応", 1).unwrap();
    let session = search::apply_to_session(&mut h.app, &outcome, "適応").unwrap();
    assert_eq!(session.results.len(), 7);
    let guard = h.app.guard().unwrap();
    let metrics = guard.metrics();
    let searches = metrics
        .iter()
        .find(|m| m.name == "session.searches")
        .expect("session.searches");
    assert_eq!(searches.used, 1);

    let entries = history_uc::list(&h.app, Some(10), None).unwrap();
    assert!(
        entries
            .iter()
            .any(|e| e.kind == llmsearch::core::history::EventType::Search)
    );
}

#[test]
fn quota_stops_further_searches() {
    let (mut h, _) = harness_with_results(10);
    h.app.quota.session.searches = Some(1);
    search::run(&mut h.app, "一度目", 1).unwrap();
    let err = search::run(&mut h.app, "二度目", 1).unwrap_err();
    assert_eq!(err.code, ErrorCode::E5005);
}

#[test]
fn fetch_outline_and_read_a_section() {
    let (mut h, _) = harness_with_results(3);
    let outcome = search::run(&mut h.app, "適応", 1).unwrap();
    let session = search::apply_to_session(&mut h.app, &outcome, "適応").unwrap();
    let hit = session.results.first().unwrap();
    let url = llmsearch::core::url::parse(&hit.url).unwrap();

    let fetched = document::fetch(&mut h.app, &url).unwrap();
    let doc = fetched.document;
    assert!(doc.chars > 100);
    assert!(fetched.new);

    let outline = document::outline(&mut h.app, &doc, 3).unwrap();
    assert!(outline.headings.len() >= 4);
    let level_two = outline
        .headings
        .iter()
        .find(|h| h.level == 2)
        .expect("h2 が要る");

    let read = document::read(&mut h.app, &doc, &[level_two.id], true).unwrap();
    assert_eq!(read.sections.len(), 1);
    let body = &read.sections[0];
    assert!(body.text.contains("影響評価") || body.text.contains("適応計画"));
    assert!(body.chars > 10);
}

#[test]
fn outline_depth_hides_deeper_headings() {
    let (mut h, _) = harness_with_results(1);
    let outcome = search::run(&mut h.app, "q", 1).unwrap();
    let session = search::apply_to_session(&mut h.app, &outcome, "q").unwrap();
    let url = llmsearch::core::url::parse(&session.results[0].url).unwrap();
    let fetched = document::fetch(&mut h.app, &url).unwrap();
    let outline = document::outline(&mut h.app, &fetched.document, 2).unwrap();
    assert!(outline.hidden >= 1);
    assert!(outline.headings.iter().any(|h| h.hidden && h.level == 3));
}

#[test]
fn context_budget_truncates_section() {
    let (mut h, _) = harness_with_results(1);
    let outcome = search::run(&mut h.app, "q", 1).unwrap();
    let session = search::apply_to_session(&mut h.app, &outcome, "q").unwrap();
    let url = llmsearch::core::url::parse(&session.results[0].url).unwrap();
    let fetched = document::fetch(&mut h.app, &url).unwrap();
    // 記事全体は 500 字未満なので、予算を 200 字に絞らないと切り詰めは起きない
    h.app.settings.context.max_chars = 200;
    let read = document::read(&mut h.app, &fetched.document, &[0], true).unwrap();
    assert_eq!(read.budget, 200, "予算が設定値になっていない");
    assert!(read.total_chars <= 200);
    assert!(read.truncated);
}

#[test]
fn second_fetch_uses_cache_without_http() {
    let (mut h, _) = harness_with_results(1);
    let outcome = search::run(&mut h.app, "q", 1).unwrap();
    let session = search::apply_to_session(&mut h.app, &outcome, "q").unwrap();
    let url = llmsearch::core::url::parse(&session.results[0].url).unwrap();
    document::fetch(&mut h.app, &url).unwrap();
    let before = h.http.request_count();
    document::fetch(&mut h.app, &url).unwrap();
    assert_eq!(before, h.http.request_count(), "キャッシュが効いていない");
}

#[test]
fn cache_can_be_bypassed() {
    let (mut h, _) = harness_with_results(1);
    let outcome = search::run(&mut h.app, "q", 1).unwrap();
    let session = search::apply_to_session(&mut h.app, &outcome, "q").unwrap();
    let url = llmsearch::core::url::parse(&session.results[0].url).unwrap();
    document::fetch(&mut h.app, &url).unwrap();
    let before = h.http.request_count();
    h.app.refresh = true;
    document::fetch(&mut h.app, &url).unwrap();
    assert_eq!(
        before + 1,
        h.http.request_count(),
        "--refresh が効いていない"
    );
}

#[test]
fn filters_remove_denied_hosts() {
    let harness = support::harness(None);
    let html = support::duckduckgo_html(&[
        (1, "良い結果", "https://site1.example.jp/a"),
        (2, "PDF", "https://site2.example.jp/b.pdf"),
    ]);
    harness.http.route(
        "html.duckduckgo.com",
        200,
        "text/html; charset=utf-8",
        &html,
    );
    let mut app = harness.app;
    let outcome = search::run(&mut app, "q", 1).unwrap();
    assert_eq!(outcome.hits.len(), 1);
    assert!(outcome.filtered.removed >= 1);
}

#[test]
fn no_filter_keeps_denied_results() {
    let harness = support::harness(None);
    let html = support::duckduckgo_html(&[
        (1, "良い結果", "https://site1.example.jp/a"),
        (2, "PDF", "https://site2.example.jp/b.pdf"),
    ]);
    harness.http.route(
        "html.duckduckgo.com",
        200,
        "text/html; charset=utf-8",
        &html,
    );
    let mut app = harness.app;
    app.no_filter = true;
    let outcome = search::run(&mut app, "q", 1).unwrap();
    assert_eq!(outcome.hits.len(), 2);
}

#[test]
fn engine_validation_reports_missing_api_key() {
    let h = support::harness(None);
    let results = engines::validate(&h.app, &["brave@api".to_string()]);
    assert_eq!(results.len(), 1);
    assert!(results[0].1.iter().any(|c| c.is_fail()));
    // API キーが無い状態では試行せず E4002 になる
    let def = engines::get(&h.app, "brave@api").unwrap();
    let err = engines::dry_run(&h.app, &def, "test", 1).unwrap_err();
    assert_eq!(err.code, ErrorCode::E4002);
    assert!(def.request.url.contains("api.search.brave.com"));
}

#[test]
fn quota_metric_line_is_reported() {
    let (mut h, _) = harness_with_results(2);
    search::run(&mut h.app, "q", 1).unwrap();
    let guard = h.app.guard().unwrap();
    assert!(guard.usage_line(Metric::Searches).contains("/50"));
}

#[test]
fn empty_query_is_rejected() {
    let (mut h, _) = harness_with_results(1);
    let err = search::run(&mut h.app, "   ", 1).unwrap_err();
    assert_eq!(err.code, ErrorCode::E1007);
}
