//! M17 実プロセス回帰テスト:
//! - G1 (#50): URL userinfo（成功系・302/meta リダイレクト・埋込リダイレクト URL `W4006`・
//!   不正ポート/非 HTTP URL・robots 拒否 `E2006`・相対リンク認証情報非伝播・履歴）
//! - G2 (#51): 送信/応答ヘッダ・JSON body・form body の秘匿、切り詰め前 `redact_body`、
//!   form テンプレート `{{query}}` 温存、通常値への過剰マスク非発生

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};

#[derive(Debug, Clone)]
struct StubResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

impl StubResponse {
    fn html(body: impl Into<String>) -> Self {
        Self {
            status: 200,
            headers: vec![("Content-Type".into(), "text/html; charset=utf-8".into())],
            body: body.into(),
        }
    }

    fn json(body: impl Into<String>) -> Self {
        Self {
            status: 200,
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: body.into(),
        }
    }

    fn redirect(location: impl Into<String>) -> Self {
        Self {
            status: 302,
            headers: vec![("Location".into(), location.into())],
            body: String::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct RecordedRequest {
    method: String,
    target: String,
    headers: BTreeMap<String, String>,
    body: String,
}

struct LoopbackStub {
    port: u16,
    stop: Arc<AtomicBool>,
    recorded: Arc<Mutex<Vec<RecordedRequest>>>,
    handle: Option<thread::JoinHandle<()>>,
}

impl LoopbackStub {
    fn start<F>(handler: F) -> Self
    where
        F: Fn(&RecordedRequest) -> StubResponse + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        listener
            .set_nonblocking(true)
            .expect("set nonblocking listener");
        let port = listener.local_addr().expect("local addr").port();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let recorded_sink = Arc::clone(&recorded);
        let handler = Arc::new(handler);

        let handle = thread::spawn(move || {
            while !stop_flag.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                        if let Some(req) = read_http_request(&mut stream) {
                            let resp = handler(&req);
                            if let Ok(mut guard) = recorded_sink.lock() {
                                guard.push(req);
                            }
                            let reason = match resp.status {
                                200 => "OK",
                                302 => "Found",
                                404 => "Not Found",
                                429 => "Too Many Requests",
                                _ => "Status",
                            };
                            let mut wire = format!(
                                "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n",
                                resp.status,
                                reason,
                                resp.body.len()
                            );
                            for (k, v) in &resp.headers {
                                wire.push_str(&format!("{k}: {v}\r\n"));
                            }
                            wire.push_str("\r\n");
                            wire.push_str(&resp.body);
                            let _ = stream.write_all(wire.as_bytes());
                            let _ = stream.flush();
                        }
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            port,
            stop,
            recorded,
            handle: Some(handle),
        }
    }

    fn requests(&self) -> Vec<RecordedRequest> {
        self.recorded.lock().expect("lock recorded").clone()
    }
}

impl Drop for LoopbackStub {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn read_http_request(stream: &mut std::net::TcpStream) -> Option<RecordedRequest> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        match stream.read(&mut chunk) {
            Ok(0) => return None,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if let Some(pos) = find_subsequence(&buf, b"\r\n\r\n") {
                    break pos + 4;
                }
                if buf.len() > 65_536 {
                    return None;
                }
            }
            Err(_) => return None,
        }
    };

    let head_str = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = head_str.split("\r\n");
    let request_line = lines.next()?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?.to_string();

    let mut headers = BTreeMap::new();
    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }

    let content_len = headers
        .get("content-length")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    while buf.len() < header_end + content_len {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(_) => break,
        }
    }
    let body_end = (header_end + content_len).min(buf.len());
    let body = String::from_utf8_lossy(&buf[header_end..body_end]).to_string();
    Some(RecordedRequest {
        method,
        target,
        headers,
        body,
    })
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_llmsearch")
}

struct EnvSetup {
    _dir: tempfile::TempDir,
    state: std::path::PathBuf,
    engines: std::path::PathBuf,
    config: std::path::PathBuf,
}

impl EnvSetup {
    fn new(default_engine: &str) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let state = dir.path().join("state");
        let engines = dir.path().join("engines");
        std::fs::create_dir_all(state.join("config/network")).expect("mkdir network");
        std::fs::create_dir_all(state.join("config/filters")).expect("mkdir filters");
        std::fs::create_dir_all(&engines).expect("mkdir engines");
        let config = state.join("config/settings.json");
        std::fs::write(
            &config,
            serde_json::to_string_pretty(&json!({
                "schemaVersion": 2,
                "search": { "engine": default_engine }
            }))
            .unwrap(),
        )
        .expect("write settings");
        Self {
            _dir: dir,
            state,
            engines,
            config,
        }
    }

    fn write_engine(&self, id: &str, value: Value) {
        std::fs::write(
            self.engines.join(format!("{id}.json")),
            serde_json::to_string_pretty(&value).unwrap(),
        )
        .expect("write engine");
    }

    fn run(&self, args: &[&str], envs: &[(&str, &str)]) -> Output {
        let mut cmd = Command::new(bin());
        for (key, _) in std::env::vars() {
            if key.starts_with("LLMSEARCH_") || key.to_ascii_lowercase().ends_with("_proxy") {
                cmd.env_remove(key);
            }
        }
        cmd.current_dir(env!("CARGO_MANIFEST_DIR"))
            .env("NO_PROXY", "127.0.0.1,localhost")
            .env("no_proxy", "127.0.0.1,localhost")
            .args([
                "--state-dir",
                self.state.to_str().unwrap(),
                "--config",
                self.config.to_str().unwrap(),
                "--engines-dir",
                self.engines.to_str().unwrap(),
            ])
            .args(args);
        for (k, v) in envs {
            cmd.env(k, v);
        }
        cmd.output().expect("spawn llmsearch")
    }
}

