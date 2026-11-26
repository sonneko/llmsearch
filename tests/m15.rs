//! M15: JSON 警告の集約と秘匿情報の出力境界を実プロセスで検証する。
//! 送信先は OS 割り当てポートの `127.0.0.1` ローカル HTTP スタブのみ。

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::path::PathBuf;
use std::process::{Command, Output};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tempfile::TempDir;

const ENV_NAME: &str = "M15_ENGINE_SECRET";
const ENV_SECRET: &str = "m15-env-secret-very-long";
const STATIC_AUTH_SECRET: &str = "m15-static-auth-secret";
const STATIC_QUERY_SECRET: &str = "m15-fixed-query-secret";
const BODY_SECRET: &str = "m15-body-password";
const EXTERNAL_TOKEN_A: &str = "provider-token-a";
const EXTERNAL_TOKEN_B: &str = "provider-token-b";

struct Fixture {
    _temp: TempDir,
    state: PathBuf,
    engines: PathBuf,
    config: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path().join("state");
        let engines = temp.path().join("engines");
        let config = temp.path().join("settings.json");
        std::fs::create_dir_all(&state).unwrap();
        std::fs::create_dir_all(&engines).unwrap();
        std::fs::write(&config, json!({"schemaVersion": 2}).to_string()).unwrap();
        Self {
            _temp: temp,
            state,
            engines,
            config,
        }
    }

    fn write_engine(&self, id: &str, definition: &Value) {
        std::fs::write(
            self.engines.join(format!("{id}.json")),
            serde_json::to_vec_pretty(definition).unwrap(),
        )
        .unwrap();
    }

    fn write_settings(&self, settings: &Value) {
        std::fs::write(&self.config, serde_json::to_vec_pretty(settings).unwrap()).unwrap();
    }

    fn command(&self) -> Command {
        self.command_with_mode(true, true)
    }

    fn command_text(&self) -> Command {
        self.command_with_mode(false, true)
    }

    fn command_with_quiet(&self, quiet: bool) -> Command {
        self.command_with_mode(true, quiet)
    }

    fn command_with_mode(&self, json: bool, quiet: bool) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_llmsearch"));
        command.current_dir(env!("CARGO_MANIFEST_DIR"));
        if json {
            command.arg("--json");
        }
        if quiet {
            command.arg("--quiet");
        }
        command
            .args(["--no-cache", "--state-dir"])
            .arg(&self.state)
            .args(["--engines-dir"])
            .arg(&self.engines)
            .args(["--config"])
            .arg(&self.config)
            .args(["--network", "self-hosted"])
            .env(ENV_NAME, ENV_SECRET)
            .env("NO_PROXY", "127.0.0.1,localhost")
            .env("no_proxy", "127.0.0.1,localhost")
            .env_remove("LLMSEARCH_STATE_DIR")
            .env_remove("LLMSEARCH_CONFIG")
            .env_remove("LLMSEARCH_ENGINES_DIR")
            .env_remove("LLMSEARCH_ENGINE")
            .env_remove("LLMSEARCH_NETWORK")
            .env_remove("LLMSEARCH_FILTERS")
            .env_remove("LLMSEARCH_QUOTA")
            .env_remove("HTTP_PROXY")
            .env_remove("HTTPS_PROXY")
            .env_remove("ALL_PROXY")
            .env_remove("http_proxy")
            .env_remove("https_proxy")
            .env_remove("all_proxy");
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        let mut command = self.command();
        command.args(args);
        command.output().unwrap()
    }

    fn run_nonquiet(&self, args: &[&str]) -> Output {
        let mut command = self.command_with_quiet(false);
        command.args(args);
        command.output().unwrap()
    }
}

