//! ルート証明書（CA）の回帰テスト。外部ネットワークは使わない。
//!
//! ローカルの HTTPS サーバーをプライベート CA の証明書で立て、その CA を
//! システムルートストア（`SSL_CERT_FILE`）または `tls.caFile` から読んだ
//! ときだけ検証が通ることを確認する。TLS を中継するプロキシ環境で
//! `tls.caFile` の手動指定なしに HTTPS が通ることを保証するための検証である。
mod support;

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use llmsearch::adapters::http::ReqwestClient;
use llmsearch::core::config::NetworkProfile;
use llmsearch::core::errors::ErrorCode;
use llmsearch::ports::http::HttpRequest;
use llmsearch::ports::{Env, HttpClient};
use serde_json::json;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/tls")
        .join(name)
}

/// テスト用の環境変数（`support::FakeEnv` と同等だが状態ディレクトリ不要）。
#[derive(Default)]
struct Vars {
    vars: std::collections::BTreeMap<String, String>,
}

impl Vars {
    fn with(mut self, key: &str, value: &str) -> Self {
        self.vars.insert(key.to_string(), value.to_string());
        self
    }
}

impl Env for Vars {
    fn var(&self, name: &str) -> Option<String> {
        self.vars.get(name).cloned()
    }
    fn var_names(&self) -> Vec<String> {
        self.vars.keys().cloned().collect()
    }
    fn is_stdin_tty(&self) -> bool {
        false
    }
    fn is_stdout_tty(&self) -> bool {
        false
    }
    fn terminal_width(&self) -> usize {
        100
    }
    fn home_dir(&self) -> Option<PathBuf> {
        None
    }
    fn state_home(&self) -> Option<PathBuf> {
        None
    }
    fn config_home(&self) -> Option<PathBuf> {
        None
    }
}

fn client(env: Vars) -> ReqwestClient {
    ReqwestClient::new(Arc::new(support::FixedClock::new("2026-11-14T00:00:00Z")))
        .with_env(Arc::new(env))
}

fn profile(value: serde_json::Value) -> NetworkProfile {
    let mut profile: NetworkProfile = serde_json::from_value(value).unwrap();
    profile.retry.enabled = false;
    profile.min_interval_ms = 0;
    profile.timeout_connect_ms = 3000;
    profile.timeout_total_ms = 5000;
    profile
}

/// ローカル HTTPS サーバー（起動時に割り当てられたポートを保持する）。
struct TlsServer {
    child: Option<Child>,
    port: String,
}

impl TlsServer {
    fn start() -> Option<Self> {
        let Ok(mut child) = Command::new("python3")
            .arg(fixture("server.py"))
            .arg(fixture("server-chain.pem"))
            .arg(fixture("server.key"))
            .stdout(Stdio::piped())
            .spawn()
        else {
            eprintln!("python3 が無いため TLS の実通信テストを飛ばす");
            return None;
        };
        let mut stdout = BufReader::new(child.stdout.take()?);
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut line = String::new();
        loop {
            line.clear();
            match stdout.read_line(&mut line) {
                Ok(0) => {
                    eprintln!("TLS サーバーが起動しなかった");
                    return None;
                }
                Ok(_) => {
                    if let Some(port) = line.trim().strip_prefix("port ") {
                        return Some(Self {
                            child: Some(child),
                            port: port.to_string(),
                        });
                    }
                }
                Err(e) => {
                    eprintln!("TLS サーバーと通信できない: {e}");
                    return None;
                }
            }
            if Instant::now() > deadline {
                eprintln!("TLS サーバーの起動がタイムアウトした");
                return None;
            }
        }
    }

    fn url(&self, path: &str) -> String {
        format!("https://127.0.0.1:{}{}", self.port, path)
    }
}

impl Drop for TlsServer {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[test]
fn ssl_cert_file_roots_are_trusted_without_ca_file() {
    let Some(server) = TlsServer::start() else {
        return;
    };
    let ca = fixture("ca.pem").display().to_string();
    let response = client(Vars::default().with("SSL_CERT_FILE", &ca))
        .send_blocking(
            &profile(json!({})),
            &HttpRequest::get(server.url("/doc").parse().unwrap()),
        )
        .expect("システムルートストアの CA で検証できること");
    assert_eq!(response.status, 200);
}

#[test]
fn unrelated_ca_is_not_trusted() {
    let Some(server) = TlsServer::start() else {
        return;
    };
    // 無関係な CA だけを信頼させても検証は失敗する（信頼の範囲が広がりすぎないこと）。
    let other = fixture("other-ca.pem").display().to_string();
    let outcome = client(Vars::default().with("SSL_CERT_FILE", &other)).send_blocking(
        &profile(json!({})),
        &HttpRequest::get(server.url("/doc").parse().unwrap()),
    );
    assert!(outcome.is_err(), "無関係な CA は信頼しない: {outcome:?}");
}

#[test]
fn explicit_ca_file_takes_priority_and_is_trusted() {
    let Some(server) = TlsServer::start() else {
        return;
    };
    let ca = fixture("ca.pem").display().to_string();
    let response = client(Vars::default())
        .send_blocking(
            &profile(json!({"tls": {"caFile": ca}})),
            &HttpRequest::get(server.url("/doc").parse().unwrap()),
        )
        .expect("tls.caFile の CA で検証できること");
    assert_eq!(response.status, 200);
}

#[test]
fn broken_ca_file_fails_closed_with_e2008() {
    let err = client(Vars::default())
        .send_blocking(
            &profile(json!({"tls": {"caFile": "/nonexistent/ca.pem"}})),
            &HttpRequest::get("https://127.0.0.1:1/doc".parse().unwrap()),
        )
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::E2008);
}

#[test]
fn root_certificates_are_loaded_once_and_reused() {
    let ca = fixture("ca.pem").display().to_string();
    let client = client(Vars::default().with("SSL_CERT_FILE", &ca));
    let first = client.root_certificates("").unwrap();
    assert!(!first.is_empty(), "システムルートストアが読めていない");
    let second = client.root_certificates("").unwrap();
    assert_eq!(first.len(), second.len());
}

#[test]
fn verify_disabled_skips_root_loading() {
    // verify=false のときはルート証明書を読まないため、壊れた caFile でも止まらない。
    client(Vars::default())
        .send_blocking(
            &profile(json!({"tls": {"verify": false, "caFile": "/nonexistent/ca.pem"}})),
            &HttpRequest::get("https://127.0.0.1:1/doc".parse().unwrap()),
        )
        .unwrap_err();
}

#[test]
fn system_bundle_is_readable_when_present() {
    // 実行環境が標準的な証明書束を持つ場合、そこからも読めることを確認する。
    // 持たない環境（一部のコンテナ）では何も読めなくてよい。
    let path = PathBuf::from("/etc/ssl/certs/ca-certificates.crt");
    if !path.is_file() {
        return;
    }
    let roots = llmsearch::adapters::tls::load(&Vars::default(), "").unwrap();
    assert!(!roots.is_empty(), "システムの証明書束が読めていない");
}