fn parse_jsonl(stdout: &[u8]) -> Vec<Value> {
    let text = String::from_utf8_lossy(stdout);
    assert!(!text.trim().is_empty(), "expected non-empty JSONL output");
    text.lines()
        .map(|line| {
            serde_json::from_str::<Value>(line)
                .unwrap_or_else(|e| panic!("invalid JSON line `{line}`: {e}"))
        })
        .collect()
}

fn assert_no_secrets(haystack: &str, secrets: &[&str], context: &str) {
    for secret in secrets {
        assert!(
            !haystack.contains(secret),
            "secret `{secret}` leaked in {context}:\n{haystack}"
        );
    }
}

fn collect_files_text(dir: &Path) -> String {
    let mut acc = String::new();
    if !dir.exists() {
        return acc;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                acc.push_str(&collect_files_text(&path));
            } else if let Ok(text) = std::fs::read_to_string(&path) {
                acc.push_str(&text);
                acc.push('\n');
            }
        }
    }
    acc
}

/// G1 (#50): userinfo URL の成功系（outline / read / 302・meta リダイレクト / search・W4006・履歴・キャッシュ）
/// で認証情報や秘匿クエリが一切露出せず、通常パラメータと本文・文字数が保持されることを検証する。
#[test]
fn g1_userinfo_success_paths_outline_read_search_redirect_and_history_do_not_leak() {
    let stub = LoopbackStub::start(move |req| {
        let host = req
            .headers
            .get("host")
            .cloned()
            .unwrap_or_else(|| "127.0.0.1".to_string());
        if req.target == "/robots.txt" {
            return StubResponse {
                status: 200,
                headers: vec![("Content-Type".into(), "text/plain".into())],
                body: "User-agent: *\nAllow: /\n".into(),
            };
        }
        if req.target.starts_with("/redir-userinfo") {
            return StubResponse::redirect(format!(
                "http://rediruser:redirpass456@{host}/ok-doc?normal=keepredir"
            ));
        }
        if req.target.starts_with("/meta-userinfo") {
            return StubResponse::html(format!(
                "<html><head><meta http-equiv=\"refresh\" content=\"0; url=http://metauser:metapass789@{host}/ok-doc?normal=keepmeta\"></head><body></body></html>"
            ));
        }
        if req.target.starts_with("/search-userinfo") {
            let wrapped = format!(
                "http://{host}/l/?uddg=http%3A%2F%2Fwrapuser%3Awrappass321%40{host}%2Fok-doc%3Ftoken%3DWRAPTOK123%26normal%3Dkeepwrap"
            );
            return StubResponse::html(format!(
                "<html><body><ul>\
                 <li><a href=\"http://hituser:hitpass654@{host}/ok-doc?api_key=HITKEY654&normal=keephit\">Direct Userinfo Hit</a><p>Direct snippet</p></li>\
                 <li><a href=\"{wrapped}\">Wrapped Userinfo Hit</a><p>Wrapped snippet</p></li>\
                 </ul></body></html>"
            ));
        }
        if req.target.starts_with("/ok-doc") {
            let padding = " readable body words".repeat(20);
            return StubResponse::html(format!(
                "<html><head><title>Loopback Doc</title></head>\
                 <body><main><h1>Overview</h1>\
                 <p>Keep ordinary marker NORMALBODYMARKER and <a href=\"/rel-path\">rel link</a>.{padding}</p>\
                 </main></body></html>"
            ));
        }
        StubResponse {
            status: 404,
            headers: vec![],
            body: "not found".into(),
        }
    });

    let env = EnvSetup::new("uinfo-search");
    env.write_engine(
        "uinfo-search",
        json!({
            "schemaVersion": 1,
            "id": "uinfo-search",
            "network": "default",
            "request": {
                "method": "GET",
                "url": format!("http://enguser:engpass987@127.0.0.1:{}/search-userinfo", stub.port),
                "query": { "q": "{{query}}" }
            },
            "response": {
                "kind": "html",
                "items": { "selector": "li" },
                "fields": {
                    "title": { "selector": "a" },
                    "url": { "selector": "a", "attr": "href" },
                    "snippet": { "selector": "p" }
                }
            },
            "limits": { "maxResults": 10, "maxPageSize": 10 }
        }),
    );

    let forbidden = [
        "uinfo_user",
        "pass123secret",
        "QUERYTOK100",
        "onlyuser999",
        "onlypass999",
        "rediruser",
        "redirpass456",
        "metauser",
        "metapass789",
        "hituser",
        "hitpass654",
        "HITKEY654",
        "wrapuser",
        "wrappass321",
        "WRAPTOK123",
        "enguser",
        "engpass987",
    ];

    // 1. Direct userinfo URL: outline (--json) -> read D1 (--json & text)
    let target_url = format!(
        "http://uinfo_user:pass123secret@127.0.0.1:{}/ok-doc?token=QUERYTOK100&normal=keepme",
        stub.port
    );
    let out_outline = env.run(
        &["--json", "--network", "default", "outline", &target_url],
        &[],
    );
    assert_eq!(out_outline.status.code(), Some(0));
    let stdout_str = String::from_utf8_lossy(&out_outline.stdout);
    let stderr_str = String::from_utf8_lossy(&out_outline.stderr);
    assert_no_secrets(&stdout_str, &forbidden, "outline --json stdout");
    assert_no_secrets(&stderr_str, &forbidden, "outline --json stderr");
    let events = parse_jsonl(&out_outline.stdout);
    let doc_ev = events.iter().find(|e| e["event"] == "document").unwrap();
    assert_eq!(doc_ev["untrusted"], json!(true));
    assert_eq!(
        doc_ev["url"],
        json!(format!(
            "http://***:***@127.0.0.1:{}/ok-doc?token=***&normal=keepme",
            stub.port
        ))
    );
    let session_id = doc_ev["session"].as_str().unwrap().to_string();

    // Confirm the stub genuinely received HTTP Basic auth over the wire!
    let reqs = stub.requests();
    assert!(
        reqs.iter()
            .any(|r| r.target.starts_with("/ok-doc") && r.headers.contains_key("authorization")),
        "expected HTTP client to transmit Basic auth to loopback stub: {reqs:?}"
    );

    // Read D1 in the same session (JSON + text)
    let out_read_d1 = env.run(
        &[
            "--json",
            "--network",
            "default",
            "--session",
            &session_id,
            "read",
            "D1",
        ],
        &[],
    );
    assert_eq!(out_read_d1.status.code(), Some(0));
    let read_stdout = String::from_utf8_lossy(&out_read_d1.stdout);
    let read_stderr = String::from_utf8_lossy(&out_read_d1.stderr);
    assert_no_secrets(&read_stdout, &forbidden, "read D1 --json stdout");
    assert_no_secrets(&read_stderr, &forbidden, "read D1 --json stderr");
    assert!(
        read_stdout.contains("NORMALBODYMARKER"),
        "actual read_stdout:\n{read_stdout}"
    );

    let out_read_txt = env.run(
        &[
            "--network",
            "default",
            "--session",
            &session_id,
            "read",
            "D1",
        ],
        &[],
    );
    assert_eq!(out_read_txt.status.code(), Some(0));
    let txt_stdout = String::from_utf8_lossy(&out_read_txt.stdout);
    let txt_stderr = String::from_utf8_lossy(&out_read_txt.stderr);
    assert_no_secrets(&txt_stdout, &forbidden, "read D1 text stdout");
    assert_no_secrets(&txt_stderr, &forbidden, "read D1 text stderr");
    assert!(txt_stdout.contains("[untrusted]"));
    assert!(txt_stdout.contains("NORMALBODYMARKER"));

    // 2. Username-only and password-only URLs
    for u in [
        format!("http://onlyuser999@127.0.0.1:{}/ok-doc", stub.port),
        format!("http://:onlypass999@127.0.0.1:{}/ok-doc", stub.port),
    ] {
        let res = env.run(&["--json", "--network", "self-hosted", "outline", &u], &[]);
        assert_eq!(res.status.code(), Some(0));
        assert_no_secrets(
            &String::from_utf8_lossy(&res.stdout),
            &forbidden,
            "user/pass-only stdout",
        );
        assert_no_secrets(
            &String::from_utf8_lossy(&res.stderr),
            &forbidden,
            "user/pass-only stderr",
        );
    }

    // 3. 302 redirect and meta-refresh to userinfo URL: relative links & cached body.md must NOT inherit userinfo
    for path in ["/redir-userinfo", "/meta-userinfo"] {
        let u = format!(
            "http://uinfo_user:pass123secret@127.0.0.1:{}{path}",
            stub.port
        );
        let res = env.run(&["--json", "--network", "default", "read", &u], &[]);
        assert_eq!(res.status.code(), Some(0));
        let out_s = String::from_utf8_lossy(&res.stdout);
        let err_s = String::from_utf8_lossy(&res.stderr);
        assert_no_secrets(&out_s, &forbidden, "redirect read stdout");
        assert_no_secrets(&err_s, &forbidden, "redirect read stderr");
        let evs = parse_jsonl(&res.stdout);
        let content = evs.iter().find(|e| e["event"] == "content").unwrap();
        let body_text = content["text"].as_str().unwrap();
        assert!(
            body_text.contains(&format!(
                "[rel link](http://127.0.0.1:{}/rel-path)",
                stub.port
            )),
            "relative link should not carry base userinfo: {body_text}"
        );
        assert_eq!(
            body_text.chars().count(),
            content["chars"].as_u64().unwrap() as usize
        );
    }
    let cached_docs_text = collect_files_text(&env.state.join("documents"));
    for body_file in std::fs::read_dir(env.state.join("documents"))
        .unwrap()
        .flatten()
    {
        let md_path = body_file.path().join("body.md");
        if md_path.exists() {
            let md = std::fs::read_to_string(md_path).unwrap();
            assert_no_secrets(&md, &forbidden, "cached body.md");
        }
    }
    let _ = cached_docs_text;

    // 4. Search with direct userinfo hit + wrapped redirect userinfo hit (W4006)
    let out_search = env.run(
        &["--json", "--network", "default", "search", "normalquery"],
        &[],
    );
    assert_eq!(out_search.status.code(), Some(0));
    let s_out = String::from_utf8_lossy(&out_search.stdout);
    let s_err = String::from_utf8_lossy(&out_search.stderr);
    assert_no_secrets(&s_out, &forbidden, "search --json stdout");
    assert_no_secrets(&s_err, &forbidden, "search --json stderr");
    assert!(s_out.contains("W4006"), "expected W4006 warning: {s_out}");
    assert!(s_out.contains("normal=keephit"), "{s_out}");
    assert!(s_out.contains("normal=keepwrap"), "{s_out}");
    assert!(s_out.contains("normal%3Dkeepwrap"), "{s_out}");

    // 5. History commands and raw history.jsonl on disk
    for args in [
        vec!["--json", "history"],
        vec!["history"],
        vec!["--json", "history", "export", "--format", "markdown"],
        vec!["history", "export", "--format", "jsonl"],
    ] {
        let h = env.run(&args, &[]);
        assert_eq!(h.status.code(), Some(0));
        assert_no_secrets(
            &String::from_utf8_lossy(&h.stdout),
            &forbidden,
            "history stdout",
        );
        assert_no_secrets(
            &String::from_utf8_lossy(&h.stderr),
            &forbidden,
            "history stderr",
        );
    }
    let disk_history =
        std::fs::read_to_string(env.state.join("history/history.jsonl")).expect("history.jsonl");
    assert_no_secrets(&disk_history, &forbidden, "on-disk history.jsonl");
}