fn search_engine(id: &str, path: &str, address: SocketAddr) -> Value {
    json!({
        "schemaVersion": 1,
        "id": id,
        "name": format!("M15 {id}"),
        "network": "self-hosted",
        "request": {
            "method": "GET",
            "url": format!("http://{address}/{path}"),
            "query": {
                "q": "{{query}}",
                "token": {"value_env": ENV_NAME}
            },
            "headers": {
                "Accept": "text/html",
                "Authorization": {"value": format!("Bearer {STATIC_AUTH_SECRET}")},
                "X-Session": {"value_env": ENV_NAME}
            }
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
        "limits": {"maxResults": 20, "maxPageSize": 10}
    })
}

fn dry_run_engine() -> Value {
    json!({
        "schemaVersion": 1,
        "id": "m15-dry-run",
        "name": "M15 dry-run fixture",
        "network": "self-hosted",
        "request": {
            "method": "POST",
            "url": "https://dry-run.example/search",
            "query": {
                "q": "{{query}}",
                "tenant": {"value": STATIC_QUERY_SECRET},
                "token": {"value_env": ENV_NAME}
            },
            "headers": {
                "Accept": "application/json",
                "Authorization": {"value": format!("Bearer {STATIC_AUTH_SECRET}")},
                "X-Session": {"value_env": ENV_NAME},
                "X-Trace": {"value_env": ENV_NAME}
            },
            "body": format!("{{\"password\":\"{BODY_SECRET}\",\"note\":\"PUBLIC_MARKER\"}}"),
            "contentType": "application/json"
        },
        "response": {
            "kind": "html",
            "items": {"selector": "li"},
            "fields": {
                "title": {"selector": "a"},
                "url": {"selector": "a", "attr": "href"}
            }
        },
        "limits": {"maxResults": 10, "maxPageSize": 10}
    })
}

fn events(output: &Output) -> Vec<Value> {
    assert!(
        output.status.success(),
        "exit status: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    parse_events(&output.stdout)
}

fn parse_events(stdout: &[u8]) -> Vec<Value> {
    String::from_utf8(stdout.to_vec())
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).expect("stdout must be JSON Lines"))
        .collect()
}

fn assert_json_output(output: &Output, expected_event: &str) -> Vec<Value> {
    let parsed = events(output);
    assert!(
        parsed.iter().any(|item| item["event"] == expected_event),
        "missing {expected_event:?} event: {parsed:?}"
    );
    assert_eq!(parsed.last().unwrap()["event"], "done", "{parsed:?}");
    assert_eq!(
        parsed.iter().filter(|item| item["event"] == "done").count(),
        1,
        "each command must terminate exactly once: {parsed:?}"
    );
    for item in &parsed {
        assert_eq!(item["v"], 1, "{item}");
        assert!(item["session"].is_string(), "{item}");
        assert!(item["at"].as_str().unwrap().ends_with('Z'), "{item}");
    }
    parsed
}

fn event<'a>(events: &'a [Value], name: &str) -> &'a Value {
    events
        .iter()
        .find(|value| value["event"] == name)
        .unwrap_or_else(|| panic!("event {name:?} not found: {events:?}"))
}

fn start_local_server(expected_requests: usize) -> (SocketAddr, JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let worker = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut requests = Vec::with_capacity(expected_requests);
        while requests.len() < expected_requests {
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(connection) => break connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            Instant::now() < deadline,
                            "timed out waiting for local requests"
                        );
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("local HTTP accept failed: {error}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0u8; 2048];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                match stream.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => request.extend_from_slice(&buffer[..count]),
                    Err(error) => panic!("failed to read local request: {error}"),
                }
            }
            let request = String::from_utf8_lossy(&request).to_string();
            let request_line = request.lines().next().unwrap_or_default();
            let body = if request_line.contains("/search-a") {
                format!(
                    "<ul><li><a href=\"https://result-a.example/article?token={EXTERNAL_TOKEN_A}\">Result A</a></li></ul>"
                )
            } else if request_line.contains("/search-b") {
                format!(
                    "<ul><li><a href=\"https://result-b.example/article?token={EXTERNAL_TOKEN_B}\">Result B</a></li></ul>"
                )
            } else {
                "<html><head><title>M15 local document</title></head><body><main><h1>Local document</h1><p>Local content for a deterministic test.</p><h2>Details</h2><p>More local content.</p></main></body></html>".to_string()
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
            requests.push(request);
        }
        requests
    });
    (address, worker)
}

