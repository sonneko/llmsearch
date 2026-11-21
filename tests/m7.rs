//! M7: 取得の作法（robots.txt / Retry-After / リダイレクト）とクォータ消費の回帰テスト。
//!
//! 外部ネットワークは使わない。HTTP 層の試験だけローカルの TCP サーバを立てる。
mod support;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use llmsearch::adapters::extract::HttpFetcher;
use llmsearch::adapters::http::ReqwestClient;
use llmsearch::core::config::NetworkProfile;
use llmsearch::core::config::filter::CompiledFilters;
use llmsearch::core::config::network::RedirectPolicy;
use llmsearch::core::config::quota::Metric;
use llmsearch::core::errors::ErrorCode;
use llmsearch::ports::fetch::FetchOptions;
use llmsearch::ports::fetch::Fetcher;
use llmsearch::ports::http::HttpRequest;
use llmsearch::ports::{Clock, HttpClient};
use llmsearch::usecases::{document, search};
use serde_json::json;

// ---------------------------------------------------------------------------
// ローカル HTTP サーバ（応答を順番に返す）
// ---------------------------------------------------------------------------

struct SeqServer {
    url: String,
    requests: Arc<AtomicUsize>,
}

impl SeqServer {
    fn requests(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }
}

/// 応答を順番に返すサーバ。使い切ったら最後の応答を繰り返す。
fn seq_server(responses: &[&str]) -> SeqServer {
    let listener = TcpListener::bind("127.0.0.1:0").expect("ローカルポートの確保に失敗");
    let addr = listener.local_addr().expect("アドレスの取得に失敗");
    let requests = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&requests);
    let responses: Vec<String> = responses.iter().map(|s| s.to_string()).collect();
    std::thread::spawn(move || {
        for _ in 0..32 {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            let mut buf = Vec::new();
            let mut byte = [0u8; 1];
            while !buf.ends_with(b"\r\n\r\n") {
                match stream.read(&mut byte) {
                    Ok(1) => buf.push(byte[0]),
                    _ => break,
                }
            }
            let index = counter.fetch_add(1, Ordering::SeqCst);
            let response = &responses[index.min(responses.len() - 1)];
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    SeqServer {
        url: format!("http://{addr}"),
        requests,
    }
}

/// 生の HTTP 応答を組み立てる（`extra_headers` は 1 行につき 1 要素）。
fn response(status_line: &str, extra_headers: &[&str], body: &str) -> String {
    let mut out = format!(
        "{status_line}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for header in extra_headers {
        out.push_str(header);
        out.push_str("\r\n");
    }
    out.push_str("\r\n");
    out.push_str(body);
    out
}

fn client_with(clock: Arc<dyn Clock>) -> ReqwestClient {
    ReqwestClient::new(clock).with_env(Arc::new(support::FakeEnv::new(std::path::PathBuf::new())))
}

fn fixed_client() -> ReqwestClient {
    client_with(Arc::new(support::FixedClock::new("2026-11-14T00:00:00Z")))
}

fn test_profile() -> NetworkProfile {
    NetworkProfile {
        min_interval_ms: 0,
        timeout_connect_ms: 3000,
        timeout_total_ms: 5000,
        ..NetworkProfile::default()
    }
}

fn get(url: &str) -> HttpRequest {
    HttpRequest::get(url.parse().unwrap())
}

// ---------------------------------------------------------------------------
// #15: Retry-After
// ---------------------------------------------------------------------------

/// `Retry-After` が `maxBackoffMs` 以下なら、その時間だけ待って再試行する。
#[test]
fn retry_after_within_limit_is_honoured() {
    let server = seq_server(&[
        &response("HTTP/1.1 503 Service Unavailable", &["Retry-After: 1"], ""),
        &response("HTTP/1.1 200 OK", &[], "ok"),
    ]);
    let mut profile = test_profile();
    profile.retry.max_attempts = 2;
    profile.retry.max_backoff_ms = 8000;
    profile.retry.backoff_ms = 5000;
    profile.retry.jitter_ms = 0;

    let client = client_with(Arc::new(llmsearch::ports::system::SystemClock::new()));
    let started = std::time::Instant::now();
    let result = client.send_blocking(&profile, &get(&server.url));
    let elapsed = started.elapsed();

    assert!(result.is_ok(), "{:?}", result.err());
    assert_eq!(server.requests(), 2, "再試行して 2 回送信する");
    // 指数バックオフ（5000 ms）ではなく `Retry-After` の 1 秒だけ待つ。
    assert!(
        elapsed >= Duration::from_millis(900),
        "Retry-After の分だけ待つ: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_millis(4000),
        "指数バックオフを使わない: {elapsed:?}"
    );
}

/// `Retry-After` が `maxBackoffMs` を超える場合は再試行せず `E4003` を返す。
#[test]
fn retry_after_over_limit_stops_with_e4003() {
    let server = seq_server(&[&response(
        "HTTP/1.1 503 Service Unavailable",
        &["Retry-After: 120"],
        "",
    )]);
    let mut profile = test_profile();
    profile.retry.max_attempts = 3;
    profile.retry.max_backoff_ms = 8000;

    let err = fixed_client()
        .send_blocking(&profile, &get(&server.url))
        .expect_err("再試行せずに打ち切る");
    assert_eq!(err.code, ErrorCode::E4003, "{err:?}");
    assert_eq!(server.requests(), 1, "1 回だけ送信して打ち切る");
    assert!(err.detail.as_deref().unwrap_or_default().contains("120000"));
}

/// `respectRetryAfter=false` は `Retry-After` を読まず指数バックオフを使う。
#[test]
fn retry_after_is_ignored_when_not_respected() {
    let server = seq_server(&[&response(
        "HTTP/1.1 503 Service Unavailable",
        &["Retry-After: 120"],
        "",
    )]);
    let mut profile = test_profile();
    profile.retry.max_attempts = 2;
    profile.retry.respect_retry_after = false;

    let err = fixed_client()
        .send_blocking(&profile, &get(&server.url))
        .expect_err("再試行しても成功しない");
    assert_eq!(err.code, ErrorCode::E2003, "{err:?}");
    assert_eq!(server.requests(), 2, "Retry-After に関わらず再試行する");
}

/// 解釈できない `Retry-After` は指数バックオフへ戻す。
#[test]
fn unparsable_retry_after_falls_back_to_backoff() {
    let server = seq_server(&[
        &response(
            "HTTP/1.1 503 Service Unavailable",
            &["Retry-After: soon"],
            "",
        ),
        &response("HTTP/1.1 200 OK", &[], "ok"),
    ]);
    let mut profile = test_profile();
    profile.retry.max_attempts = 2;

    let result = fixed_client().send_blocking(&profile, &get(&server.url));
    assert!(result.is_ok(), "{:?}", result.err());
    assert_eq!(server.requests(), 2);
}

// ---------------------------------------------------------------------------
// #17: 3xx と redirectPolicy
// ---------------------------------------------------------------------------

/// `never` は 3xx を `E2003` として返す（従来は成功扱いだった）。
#[test]
fn never_policy_rejects_3xx() {
    let server = seq_server(&[&response("HTTP/1.1 302 Found", &["Location: /moved"], "")]);
    let mut profile = test_profile();
    profile.redirect_policy = RedirectPolicy::Never;

    let err = fixed_client()
        .send_blocking(&profile, &get(&server.url))
        .expect_err("3xx はエラー");
    assert_eq!(err.code, ErrorCode::E2003, "{err:?}");
    assert!(err.detail.as_deref().unwrap_or_default().contains("302"));
}

/// `never` でも 2xx はそのまま成功する。
#[test]
fn never_policy_allows_2xx() {
    let server = seq_server(&[&response("HTTP/1.1 200 OK", &[], "ok")]);
    let mut profile = test_profile();
    profile.redirect_policy = RedirectPolicy::Never;

    let response = fixed_client()
        .send_blocking(&profile, &get(&server.url))
        .expect("2xx は成功");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"ok");
}

/// `manual` は追わずに `Location` を最終 URL として返す（本文は空）。
#[test]
fn manual_policy_returns_location_without_body() {
    let server = seq_server(&[&response("HTTP/1.1 302 Found", &["Location: /moved"], "")]);
    let mut profile = test_profile();
    profile.redirect_policy = RedirectPolicy::Manual;

    let response = fixed_client()
        .send_blocking(&profile, &get(&server.url))
        .expect("manual はエラーにしない");
    assert_eq!(response.status, 302);
    assert!(response.body.is_empty(), "本文は読まない");
    assert!(
        response.final_url.as_str().ends_with("/moved"),
        "Location を解決して返す: {}",
        response.final_url
    );
    assert_eq!(server.requests(), 1, "追従しない");
}

/// `maxRedirects=0` は HTML 中継ページの追従も行わない。
#[test]
fn max_redirects_zero_stops_html_redirect() {
    let mid = r#"<html lang="ja"><head><title>中継</title>
<meta http-equiv="refresh" content="0;url=https://site.test/target"></head>
<body><p>移動します</p></body></html>"#;
    let h = support::harness(None);
    // キーの辞書順で "/target" が "site.test/mid" より先に評価される
    h.http
        .route("/target", 200, "text/html; charset=utf-8", support::ARTICLE);
    h.http
        .route("site.test/mid", 200, "text/html; charset=utf-8", mid);

    let url: reqwest::Url = "https://site.test/mid".parse().unwrap();
    let fetcher = HttpFetcher::new(
        Arc::clone(&h.app.http),
        None,
        Arc::clone(&h.app.clock),
        Arc::clone(&h.app.reporter),
    );

    let mut profile = test_profile();
    profile.max_redirects = 0;
    let stopped = fetcher
        .fetch(
            &url,
            &FetchOptions {
                profile: profile.clone(),
                ttl_hours: 24,
                ..FetchOptions::default()
            },
        )
        .expect("中継ページそのものを返す");
    assert_eq!(stopped.meta.final_url, "https://site.test/mid");

    profile.max_redirects = 1;
    let followed = fetcher
        .fetch(
            &url,
            &FetchOptions {
                profile,
                ttl_hours: 24,
                ..FetchOptions::default()
            },
        )
        .expect("追従先を返す");
    assert_eq!(followed.meta.final_url, "https://site.test/target");
}

// ---------------------------------------------------------------------------
// #13: robots.txt
// ---------------------------------------------------------------------------

/// `robots` の経路を登録する（キーは辞書順で robots.txt が先に一致する）。
fn route_robots(h: &support::Harness, robots_txt: &str) {
    h.http
        .route("/robots.txt", 200, "text/plain; charset=utf-8", robots_txt);
    h.http.route(
        "site.test/public",
        200,
        "text/html; charset=utf-8",
        support::ARTICLE,
    );
    h.http.route(
        "site.test/private",
        200,
        "text/html; charset=utf-8",
        support::ARTICLE,
    );
}

fn fetch_url(h: &mut support::Harness, url: &str) -> Result<(), ErrorCode> {
    let url: reqwest::Url = url.parse().unwrap();
    document::fetch(&mut h.app, &url)
        .map(|_| ())
        .map_err(|e| e.code)
}

/// `Disallow` に該当するパスは取得せず `E2006` を返す。
#[test]
fn robots_denies_disallowed_path() {
    let mut h = support::harness(None);
    route_robots(&h, "User-agent: *\nDisallow: /private\n");

    assert_eq!(
        fetch_url(&mut h, "https://site.test/private/page"),
        Err(ErrorCode::E2006)
    );
    assert_eq!(fetch_url(&mut h, "https://site.test/public/page"), Ok(()));
}

/// robots.txt が取れない場合は許可する（`onError=allow` が既定）。
#[test]
fn robots_allows_when_unavailable() {
    let mut h = support::harness(None);
    h.http.route(
        "site.test/public",
        200,
        "text/html; charset=utf-8",
        support::ARTICLE,
    );
    assert_eq!(fetch_url(&mut h, "https://site.test/public/page"), Ok(()));
}

/// `onError=deny` は robots.txt が取れないホストを拒否する。
#[test]
fn robots_on_error_deny_blocks_unavailable_host() {
    let mut h = support::harness(None);
    h.http.route(
        "site.test/public",
        200,
        "text/html; charset=utf-8",
        support::ARTICLE,
    );
    h.app.network.robots.on_error = "deny".to_string();
    assert_eq!(
        fetch_url(&mut h, "https://site.test/public/page"),
        Err(ErrorCode::E2006)
    );
}

/// `respect=false` は判定も robots.txt の取得も行わない。
#[test]
fn robots_respect_false_skips_check() {
    let mut h = support::harness(None);
    route_robots(&h, "User-agent: *\nDisallow: /\n");
    h.app.network.robots.respect = false;

    assert_eq!(fetch_url(&mut h, "https://site.test/private/page"), Ok(()));
    assert_eq!(h.http.request_count(), 1, "robots.txt を取得しない");
    let entry = h
        .app
        .history
        .query(&Default::default())
        .unwrap()
        .into_iter()
        .find(|e| e.kind == llmsearch::core::history::EventType::Fetch)
        .expect("fetch の履歴");
    assert_eq!(entry.payload["robotsSkipped"], json!(true));
}

/// `respect=false` で取得した本文は保存しないが、その場では使える。
#[test]
fn robots_respect_false_can_still_outline() {
    let mut h = support::harness(None);
    route_robots(&h, "User-agent: *\nDisallow: /private\n");
    h.app.network.robots.respect = false;
    let url: reqwest::Url = "https://site.test/private/page".parse().unwrap();
    let fetched = document::fetch(&mut h.app, &url).expect("取得");
    // 保存しないのでキャッシュは空のまま
    assert!(h.app.documents.get(&url).unwrap().is_none());
    // それでもアウトラインは出せる（E3001 にならない）
    let view = document::outline(&mut h.app, &fetched.document, 2).expect("アウトライン");
    assert!(!view.headings.is_empty(), "見出しが出る");
}

/// 判定結果は保持され、同じホストで取り直さない。
#[test]
fn robots_verdict_is_cached() {
    let mut h = support::harness(None);
    route_robots(&h, "User-agent: *\nDisallow: /private\n");
    assert_eq!(fetch_url(&mut h, "https://site.test/public/page"), Ok(()));
    assert_eq!(h.http.request_count(), 2, "robots.txt + 本文");

    // 本文キャッシュを使わず再取得しても robots.txt は取り直さない
    h.app.no_cache = true;
    assert_eq!(fetch_url(&mut h, "https://site.test/public/page"), Ok(()));
    assert_eq!(h.http.request_count(), 3, "robots.txt はキャッシュから");

    let path = h.app.paths.robots_file();
    assert!(
        path.is_file(),
        "キャッシュファイルがある: {}",
        path.display()
    );
}

/// robots.txt の取得はクォータを消費しない。
#[test]
fn robots_fetch_does_not_consume_quota() {
    let mut h = support::harness(None);
    route_robots(&h, "User-agent: *\nDisallow: /private\n");
    assert_eq!(fetch_url(&mut h, "https://site.test/public/page"), Ok(()));
    let counters = h.app.counters.load().unwrap();
    assert_eq!(counters.session.get(Metric::Fetches), 1, "本文取得のみ");
}

// ---------------------------------------------------------------------------
// #18: クォータ消費
// ---------------------------------------------------------------------------

/// `outline` は `chars` を消費しない。`read` で消費する。
#[test]
fn fetch_does_not_consume_chars_but_read_does() {
    let mut h = support::harness(None);
    h.http.route(
        "site.test/public",
        200,
        "text/html; charset=utf-8",
        support::ARTICLE,
    );
    let url: reqwest::Url = "https://site.test/public/page".parse().unwrap();
    let fetched = document::fetch(&mut h.app, &url).expect("取得");
    let outline = document::outline(&mut h.app, &fetched.document, 3).expect("アウトライン");
    assert!(outline.total_chars > 100, "本文は変換されている");
    let counters = h.app.counters.load().unwrap();
    assert_eq!(counters.session.get(Metric::Fetches), 1);
    assert_eq!(
        counters.session.get(Metric::Chars),
        0,
        "取得・アウトラインでは chars を消費しない"
    );

    let section = outline
        .headings
        .iter()
        .find(|h| h.level == 2)
        .expect("h2 が要る");
    let read = document::read(&mut h.app, &fetched.document, &[section.id], true).expect("節読み");
    assert!(read.total_chars > 0);
    let counters = h.app.counters.load().unwrap();
    assert_eq!(
        counters.session.get(Metric::Chars),
        read.total_chars as u64,
        "read で出力した分だけ消費する"
    );
}

/// `results` はフィルタ前の件数を数える。
#[test]
fn results_are_counted_before_filtering() {
    let items = [
        (1usize, "結果 1", "https://site1.example.jp/page"),
        (2, "結果 2", "https://site2.example.jp/page"),
        (3, "結果 3", "https://site3.example.jp/page"),
    ];
    let html = support::duckduckgo_html(&items);
    let mut h = support::harness(None);
    h.http.route(
        "html.duckduckgo.com",
        200,
        "text/html; charset=utf-8",
        &html,
    );

    // site2 を除外するフィルタを設定する
    let set = serde_json::from_value(json!({
        "schemaVersion": 2,
        "id": "test",
        "results": {"deny": [{"host": ["site2.example.jp"]}]},
    }))
    .expect("フィルタ集合の組み立て");
    h.app.filters = CompiledFilters::compile(set).expect("フィルタのコンパイル");

    let outcome = search::run(&mut h.app, "気候変動", 1).expect("検索");
    assert_eq!(outcome.hits.len(), 2, "フィルタで 1 件除外される");
    let counters = h.app.counters.load().unwrap();
    assert_eq!(
        counters.session.get(Metric::Results),
        3,
        "フィルタ前の件数を数える"
    );
}

/// フォールバックで試した回数だけ `searches` を消費する。
#[test]
fn fallback_attempts_each_count_as_one_search() {
    let broken = json!({
        "schemaVersion": 1,
        "id": "broken",
        "request": {"method": "GET", "url": "http://broken.test/search", "query": {"q": "{{query}}"}},
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
    let items = [(1usize, "結果 1", "https://site1.example.jp/page")];
    let html = support::duckduckgo_html(&items);
    let mut h = support::harness(Some(&broken));
    h.http.route(
        "html.duckduckgo.com",
        200,
        "text/html; charset=utf-8",
        &html,
    );
    // 結果要素の無い応答（抽出 0 件 → E4004 → フォールバック）
    h.http.route(
        "broken.test",
        200,
        "text/html; charset=utf-8",
        "<html><body><p>no results</p></body></html>",
    );
    h.app.network.retry.max_attempts = 1;
    h.app.default_engine = "broken".to_string();
    h.app.settings.search.fallback = vec!["duckduckgo".to_string()];

    let outcome = search::run(&mut h.app, "気候変動", 1).expect("フォールバックで成功");
    assert_eq!(outcome.engine, "duckduckgo");
    assert_eq!(outcome.fallback_from.as_deref(), Some("broken"));
    let counters = h.app.counters.load().unwrap();
    assert_eq!(
        counters.session.get(Metric::Searches),
        2,
        "試した 2 エンジン分を数える"
    );
}

/// 全ての候補が失敗した場合は `searches` を消費しない。
#[test]
fn failed_search_consumes_nothing() {
    let broken = json!({
        "schemaVersion": 1,
        "id": "broken",
        "request": {"method": "GET", "url": "http://broken.test/search", "query": {"q": "{{query}}"}},
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
    let mut h = support::harness(Some(&broken));
    h.http.route(
        "broken.test",
        200,
        "text/html; charset=utf-8",
        "<html><body><p>no results</p></body></html>",
    );
    h.app.network.retry.max_attempts = 1;
    h.app.default_engine = "broken".to_string();

    let err = search::run(&mut h.app, "気候変動", 1).expect_err("失敗する");
    assert_eq!(err.code, ErrorCode::E4004, "{err:?}");
    let counters = h.app.counters.load().unwrap();
    assert_eq!(
        counters.session.get(Metric::Searches),
        0,
        "操作が失敗したので消費しない"
    );
}