/// G1 (#50): 構文エラー URL (`E2005`)、非 HTTP スキーム (`E1002`)、接続拒否 (`E2001`)、
/// および `robots.txt` 拒否 (`E2006`) で userinfo・秘匿パラメータが漏れないことを検証する。
#[test]
fn g1_malformed_non_http_and_robots_disallow_paths_redact_credentials_and_tokens() {
    let stub = LoopbackStub::start(move |req| {
        if req.target == "/robots.txt" {
            return StubResponse {
                status: 200,
                headers: vec![("Content-Type".into(), "text/plain".into())],
                body: "User-agent: *\nDisallow: /disallowed\nAllow: /ok\n".into(),
            };
        }
        StubResponse::html(
            "<html><title>Seed</title><body><main><h1>Seed</h1><p>seed text</p></main></body></html>",
        )
    });

    let closed_port = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };

    let env = EnvSetup::new("duckduckgo");
    let forbidden = [
        "badportuser",
        "badportpass999",
        "BADPORTTOK",
        "ftpuser",
        "ftppass999",
        "FTPTOK",
        "connuser",
        "connpass999",
        "CONNTOK",
        "robouser",
        "robopass999",
        "ROBOTTOK999",
    ];

    // 1. Invalid port URL -> E2005
    let bad_port =
        "http://badportuser:badportpass999@127.0.0.1:99999/doc?token=BADPORTTOK&normal=keepbp";
    let res_bp = env.run(&["--json", "outline", bad_port], &[]);
    let bp_out = String::from_utf8_lossy(&res_bp.stdout);
    let bp_err = String::from_utf8_lossy(&res_bp.stderr);
    assert_eq!(
        res_bp.status.code(),
        Some(2),
        "bp_out={bp_out} bp_err={bp_err}"
    );
    assert_no_secrets(&bp_out, &forbidden, "E2005 stdout");
    assert_no_secrets(&bp_err, &forbidden, "E2005 stderr");
    assert!(bp_out.contains("http://***:***@127.0.0.1:99999/doc?token=***&normal=keepbp"));

    // 2a. Non-HTTP scheme URL without existing session -> E5001 (no state leak, no credential leak)
    let ftp_url = "ftp://ftpuser:ftppass999@127.0.0.1/doc?token=FTPTOK&normal=keepftp";
    let res_ftp_nosess = env.run(&["--json", "outline", ftp_url], &[]);
    assert_eq!(res_ftp_nosess.status.code(), Some(5));
    assert_no_secrets(
        &String::from_utf8_lossy(&res_ftp_nosess.stdout),
        &forbidden,
        "E5001 ftp stdout",
    );
    assert_no_secrets(
        &String::from_utf8_lossy(&res_ftp_nosess.stderr),
        &forbidden,
        "E5001 ftp stderr",
    );

    // Seed a session, then 2b. Non-HTTP scheme URL with existing session -> E1002 with masked ftp://***:***@...
    let seed_url = format!("http://127.0.0.1:{}/ok", stub.port);
    let seed_res = env.run(
        &["--json", "--network", "self-hosted", "outline", &seed_url],
        &[],
    );
    assert_eq!(seed_res.status.code(), Some(0));

    let res_ftp = env.run(&["--json", "outline", ftp_url], &[]);
    assert_eq!(res_ftp.status.code(), Some(1));
    let ftp_out = String::from_utf8_lossy(&res_ftp.stdout);
    let ftp_err = String::from_utf8_lossy(&res_ftp.stderr);
    assert_no_secrets(&ftp_out, &forbidden, "E1002 ftp stdout");
    assert_no_secrets(&ftp_err, &forbidden, "E1002 ftp stderr");
    assert!(ftp_out.contains("ftp://***:***@127.0.0.1/doc?token=***&normal=keepftp"));

    // 3. Connection error on closed loopback port -> E2001
    let conn_url = format!(
        "http://connuser:connpass999@127.0.0.1:{closed_port}/doc?api_key=CONNTOK&normal=keepconn"
    );
    let res_conn = env.run(
        &["--json", "--network", "self-hosted", "outline", &conn_url],
        &[],
    );
    assert_eq!(res_conn.status.code(), Some(2));
    assert_no_secrets(
        &String::from_utf8_lossy(&res_conn.stdout),
        &forbidden,
        "E2001 stdout",
    );
    assert_no_secrets(
        &String::from_utf8_lossy(&res_conn.stderr),
        &forbidden,
        "E2001 stderr",
    );

    // 4. robots.txt disallow -> E2006 with preserved normal query param
    let robo_url = format!(
        "http://robouser:robopass999@127.0.0.1:{}/disallowed?token=ROBOTTOK999&normal=keeprobo",
        stub.port
    );
    let res_robo = env.run(
        &["--json", "--network", "default", "outline", &robo_url],
        &[],
    );
    assert_eq!(res_robo.status.code(), Some(2));
    let robo_out = String::from_utf8_lossy(&res_robo.stdout);
    let robo_err = String::from_utf8_lossy(&res_robo.stderr);
    assert_no_secrets(&robo_out, &forbidden, "E2006 stdout");
    assert_no_secrets(&robo_err, &forbidden, "E2006 stderr");
    assert!(
        robo_out.contains("/disallowed?token=***&normal=keeprobo"),
        "{robo_out}"
    );
}