fn start_target_server(expected_requests: usize) -> (SocketAddr, JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let worker = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut requests = Vec::with_capacity(expected_requests);
        while requests.len() < expected_requests {
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(connection) => break connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            Instant::now() < deadline,
                            "timed out waiting for local requests"
                        );
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("local HTTP accept failed: {error}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0u8; 2048];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                match stream.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => request.extend_from_slice(&buffer[..count]),
                    Err(error) => panic!("failed to read local request: {error}"),
                }
            }
            let request = String::from_utf8_lossy(&request).to_string();
            let request_line = request.lines().next().unwrap_or_default();
            let body = if request_line.contains("/search") {
                let (path, token, title) = if request_line.contains("first") {
                    ("document-first", EXTERNAL_TOKEN_A, "First result")
                } else {
                    ("document-second", EXTERNAL_TOKEN_B, "Second result")
                };
                format!(
                    r#"<ul><li><a href="http://{address}/{path}?token={token}">{title}</a></li></ul>"#
                )
            } else if request_line.contains("/document-second") {
                "<html><head><title>Second document</title></head><body><main><h1>Second document</h1><p>Second local content.</p><h2>Details</h2><p>Updated result content.</p></main></body></html>".to_string()
            } else {
                "<html><head><title>First document</title></head><body><main><h1>First document</h1><p>First local content.</p><h2>Details</h2><p>Original result content.</p></main></body></html>".to_string()
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
            requests.push(request);
        }
        requests
    });
    (address, worker)
}

fn start_status_server(
    status: u16,
    body: String,
    expected_requests: usize,
) -> (SocketAddr, JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        let mut requests = Vec::with_capacity(expected_requests);
        for _ in 0..expected_requests {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0u8; 2048];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                match stream.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => request.extend_from_slice(&buffer[..count]),
                    Err(error) => panic!("failed to read local request: {error}"),
                }
            }
            let response = format!(
                "HTTP/1.1 {status} Test Response\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
            requests.push(String::from_utf8_lossy(&request).to_string());
        }
        requests
    });
    (address, worker)
}

