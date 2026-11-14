//! #14: 外部ネットワークを使わず、実 reqwest のプロキシ経路を検証する。
mod support;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use llmsearch::adapters::http::ReqwestClient;
use llmsearch::core::config::NetworkProfile;
use llmsearch::core::errors::ErrorCode;
use llmsearch::ports::HttpClient;
use llmsearch::ports::http::HttpRequest;
use llmsearch::ports::system::{in_no_proxy, proxy_url_for_scheme};
use serde_json::json;

fn server(response: String) -> (String, JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let handle = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        Instant::now() < deadline,
                        "ローカルサーバーに接続されなかった"
                    );
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(e) => panic!("accept: {e}"),
            }
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            if stream.read(&mut byte).unwrap() == 0 {
                break;
            }
            request.push(byte[0]);
            assert!(request.len() < 32_768);
        }
        stream.write_all(response.as_bytes()).unwrap();
        String::from_utf8(request).unwrap()
    });
    (format!("http://{addr}"), handle)
}

fn ok_server() -> (String, JoinHandle<String>) {
    server("HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok".into())
}

fn env() -> support::FakeEnv {
    support::FakeEnv::new(std::path::PathBuf::new())
}

fn client(env: support::FakeEnv) -> ReqwestClient {
    ReqwestClient::new(Arc::new(support::FixedClock::new("2026-11-14T00:00:00Z")))
        .with_env(Arc::new(env))
}

fn profile(value: serde_json::Value) -> NetworkProfile {
    let mut profile: NetworkProfile = serde_json::from_value(value).unwrap();
    profile.retry.enabled = false;
    profile.min_interval_ms = 0;
    profile.timeout_connect_ms = 1500;
    profile.timeout_total_ms = 3000;
    profile
}

fn request(url: &str) -> HttpRequest {
    HttpRequest::get(url.parse().unwrap())
}

#[test]
fn http_proxy_env_is_applied_to_real_client() {
    for variable in ["HTTP_PROXY", "http_proxy"] {
        let (proxy, received) = ok_server();
        let response = client(env().with(variable, &proxy))
            .send_blocking(
                &profile(json!({})),
                &request("http://unresolvable.invalid/doc"),
            )
            .unwrap();
        assert_eq!(response.body, b"ok");
        assert!(
            received
                .join()
                .unwrap()
                .starts_with("GET http://unresolvable.invalid/doc HTTP/1.1")
        );
    }
}

#[test]
fn https_proxy_env_uses_connect() {
    for variable in ["HTTPS_PROXY", "https_proxy"] {
        let (proxy, received) = server(
            "HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
        );
        let outcome = client(env().with(variable, &proxy)).send_blocking(
            &profile(json!({})),
            &request("https://unresolvable.invalid/doc"),
        );
        assert!(outcome.is_err()); // TLS トンネルの接続試行だけを検証する
        assert!(
            received
                .join()
                .unwrap()
                .starts_with("CONNECT unresolvable.invalid:443 HTTP/1.1")
        );
    }
}

#[test]
fn explicit_profile_overrides_proxy_and_no_proxy_environment() {
    let (proxy, received) = ok_server();
    let c = client(
        env()
            .with("HTTP_PROXY", "http://127.0.0.1:1")
            .with("NO_PROXY", "*"),
    );
    let response = c
        .send_blocking(
            &profile(json!({"proxy": proxy})),
            &request("http://unresolvable.invalid/doc"),
        )
        .unwrap();
    assert_eq!(response.status, 200);
    assert!(
        received
            .join()
            .unwrap()
            .starts_with("GET http://unresolvable.invalid/doc")
    );
}

#[test]
fn proxy_password_from_environment_is_applied_without_origin_auth() {
    let (proxy, received) = ok_server();
    let proxy = proxy.replacen("http://", "http://user@", 1);
    let p = profile(json!({"proxy":{"url":proxy,"auth":{"value_env":"PROXY_PASS"}}}));
    let response = client(env().with("PROXY_PASS", "secret"))
        .send_blocking(&p, &request("http://unresolvable.invalid/doc"))
        .unwrap();
    assert_eq!(response.status, 200);
    let received = received.join().unwrap().to_lowercase();
    assert!(received.contains("proxy-authorization: basic dxnlcjpzzwnyzxq="));
    assert!(
        !received
            .lines()
            .any(|line| line.starts_with("authorization:"))
    );
    assert!(!format!("{response:?}").contains("secret"));
}