/// G2 (#51): 送信側のヘッダ・JSON body・form body の秘匿と、通常値・テンプレート変数 `{{query}}`
/// の非破壊（過剰マスクなし）を実プロセスとスタブ受信記録の双方で検証する。
#[test]
fn g2_request_headers_json_body_and_form_body_redaction_without_over_redaction() {
    let stub = LoopbackStub::start(move |req| {
        let host = req
            .headers
            .get("host")
            .cloned()
            .unwrap_or_else(|| "127.0.0.1".to_string());
        StubResponse::json(
            json!({
                "items": [{
                    "title": format!("Result via {}", req.method),
                    "url": format!("http://{host}/doc?normal=keephit"),
                    "snippet": "Normal result snippet"
                }]
            })
            .to_string(),
        )
    });

    let env = EnvSetup::new("post-json");
    env.write_engine(
        "post-json",
        json!({
            "schemaVersion": 1,
            "id": "post-json",
            "network": "m17-net",
            "request": {
                "method": "POST",
                "url": format!("http://juser:jpass700@127.0.0.1:{}/api-json", stub.port),
                "query": {
                    "q": "{{query}}",
                    "api_key": "REQ_QUERY_APIKEY_701",
                    "normal_q": "KEEP_QUERY_PARAM"
                },
                "headers": {
                    "Content-Type": "application/json",
                    "Authorization": { "value_env": "M17_BEARER_ENV" },
                    "X-Api-Key": { "value": "REQ_EXPLICIT_APIKEY_702" },
                    "Cookie": "sid=REQ_COOKIE_703",
                    "X-Normal": "KEEP_REQ_HEADER"
                },
                "body": "{\"query\":\"{{query}}\",\"api_key\":\"REQ_BODY_APIKEY_704\",\"nested\":{\"password\":\"REQ_BODY_PASS_705\",\"keep\":\"KEEP_JSON_NESTED\"},\"value\":\"KEEP_JSON_VALUE\"}"
            },
            "response": {
                "kind": "json",
                "items": { "path": "items" },
                "fields": {
                    "title": { "path": "title" },
                    "url": { "path": "url" },
                    "snippet": { "path": "snippet" }
                }
            },
            "limits": { "maxResults": 10, "maxPageSize": 10 }
        }),
    );

    env.write_engine(
        "post-form",
        json!({
            "schemaVersion": 1,
            "id": "post-form",
            "network": "m17-net",
            "request": {
                "method": "POST",
                "url": format!("http://fuser:fpass800@127.0.0.1:{}/api-form", stub.port),
                "headers": {
                    "Content-Type": "application/x-www-form-urlencoded",
                    "X-Auth-Token": "REQ_FORM_HDR_801",
                    "X-Normal": "KEEP_FORM_HEADER"
                },
                "body": "q={{query}}&api_key=REQ_FORM_KEY_802&password=REQ_FORM_PASS_803&normal=KEEP_FORM_BODY"
            },
            "response": {
                "kind": "json",
                "items": { "path": "items" },
                "fields": {
                    "title": { "path": "title" },
                    "url": { "path": "url" },
                    "snippet": { "path": "snippet" }
                }
            },
            "limits": { "maxResults": 10, "maxPageSize": 10 }
        }),
    );

    std::fs::write(
        env.state.join("config/network/m17-net.json"),
        serde_json::to_string_pretty(&json!({
            "schemaVersion": 2,
            "id": "m17-net",
            "robots": { "respect": false },
            "retry": { "enabled": false, "maxAttempts": 1 },
            "headers": {
                "Proxy-Authorization": "Basic NET_PROXY_AUTH_901",
                "X-Net-Normal": "KEEP_NET_HEADER"
            }
        }))
        .unwrap(),
    )
    .unwrap();

    std::fs::write(
        env.state.join("config/filters/m17-filter.json"),
        serde_json::to_string_pretty(&json!({
            "schemaVersion": 1,
            "id": "m17-filter",
            "description": "Filter referencing http://fltuser:fltpass902@example.com/x?token=FLTTOK903&normal=keepflt"
        }))
        .unwrap(),
    )
    .unwrap();

    let env_vars = [("M17_BEARER_ENV", "Bearer ENV_BEARER_SECRET_706")];
    let forbidden = [
        "juser",
        "jpass700",
        "REQ_QUERY_APIKEY_701",
        "REQ_EXPLICIT_APIKEY_702",
        "REQ_COOKIE_703",
        "REQ_BODY_APIKEY_704",
        "REQ_BODY_PASS_705",
        "ENV_BEARER_SECRET_706",
        "fuser",
        "fpass800",
        "REQ_FORM_HDR_801",
        "REQ_FORM_KEY_802",
        "REQ_FORM_PASS_803",
        "NET_PROXY_AUTH_901",
        "fltuser",
        "fltpass902",
        "FLTTOK903",
    ];

    // 1. engines show (summary, --json, --raw) on both engines — verify NO template corruption on q={{query}}!
    for mode_args in [
        vec!["engines", "show", "post-json"],
        vec!["--json", "engines", "show", "post-json"],
        vec!["engines", "show", "post-json", "--raw"],
        vec!["engines", "show", "post-form"],
        vec!["--json", "engines", "show", "post-form"],
        vec!["engines", "show", "post-form", "--raw"],
    ] {
        let out = env.run(&mode_args, &env_vars);
        assert_eq!(out.status.code(), Some(0));
        let s_out = String::from_utf8_lossy(&out.stdout);
        let s_err = String::from_utf8_lossy(&out.stderr);
        assert_no_secrets(&s_out, &forbidden, &format!("{mode_args:?} stdout"));
        assert_no_secrets(&s_err, &forbidden, &format!("{mode_args:?} stderr"));
    }

    let show_form_json = env.run(&["--json", "engines", "show", "post-form"], &env_vars);
    let show_events = parse_jsonl(&show_form_json.stdout);
    let eng_ev = show_events.iter().find(|e| e["event"] == "engine").unwrap();
    assert_eq!(
        eng_ev["definition"]["request"]["body"],
        json!("q={{query}}&api_key=***&password=***&normal=KEEP_FORM_BODY")
    );
    assert_eq!(
        eng_ev["definition"]["request"]["headers"]["X-Normal"],
        json!("KEEP_FORM_HEADER")
    );

    let show_json_def = env.run(&["--json", "engines", "show", "post-json"], &env_vars);
    let show_json_str = String::from_utf8_lossy(&show_json_def.stdout);
    assert!(show_json_str.contains("M17_BEARER_ENV"));
    assert!(show_json_str.contains("KEEP_REQ_HEADER"));
    assert!(show_json_str.contains("KEEP_JSON_NESTED"));
    assert!(show_json_str.contains("KEEP_JSON_VALUE"));
    assert!(show_json_str.contains("KEEP_QUERY_PARAM"));

    // 2. network show & filters show (text + --json)
    for cmd in [
        vec!["--json", "network", "show", "m17-net"],
        vec!["network", "show", "m17-net"],
        vec!["--json", "filters", "show", "m17-filter"],
        vec!["filters", "show", "m17-filter", "--raw"],
    ] {
        let out = env.run(&cmd, &env_vars);
        assert_eq!(out.status.code(), Some(0));
        let s_out = String::from_utf8_lossy(&out.stdout);
        assert_no_secrets(&s_out, &forbidden, &format!("{cmd:?}"));
    }

    // 3. engines test --dry-run (JSON & form)
    let dry_json = env.run(
        &[
            "--json",
            "--network",
            "m17-net",
            "engines",
            "test",
            "post-json",
            "--query",
            "keepquery",
            "--dry-run",
        ],
        &env_vars,
    );
    assert_eq!(dry_json.status.code(), Some(0));
    let dry_json_s = String::from_utf8_lossy(&dry_json.stdout);
    assert_no_secrets(&dry_json_s, &forbidden, "dry-run post-json");
    assert!(dry_json_s.contains("keepquery"));
    assert!(dry_json_s.contains("KEEP_REQ_HEADER"));
    assert!(dry_json_s.contains("KEEP_JSON_NESTED"));
    assert!(dry_json_s.contains("KEEP_JSON_VALUE"));

    let dry_form = env.run(
        &[
            "--json",
            "--network",
            "m17-net",
            "engines",
            "test",
            "post-form",
            "--query",
            "keepquery",
            "--dry-run",
        ],
        &env_vars,
    );
    assert_eq!(dry_form.status.code(), Some(0));
    let dry_form_s = String::from_utf8_lossy(&dry_form.stdout);
    assert_no_secrets(&dry_form_s, &forbidden, "dry-run post-form");
    assert!(dry_form_s.contains("q=keepquery&api_key=***&password=***&normal=KEEP_FORM_BODY"));

    // 4. Live search & engines test on both POST JSON and POST form engines
    for eid in ["post-json", "post-form"] {
        let live_search = env.run(
            &[
                "--json",
                "--network",
                "m17-net",
                "search",
                "liveq",
                "--engine",
                eid,
            ],
            &env_vars,
        );
        assert_eq!(live_search.status.code(), Some(0));
        assert_no_secrets(
            &String::from_utf8_lossy(&live_search.stdout),
            &forbidden,
            "live search stdout",
        );
        assert_no_secrets(
            &String::from_utf8_lossy(&live_search.stderr),
            &forbidden,
            "live search stderr",
        );

        let live_test = env.run(
            &[
                "--json",
                "--network",
                "m17-net",
                "engines",
                "test",
                eid,
                "--query",
                "liveq",
            ],
            &env_vars,
        );
        assert_eq!(live_test.status.code(), Some(0));
        assert_no_secrets(
            &String::from_utf8_lossy(&live_test.stdout),
            &forbidden,
            "live engines test stdout",
        );
    }

    // Prove the loopback stub actually received the real unredacted headers and bodies over HTTP!
    let wire_reqs = stub.requests();
    assert!(
        wire_reqs.iter().any(|r| {
            r.target.starts_with("/api-json")
                && r.body.contains("REQ_BODY_APIKEY_704")
                && r.headers
                    .get("x-api-key")
                    .is_some_and(|v| v == "REQ_EXPLICIT_APIKEY_702")
        }),
        "stub did not receive real JSON request secrets: {wire_reqs:?}"
    );
    assert!(
        wire_reqs
            .iter()
            .any(|r| r.target.starts_with("/api-form") && r.body.contains("REQ_FORM_KEY_802")),
        "stub did not receive real form request secrets: {wire_reqs:?}"
    );
}