#[test]
fn process_redacts_dry_run_search_output_and_fetch_history() {
    let fixture = Fixture::new();
    let (address, server) = start_local_server(4);
    fixture.write_engine("m15-a", &search_engine("m15-a", "search-a", address));
    fixture.write_engine("m15-b", &search_engine("m15-b", "search-b", address));
    fixture.write_engine("m15-dry-run", &dry_run_engine());
    let mut settings = json!({
        "schemaVersion": 2,
        "search": {
            "merge": true,
            "engines": ["m15-a", "m15-b"],
            "limit": 1,
            "maxResults": 10
        }
    });
    settings
        .as_object_mut()
        .unwrap()
        .insert(ENV_SECRET.to_string(), json!("kept"));
    fixture.write_settings(&settings);

    let dry_run = fixture.run(&[
        "engines",
        "test",
        "m15-dry-run",
        "--query",
        "probe",
        "--dry-run",
    ]);
    let dry_run_events = events(&dry_run);
    let request = &event(&dry_run_events, "diagnostics")["request"];
    let shown_url = request["url"].as_str().unwrap();
    assert!(shown_url.contains("tenant=***"), "{shown_url}");
    assert!(shown_url.contains("token=***"), "{shown_url}");
    let headers = request["headers"].as_array().unwrap();
    let header_value = |name: &str| {
        headers
            .iter()
            .find(|pair| pair[0] == name)
            .unwrap_or_else(|| panic!("missing header {name}: {headers:?}"))[1]
            .as_str()
            .unwrap()
    };
    assert_eq!(header_value("Authorization"), "***");
    assert_eq!(header_value("X-Session"), "***");
    assert_eq!(header_value("X-Trace"), "***");
    let body: Value = serde_json::from_str(request["body"].as_str().unwrap()).unwrap();
    assert_eq!(body["password"], "***");
    assert_eq!(body["note"], "PUBLIC_MARKER");
    let dry_run_text = String::from_utf8_lossy(&dry_run.stdout);
    for secret in [
        ENV_SECRET,
        STATIC_AUTH_SECRET,
        STATIC_QUERY_SECRET,
        BODY_SECRET,
    ] {
        assert!(!dry_run_text.contains(secret), "dry-run leaked {secret}");
    }

    let show = fixture.run(&["engines", "show", "m15-dry-run", "--raw"]);
    let show_events = events(&show);
    let show_text = serde_json::to_string(&show_events).unwrap();
    for secret in [
        ENV_SECRET,
        STATIC_AUTH_SECRET,
        STATIC_QUERY_SECRET,
        BODY_SECRET,
    ] {
        assert!(!show_text.contains(secret), "engines show leaked {secret}");
    }
    assert!(
        show_text.contains(ENV_NAME),
        "environment variable name stays visible"
    );

    let mut summary_command = fixture.command_text();
    summary_command.args(["engines", "show", "m15-dry-run"]);
    let summary = summary_command.output().unwrap();
    assert!(summary.status.success());
    let summary_text = String::from_utf8(summary.stdout).unwrap();
    for secret in [
        ENV_SECRET,
        STATIC_AUTH_SECRET,
        STATIC_QUERY_SECRET,
        BODY_SECRET,
    ] {
        assert!(
            !summary_text.contains(secret),
            "engines summary leaked {secret}"
        );
    }

    let mut dry_run_text_command = fixture.command_text();
    dry_run_text_command.args([
        "engines",
        "test",
        "m15-dry-run",
        "--query",
        "probe",
        "--dry-run",
    ]);
    let dry_run_text_output = dry_run_text_command.output().unwrap();
    assert!(dry_run_text_output.status.success());
    let dry_run_text = String::from_utf8(dry_run_text_output.stdout).unwrap();
    for secret in [
        ENV_SECRET,
        STATIC_AUTH_SECRET,
        STATIC_QUERY_SECRET,
        BODY_SECRET,
    ] {
        assert!(
            !dry_run_text.contains(secret),
            "text dry-run leaked {secret}"
        );
    }
    assert!(dry_run_text.contains("PUBLIC_MARKER"));

    let list = fixture.run(&["engines", "list"]);
    let list_events = events(&list);
    let engine_rows = event(&list_events, "engines")["engines"]
        .as_array()
        .unwrap();
    let m15_a = engine_rows.iter().find(|row| row["id"] == "m15-a").unwrap();
    assert_eq!(m15_a["apiKey"], "configured");

    let search = fixture.run(&["search", ENV_SECRET]);
    let search_events = events(&search);
    let search_event = event(&search_events, "search");
    assert!(!search_event["results"].as_array().unwrap().is_empty());
    let result_urls: Vec<&str> = search_event["results"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|hit| hit["url"].as_str())
        .collect();
    assert!(
        result_urls
            .iter()
            .all(|url| !url.contains("provider-token-")),
        "display URLs must redact sensitive query parameters: {result_urls:?}"
    );
    let warnings = search_event["warnings"].as_array().unwrap();
    assert!(
        warnings.iter().any(|warning| warning["code"] == "W1002"),
        "settings warnings must join an event that already has warnings[]: {warnings:?}"
    );
    assert!(
        warnings.iter().any(|warning| warning["code"] == "W2001"),
        "Reporter warnings must join the command event: {warnings:?}"
    );
    assert!(
        warnings.iter().any(|warning| warning["message"]
            .as_str()
            .unwrap()
            .contains("unknown key ***")),
        "warning messages are redacted at the output boundary: {warnings:?}"
    );
    assert!(!String::from_utf8_lossy(&search.stdout).contains(ENV_SECRET));
    assert!(!String::from_utf8_lossy(&search.stderr).contains("W2001"));
    assert!(!String::from_utf8_lossy(&search.stderr).contains("W1002"));

    let document_url = format!("http://{address}/document/{ENV_SECRET}?x=ok");
    let mut outline_command = fixture.command();
    outline_command.arg("outline").arg(&document_url);
    let outline = outline_command.output().unwrap();
    let outline_events = events(&outline);
    let document = event(&outline_events, "document");
    assert_eq!(
        document["url"],
        format!("http://{address}/document/***?x=ok")
    );
    assert!(!String::from_utf8_lossy(&outline.stdout).contains(ENV_SECRET));
    assert!(!String::from_utf8_lossy(&outline.stderr).contains(ENV_SECRET));

    let requests = server.join().unwrap();
    assert_eq!(
        requests.len(),
        4,
        "unexpected local HTTP traffic: {requests:?}"
    );
    assert!(
        requests.iter().any(|request| request.contains(ENV_SECRET)),
        "the process must resolve the engine secret for local requests"
    );

    let history_path = fixture.state.join("history/history.jsonl");
    let history = std::fs::read_to_string(&history_path).unwrap();
    assert!(
        !history.contains(ENV_SECRET),
        "fetch history leaked an environment secret: {history}"
    );
    assert!(history.contains("document/***?x=ok"), "{history}");
    let fetch_entry: Value = history
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .find(|entry: &Value| entry["type"] == "fetch")
        .expect("the local fetch must be recorded");
    assert_eq!(
        fetch_entry["payload"]["url"],
        format!("http://{address}/document/***?x=ok")
    );
    let search_entry: Value = history
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .find(|entry: &Value| entry["type"] == "search")
        .expect("search must be recorded");
    assert_eq!(search_entry["payload"]["query"], "***");
}