#[test]
fn missing_password_or_invalid_proxy_fails_closed_without_secrets() {
    let p = profile(
        json!({"proxy":{"url":"http://user@proxy.invalid:80", "auth":{"value_env":"MISSING"}}}),
    );
    let error = client(env())
        .send_blocking(&p, &request("http://unresolvable.invalid/doc"))
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::E1004);
    let p = profile(json!({"proxy":"http://user:very-secret@[bad"}));
    let error = client(env())
        .send_blocking(&p, &request("http://unresolvable.invalid/doc"))
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::E1004);
    assert!(!format!("{error:?}").contains("very-secret"));
}

#[test]
fn no_proxy_is_rechecked_for_redirect_and_auth_does_not_leak() {
    let (origin, origin_received) = ok_server();
    let (proxy, proxy_received) = server(format!(
        "HTTP/1.1 302 Found\r\nLocation: {origin}/direct\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    ));
    let proxy = proxy.replacen("http://", "http://user:secret@", 1);
    let p = profile(json!({"proxy":{"url":proxy,"noProxy":["127.0.0.1"]}}));
    let response = client(env())
        .send_blocking(&p, &request("http://unresolvable.invalid/doc"))
        .unwrap();
    assert_eq!(response.body, b"ok");
    assert!(
        proxy_received
            .join()
            .unwrap()
            .to_lowercase()
            .contains("proxy-authorization:")
    );
    let origin_request = origin_received.join().unwrap().to_lowercase();
    assert!(origin_request.starts_with("get /direct http/1.1"));
    assert!(!origin_request.contains("authorization:"));
}

#[test]
fn environment_no_proxy_bypasses_proxy() {
    let (origin, received) = ok_server();
    let c = client(
        env()
            .with("HTTP_PROXY", "http://127.0.0.1:1")
            .with("NO_PROXY", "127.0.0.1"),
    );
    assert_eq!(
        c.send_blocking(&profile(json!({})), &request(&origin))
            .unwrap()
            .status,
        200
    );
    assert!(received.join().unwrap().starts_with("GET / HTTP/1.1"));
}

#[test]
fn scheme_selection_and_exclusions_follow_specification() {
    let e = env()
        .with("HTTP_PROXY", "http://http-proxy.invalid")
        .with("HTTPS_PROXY", "http://https-proxy.invalid");
    let p = profile(json!({}));
    assert_eq!(
        proxy_url_for_scheme(&p, "http", &e).as_deref(),
        Some("http://http-proxy.invalid")
    );
    assert_eq!(
        proxy_url_for_scheme(&p, "https", &e).as_deref(),
        Some("http://https-proxy.invalid")
    );
    let p = profile(
        json!({"proxy":{"url":"http://proxy.invalid", "noProxy":["exact.example", ".internal", "UPPER.EXAMPLE"]}}),
    );
    for (host, expected) in [
        ("exact.example", true),
        ("sub.exact.example", false),
        ("a.internal", true),
        ("internal", false),
        ("notinternal", false),
        ("upper.example", true),
        ("external.example", false),
    ] {
        assert_eq!(in_no_proxy(&p, host, &e), expected, "{host}");
    }
    let p = profile(json!({"proxy":{"url":"http://proxy.invalid", "noProxy":["*"]}}));
    assert!(in_no_proxy(&p, "anything.invalid", &e));
}

#[test]
fn transport_errors_do_not_include_request_url_secrets() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    let p = profile(json!({"proxy":format!("http://user:proxy-secret@{addr}")}));
    let error = client(env())
        .send_blocking(
            &p,
            &request("http://unresolvable.invalid/private-path?token=request-secret"),
        )
        .unwrap_err();
    let diagnostic = format!("{error:?}");
    assert!(!diagnostic.contains("request-secret"));
    assert!(!diagnostic.contains("private-path"));
    assert!(!diagnostic.contains("proxy-secret"));
}