/// G2 (#51): 応答側の JSON body・form body・HTTP ヘッダ行・200 文字途中切断 JSON・
/// サーバエコーヘッダ・`Content-Type` パラメータ（`E3002`）が `W4004` / `E4004` / `E3002`
/// （リトライ成功時を含む）で一切漏れず、通常フィールドが保持されることを検証する。
#[test]
fn g2_response_headers_json_body_and_form_body_leaks_redacted_in_errors_and_warnings() {
    let flaky_counter = Arc::new(AtomicUsize::new(0));
    let flaky_ref = Arc::clone(&flaky_counter);

    let stub = LoopbackStub::start(move |req| {
        let host = req
            .headers
            .get("host")
            .cloned()
            .unwrap_or_else(|| "127.0.0.1".to_string());
        match req.target.as_str() {
            t if t.starts_with("/unmatched-json") => StubResponse::json(
                json!({
                    "error": "unauthorized",
                    "api_key": "RESP_JSON_APIKEY_111",
                    "access_token": "RESP_JSON_TOKEN_222",
                    "x_auth_token": "RESP_JSON_XAUTH_333",
                    "password": "RESP_JSON_PASS_444",
                    "headers": {
                        "Authorization": "Bearer RESP_HDR_BEARER_555",
                        "Cookie": "sid=RESP_HDR_COOKIE_666",
                        "X-Normal": "KEEP_NESTED_HDR"
                    },
                    "normal_json": "KEEP_RESP_JSON"
                })
                .to_string(),
            ),
            t if t.starts_with("/flaky-json") => {
                let n = flaky_ref.fetch_add(1, Ordering::SeqCst);
                if n == 0 {
                    StubResponse::json(
                        json!({
                            "status": "warming_up",
                            "api_key": "FLAKY_RESP_APIKEY_701",
                            "password": "FLAKY_RESP_PASS_702",
                            "normal_flaky": "KEEP_FLAKY_MARKER"
                        })
                        .to_string(),
                    )
                } else {
                    StubResponse::json(
                        json!({
                            "items": [{
                                "t": "Recovered Hit",
                                "u": format!("http://recuser:recpass703@{host}/doc?normal=keeprec"),
                                "s": "Recovered snippet"
                            }]
                        })
                        .to_string(),
                    )
                }
            }
            t if t.starts_with("/trunc-json") => StubResponse::json(
                json!({
                    "aaa_padding": "P".repeat(165),
                    "api_key": "MID_TRUNC_SECRET_888888",
                    "zzz_tail": "KEEP_TAIL"
                })
                .to_string(),
            ),
            t if t.starts_with("/unmatched-form") => StubResponse {
                status: 200,
                headers: vec![(
                    "Content-Type".into(),
                    "application/x-www-form-urlencoded".into(),
                )],
                body: "client_id=KEEP_CLIENT_ID&access_token=RESP_FORM_TOK_911&password=RESP_FORM_PASS_912&normal=KEEP_RESP_FORM".into(),
            },
            t if t.starts_with("/unmatched-headers") => StubResponse {
                status: 200,
                headers: vec![("Content-Type".into(), "text/plain".into())],
                body: "Authorization: Bearer RESP_RAW_BEARER_921\r\nSet-Cookie: sid=RESP_RAW_COOKIE_922\r\nX-Api-Key: RESP_RAW_APIKEY_923\r\nX-Normal: KEEP_RAW_HDR\r\n".into(),
            },
            t if t.starts_with("/echo-req") => StubResponse::json(
                json!({
                    "received": req.headers,
                    "normal_echo": "KEEP_ECHO_VAL"
                })
                .to_string(),
            ),
            t if t.starts_with("/pdf-token") => StubResponse {
                status: 200,
                headers: vec![(
                    "Content-Type".into(),
                    "application/pdf; token=CT_RESP_SECRET_930; charset=utf-8".into(),
                )],
                body: "%PDF-1.4".into(),
            },
            _ => StubResponse {
                status: 404,
                headers: vec![],
                body: "not found".into(),
            },
        }
    });

    let env = EnvSetup::new("unmatched-json");
    for (id, path, kind, extra_headers) in [
        ("unmatched-json", "/unmatched-json", "json", json!({})),
        ("flaky-json", "/flaky-json", "json", json!({})),
        ("trunc-json", "/trunc-json", "json", json!({})),
        ("unmatched-form", "/unmatched-form", "html", json!({})),
        ("unmatched-headers", "/unmatched-headers", "html", json!({})),
        (
            "echo-req",
            "/echo-req",
            "json",
            json!({
                "Authorization": "Bearer ECHO_PLAIN_BEARER_940",
                "X-Custom-Plain": "KEEP_CUSTOM_PLAIN"
            }),
        ),
    ] {
        let items = if kind == "json" {
            json!({ "path": "items" })
        } else {
            json!({ "selector": "li" })
        };
        let fields = if kind == "json" {
            json!({ "title": { "path": "t" }, "url": { "path": "u" } })
        } else {
            json!({ "title": { "selector": "a" }, "url": { "selector": "a", "attr": "href" } })
        };
        env.write_engine(
            id,
            json!({
                "schemaVersion": 1,
                "id": id,
                "network": "self-hosted",
                "request": {
                    "method": "GET",
                    "url": format!("http://127.0.0.1:{}{path}", stub.port),
                    "headers": extra_headers
                },
                "response": {
                    "kind": kind,
                    "items": items,
                    "fields": fields
                },
                "limits": { "maxResults": 10, "maxPageSize": 10 }
            }),
        );
    }

    let forbidden = [
        "RESP_JSON_APIKEY_111",
        "RESP_JSON_TOKEN_222",
        "RESP_JSON_XAUTH_333",
        "RESP_JSON_PASS_444",
        "RESP_HDR_BEARER_555",
        "RESP_HDR_COOKIE_666",
        "FLAKY_RESP_APIKEY_701",
        "FLAKY_RESP_PASS_702",
        "recuser",
        "recpass703",
        "MID_TRUNC_SECRET_888888",
        "MID_TRUNC",
        "RESP_FORM_TOK_911",
        "RESP_FORM_PASS_912",
        "RESP_RAW_BEARER_921",
        "RESP_RAW_COOKIE_922",
        "RESP_RAW_APIKEY_923",
        "CT_RESP_SECRET_930",
        "ECHO_PLAIN_BEARER_940",
    ];

    // 1. Unmatched JSON -> rc=4, secrets masked, normal markers preserved
    let r_json = env.run(
        &[
            "--json",
            "--network",
            "self-hosted",
            "search",
            "q",
            "--engine",
            "unmatched-json",
        ],
        &[],
    );
    assert_eq!(r_json.status.code(), Some(4));
    let j_out = String::from_utf8_lossy(&r_json.stdout);
    let j_err = String::from_utf8_lossy(&r_json.stderr);
    assert_no_secrets(&j_out, &forbidden, "unmatched-json stdout");
    assert_no_secrets(&j_err, &forbidden, "unmatched-json stderr");
    assert!(j_out.contains("KEEP_NESTED_HDR") && j_out.contains("KEEP_RESP_JSON"));

    // 2. Flaky JSON -> attempt 1 emits W4004, attempt 2 succeeds (rc=0)!
    let r_flaky = env.run(
        &[
            "--json",
            "--network",
            "self-hosted",
            "search",
            "q",
            "--engine",
            "flaky-json",
        ],
        &[],
    );
    assert_eq!(r_flaky.status.code(), Some(0));
    let f_out = String::from_utf8_lossy(&r_flaky.stdout);
    let f_err = String::from_utf8_lossy(&r_flaky.stderr);
    assert_no_secrets(&f_out, &forbidden, "flaky-json stdout");
    assert_no_secrets(&f_err, &forbidden, "flaky-json stderr");
    assert!(f_out.contains("W4004") && f_out.contains("KEEP_FLAKY_MARKER"));

    // 3. >200-char mid-JSON truncation -> rc=4, no partial secret leaked at 200-char cut
    let r_trunc = env.run(
        &[
            "--json",
            "--network",
            "self-hosted",
            "search",
            "q",
            "--engine",
            "trunc-json",
        ],
        &[],
    );
    assert_eq!(r_trunc.status.code(), Some(4));
    assert_no_secrets(
        &String::from_utf8_lossy(&r_trunc.stdout),
        &forbidden,
        "trunc-json stdout",
    );
    assert_no_secrets(
        &String::from_utf8_lossy(&r_trunc.stderr),
        &forbidden,
        "trunc-json stderr",
    );

    // 4. Unmatched form body -> rc=4, form secrets masked, KEEP_CLIENT_ID & KEEP_RESP_FORM preserved
    let r_form = env.run(
        &[
            "--json",
            "--network",
            "self-hosted",
            "search",
            "q",
            "--engine",
            "unmatched-form",
        ],
        &[],
    );
    assert_eq!(r_form.status.code(), Some(4));
    let form_out = String::from_utf8_lossy(&r_form.stdout);
    assert_no_secrets(&form_out, &forbidden, "unmatched-form stdout");
    assert_no_secrets(
        &String::from_utf8_lossy(&r_form.stderr),
        &forbidden,
        "unmatched-form stderr",
    );
    assert!(form_out.contains("KEEP_CLIENT_ID") && form_out.contains("KEEP_RESP_FORM"));

    // 5. Unmatched raw HTTP header lines & echoed request headers -> rc=4, KEEP_RAW_HDR & KEEP_CUSTOM_PLAIN preserved
    let r_hdr = env.run(
        &[
            "--json",
            "--network",
            "self-hosted",
            "search",
            "q",
            "--engine",
            "unmatched-headers",
        ],
        &[],
    );
    assert_eq!(r_hdr.status.code(), Some(4));
    let hdr_out = String::from_utf8_lossy(&r_hdr.stdout);
    assert_no_secrets(&hdr_out, &forbidden, "unmatched-headers stdout");
    assert!(hdr_out.contains("KEEP_RAW_HDR"));

    let r_echo = env.run(
        &[
            "--json",
            "--network",
            "self-hosted",
            "search",
            "q",
            "--engine",
            "echo-req",
        ],
        &[],
    );
    assert_eq!(r_echo.status.code(), Some(4));
    let echo_out = String::from_utf8_lossy(&r_echo.stdout);
    assert_no_secrets(&echo_out, &forbidden, "echo-req stdout");
    assert!(echo_out.contains("KEEP_CUSTOM_PLAIN") && echo_out.contains("KEEP_ECHO_VAL"));

    // 6. E3002 Content-Type with sensitive header parameter
    let pdf_url = format!("http://127.0.0.1:{}/pdf-token", stub.port);
    let r_pdf = env.run(
        &["--json", "--network", "self-hosted", "outline", &pdf_url],
        &[],
    );
    assert_eq!(r_pdf.status.code(), Some(3));
    let pdf_out = String::from_utf8_lossy(&r_pdf.stdout);
    assert_no_secrets(&pdf_out, &forbidden, "pdf-token stdout");
    assert_no_secrets(
        &String::from_utf8_lossy(&r_pdf.stderr),
        &forbidden,
        "pdf-token stderr",
    );
    assert!(pdf_out.contains("application/pdf; token=***; charset=utf-8"));
}