#[test]
fn process_refine_and_resolve_result_and_document_ids_in_persisted_session() {
    let fixture = Fixture::new();
    let (address, server) = start_target_server(8);
    fixture.write_engine(
        "m15-target",
        &search_engine("m15-target", "search", address),
    );
    fixture.write_settings(&json!({
        "schemaVersion": 2,
        "search": {
            "merge": true,
            "engines": ["m15-target"],
            "limit": 1,
            "maxResults": 10
        }
    }));

    let no_session_fixture = Fixture::new();
    let no_session = no_session_fixture.run(&["outline", "R1"]);
    assert!(!no_session.status.success());
    let no_session_events = parse_events(&no_session.stdout);
    let no_session_error = event(&no_session_events, "error");
    assert_eq!(no_session_error["code"], "E5001");
    assert!(!no_session_error["hint"].as_str().unwrap().is_empty());

    let no_session_next = no_session_fixture.run(&["next"]);
    assert!(!no_session_next.status.success());
    let next_events = parse_events(&no_session_next.stdout);
    let next_error = event(&next_events, "error");
    assert_eq!(next_error["code"], "E5001");
    assert!(next_error["hint"].as_str().unwrap().contains("`search`"));
    let session_files = std::fs::read_dir(no_session_fixture.state.join("sessions"))
        .map(|entries| entries.filter_map(Result::ok).collect::<Vec<_>>())
        .unwrap_or_default();
    assert!(
        session_files.is_empty(),
        "failed identifier resolution must not persist a session: {session_files:?}"
    );

    let first_search = fixture.run(&["search", "first query"]);
    let first_events = events(&first_search);
    let first = event(&first_events, "search");
    assert_eq!(first["results"][0]["id"], 1);

    let missing_result = fixture.run(&["outline", "R99"]);
    assert!(!missing_result.status.success());
    assert_eq!(
        event(&parse_events(&missing_result.stdout), "error")["code"],
        "E1006"
    );
    assert!(
        first["results"][0]["url"]
            .as_str()
            .unwrap()
            .contains("/document-first?token=***")
    );

    let outline_result = fixture.run(&["outline", "R1"]);
    let outline_events = events(&outline_result);
    let first_document = event(&outline_events, "document");
    assert_eq!(first_document["documentId"], 1);
    assert!(
        first_document["url"]
            .as_str()
            .unwrap()
            .contains("/document-first?token=***")
    );

    let missing_document = fixture.run(&["read", "D99"]);
    assert!(!missing_document.status.success());
    assert_eq!(
        event(&parse_events(&missing_document.stdout), "error")["code"],
        "E1006"
    );

    let read_document = fixture.run(&["read", "D1", "--section", "S0"]);
    let read_events = events(&read_document);
    let content = event(&read_events, "content");
    assert_eq!(content["documentId"], 1);
    assert!(
        content["text"]
            .as_str()
            .unwrap()
            .contains("First local content")
    );

    let refined = fixture.run(&["refine", "second query"]);
    let refined_events = events(&refined);
    let refine = event(&refined_events, "refine");
    assert_eq!(refine["previousQuery"], "first query");
    assert_eq!(refine["query"], "second query");
    assert_eq!(refine["results"].as_array().unwrap().len(), 1);
    assert_eq!(refine["results"][0]["title"], "Second result");
    assert!(
        refine["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning["code"] == "W4001")
    );

    let refined_result = fixture.run(&["outline", "R1"]);
    let refined_events = events(&refined_result);
    let refined_document = event(&refined_events, "document");
    assert!(
        refined_document["url"]
            .as_str()
            .unwrap()
            .contains("/document-second?token=***")
    );

    let history = assert_json_output(&fixture.run(&["history", "list"]), "history");
    let refine_entry = event(&history, "history")["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["type"] == "refine")
        .expect("refine must be recorded across processes");
    assert_eq!(refine_entry["payload"]["previous"], "first query");
    assert_eq!(refine_entry["payload"]["query"], "second query");

    let requests = server.join().unwrap();
    assert_eq!(
        requests.len(),
        8,
        "unexpected local HTTP traffic: {requests:?}"
    );
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.contains("GET /search?"))
            .count(),
        2,
        "only the initial search and refine may query the provider: {requests:?}"
    );
    assert!(requests.iter().all(|request| request.starts_with("GET /")));
}

#[test]
fn process_http_error_does_not_echo_request_secrets() {
    let fixture = Fixture::new();
    let (address, server) = start_status_server(503, format!("body mentions {ENV_SECRET}"), 3);
    fixture.write_engine(
        "m15-http-error",
        &search_engine("m15-http-error", "failure", address),
    );
    fixture.write_settings(&json!({
        "schemaVersion": 2,
        "search": {
            "merge": true,
            "engines": ["m15-http-error"],
            "limit": 1,
            "maxResults": 10
        }
    }));

    let output = fixture.run(&["search", ENV_SECRET]);
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 3, "{requests:?}");
    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed = parse_events(stdout.as_bytes());
    let error = event(&parsed, "error");
    assert_eq!(error["code"], "E2003", "requests: {requests:?}\n{stdout}");
    assert_eq!(error["details"], "HTTP 503");
    assert!(!stdout.contains(ENV_SECRET));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(!stderr.contains(ENV_SECRET));
    assert!(!stderr.contains(STATIC_AUTH_SECRET));

    assert!(
        requests.iter().all(|request| request.contains(ENV_SECRET)),
        "{requests:?}"
    );
    assert!(
        requests
            .iter()
            .all(|request| request.contains(STATIC_AUTH_SECRET)),
        "{requests:?}"
    );
}

#[test]
fn config_warnings_precede_events_without_a_warnings_field_even_in_quiet_json() {
    let fixture = Fixture::new();
    fixture.write_settings(&json!({
        "schemaVersion": 2,
        "m15UnknownSetting": "kept"
    }));
    let output = fixture.run(&["doctor", "--no-network"]);
    let parsed = events(&output);
    assert_eq!(parsed.first().unwrap()["event"], "warnings", "{parsed:?}");
    assert_eq!(parsed.get(1).unwrap()["event"], "diagnostics", "{parsed:?}");
    assert!(
        parsed[0]["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning["code"] == "W1002")
    );
    assert!(!String::from_utf8_lossy(&output.stderr).contains("W1002"));
    for item in &parsed {
        assert_eq!(item["v"], 1);
        assert!(item["at"].as_str().unwrap().ends_with('Z'));
        assert!(item.get("session").is_some());
    }
}

#[test]
fn administrative_commands_emit_only_enveloped_json_lines_and_one_terminal_event() {
    let fixture = Fixture::new();
    let (address, server) = start_target_server(1);
    fixture.write_engine("m15-admin", &search_engine("m15-admin", "search", address));
    fixture.write_engine("m15-dry-run", &dry_run_engine());
    fixture.write_settings(&json!({
        "schemaVersion": 2,
        "search": {
            "merge": true,
            "engines": ["m15-admin"],
            "limit": 1,
            "maxResults": 10
        }
    }));

    let search = fixture.run(&["search", "first query"]);
    assert_json_output(&search, "search");
    let requests = server.join().unwrap();
    assert_eq!(
        requests.len(),
        1,
        "unexpected local HTTP traffic: {requests:?}"
    );

    let history = assert_json_output(&fixture.run(&["history"]), "history");
    let search_id = event(&history, "history")["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["type"] == "search")
        .and_then(|entry| entry["id"].as_str())
        .expect("search history entry")
        .to_string();
    let commands: &[(&[&str], &str)] = &[
        (&["history", "list"], "history"),
        (&["history", "show", search_id.as_str()], "history"),
        (&["history", "grep", "first"], "history"),
        (&["history", "export"], "history"),
        (&["history", "export", "--format", "json"], "history"),
        (&["history", "export", "--format", "jsonl"], "history"),
        (&["engines"], "engines"),
        (&["engines", "list"], "engines"),
        (&["engines", "show", "m15-admin"], "engine"),
        (&["engines", "show", "m15-admin", "--raw"], "engine"),
        (&["engines", "validate", "m15-admin"], "diagnostics"),
        (
            &[
                "engines",
                "test",
                "m15-dry-run",
                "--query",
                "probe",
                "--dry-run",
            ],
            "diagnostics",
        ),
        (&["network"], "network"),
        (&["network", "list"], "network"),
        (&["network", "show", "self-hosted"], "network"),
        (&["network", "validate", "self-hosted"], "diagnostics"),
        (&["filters"], "filters"),
        (&["filters", "list"], "filters"),
        (&["filters", "show", "default"], "filters"),
        (&["filters", "show", "default", "--raw"], "filters"),
        (&["filters", "validate", "default"], "diagnostics"),
        (&["quota"], "quota"),
        (&["quota", "show"], "quota"),
        (&["quota", "check"], "quota"),
        (&["config"], "config"),
        (&["config", "show"], "config"),
        (&["config", "path"], "config"),
        (&["config", "explain", "search.limit"], "config"),
        (&["config", "validate"], "diagnostics"),
        (&["doctor", "--no-network"], "diagnostics"),
    ];
    for (args, expected_event) in commands {
        let output = fixture.run(args);
        assert_json_output(&output, expected_event);
    }

    let reset = fixture.run(&["quota", "reset", "--scope", "day", "--yes"]);
    assert_json_output(&reset, "quota");
    let clear = fixture.run(&["history", "clear", "--yes"]);
    assert_json_output(&clear, "history");
    let init = fixture.run(&["config", "init"]);
    assert_json_output(&init, "config");
}

#[test]
fn startup_warnings_redact_static_engine_values_from_stdout_and_stderr() {
    let fixture = Fixture::new();
    fixture.write_engine("m15-dry-run", &dry_run_engine());
    let mut settings = json!({"schemaVersion": 2});
    settings
        .as_object_mut()
        .unwrap()
        .insert(STATIC_QUERY_SECRET.to_string(), json!("kept"));
    fixture.write_settings(&settings);

    let output = fixture.run_nonquiet(&["doctor", "--no-network"]);
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed = parse_events(stdout.as_bytes());
    assert_eq!(parsed[0]["event"], "warnings", "{parsed:?}");
    assert!(!stdout.contains(STATIC_QUERY_SECRET));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("W1002"), "{stderr}");
    assert!(
        !stderr.contains(STATIC_QUERY_SECRET),
        "startup warning leaked a static engine value: {stderr}"
    );
}

#[test]
fn error_event_and_stderr_redact_secret_values_inside_details_and_warnings() {
    let fixture = Fixture::new();
    let unused_address: SocketAddr = "127.0.0.1:0".parse().unwrap();
    fixture.write_engine(
        "m15-error",
        &search_engine("m15-error", "unused", unused_address),
    );
    let mut settings = json!({"schemaVersion": 2});
    settings
        .as_object_mut()
        .unwrap()
        .insert(ENV_SECRET.to_string(), json!("kept"));
    fixture.write_settings(&settings);

    let invalid_target = format!("http://{ENV_SECRET} invalid");
    let mut command = fixture.command();
    command.arg("outline").arg(invalid_target);
    let output = command.output().unwrap();
    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed = parse_events(stdout.as_bytes());
    let error = event(&parsed, "error");
    assert!(
        error["details"].as_str().unwrap().contains("http://***"),
        "{error}"
    );
    assert!(!stdout.contains(ENV_SECRET));
    assert!(error["message"].as_str().unwrap().is_ascii());
    assert!(error["hint"].as_str().unwrap().is_ascii());
    assert!(
        error["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning["code"] == "W1002"
                && !warning["message"].as_str().unwrap().contains(ENV_SECRET))
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains(ENV_SECRET));
    assert!(
        stderr.contains(error["message"].as_str().unwrap()),
        "{stderr}"
    );
    assert!(stderr.contains(error["hint"].as_str().unwrap()), "{stderr}");
}

#[test]
fn process_uses_custom_english_hint_consistently_in_json_and_text_errors() {
    let fixture = Fixture::new();
    let args = ["quota", "reset", "--scope", "bogus", "--yes"];

    let json_error = fixture.run(&args);
    assert!(!json_error.status.success());
    let parsed = parse_events(&json_error.stdout);
    let error = event(&parsed, "error");
    assert_eq!(error["code"], "E1004");
    let message = error["message"].as_str().unwrap();
    let hint = error["hint"].as_str().unwrap();
    assert!(message.is_ascii() && hint.is_ascii());
    for scope in ["session", "day", "window", "total", "all"] {
        assert!(hint.contains(&format!("--scope {scope}")), "{hint}");
    }
    let json_stderr = String::from_utf8_lossy(&json_error.stderr);
    assert!(json_stderr.contains(message), "{json_stderr}");
    assert!(json_stderr.contains(hint), "{json_stderr}");

    let mut text_command = fixture.command_text();
    text_command.args(args);
    let text_error = text_command.output().unwrap();
    assert!(!text_error.status.success());
    let text_stderr = String::from_utf8_lossy(&text_error.stderr);
    assert!(text_stderr.contains(message), "{text_stderr}");
    assert!(text_stderr.contains(hint), "{text_stderr}");
}

#[test]
fn process_redacts_legacy_history_payloads_at_text_output_boundaries() {
    let fixture = Fixture::new();
    let history_path = fixture.state.join("history/history.jsonl");
    std::fs::create_dir_all(history_path.parent().unwrap()).unwrap();
    let at = chrono::DateTime::parse_from_rfc3339("2026-11-19T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let entry = llmsearch::core::history::HistoryEntry::new(
        "H1",
        at,
        "",
        llmsearch::core::history::EventType::Search,
        json!({
            "query": "legacy query",
            "url": "https://example.test/path?token=legacy-url-secret&keep=public",
            "headers": [["Authorization", "legacy-auth-secret"]],
            "body": "{\"password\":\"legacy-body-secret\",\"note\":\"PUBLIC_MARKER\"}"
        }),
    );
    std::fs::write(
        &history_path,
        format!("{}\n", serde_json::to_string(&entry).unwrap()),
    )
    .unwrap();

    let mut show_command = fixture.command_text();
    show_command.args(["history", "show", "H1"]);
    let show = show_command.output().unwrap();
    assert!(
        show.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&show.stdout),
        String::from_utf8_lossy(&show.stderr)
    );
    let show_text = String::from_utf8(show.stdout).unwrap();
    for secret in [
        "legacy-url-secret",
        "legacy-auth-secret",
        "legacy-body-secret",
    ] {
        assert!(!show_text.contains(secret), "history show leaked {secret}");
    }
    assert!(show_text.contains("PUBLIC_MARKER"));

    let mut export_command = fixture.command_text();
    export_command.args(["history", "export", "--format", "jsonl"]);
    let export = export_command.output().unwrap();
    assert!(export.status.success());
    let export_text = String::from_utf8(export.stdout).unwrap();
    for secret in [
        "legacy-url-secret",
        "legacy-auth-secret",
        "legacy-body-secret",
    ] {
        assert!(
            !export_text.contains(secret),
            "history export leaked {secret}"
        );
    }
    let exported: Value = serde_json::from_str(export_text.lines().next().unwrap()).unwrap();
    assert_eq!(exported["payload"]["headers"][0][1], "***");
    assert_eq!(
        exported["payload"]["url"],
        "https://example.test/path?token=***&keep=public"
    );
}

#[test]
fn url_redaction_masks_userinfo_and_sensitive_path_segments() {
    let url =
        url::Url::parse("http://alice:password@localhost/document/token=path-secret?x=public")
            .unwrap();
    let displayed = llmsearch::core::url::redact(&url, &[]);
    assert_eq!(
        displayed,
        "http://***:***@localhost/document/token=***?x=public"
    );
}
