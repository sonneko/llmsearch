//! テスト用の共通部品（偽 HTTP・固定時計・メモリ状態）。
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use llmsearch::core::config::NetworkProfile;
use llmsearch::core::errors::{AppError, Diagnostic, ErrorCode};
use llmsearch::ports::http::{HttpRequest, HttpResponse};
use llmsearch::ports::{Clock, Env, HttpClient, IdGen, Reporter};
use llmsearch::usecases::App;
use llmsearch::usecases::config::{Bootstrap, build};

/// 固定時計。
pub struct FixedClock {
    pub now: chrono::DateTime<chrono::Utc>,
    pub date: chrono::NaiveDate,
}

impl FixedClock {
    pub fn new(iso: &str) -> Self {
        let now = chrono::DateTime::parse_from_rfc3339(iso)
            .unwrap()
            .with_timezone(&chrono::Utc);
        Self {
            now,
            date: now.date_naive(),
        }
    }
}

impl Clock for FixedClock {
    fn now_utc(&self) -> chrono::DateTime<chrono::Utc> {
        self.now
    }

    fn today_local(&self) -> chrono::NaiveDate {
        self.date
    }

    fn monotonic_ns(&self) -> u64 {
        0
    }

    fn sleep(&self, _duration: std::time::Duration) {}
}

/// 決定的な識別子。
pub struct SeqIds;

impl IdGen for SeqIds {
    fn session_id(&self, now: chrono::DateTime<chrono::Utc>) -> String {
        let _ = now;
        "20261107-000000-test".to_string()
    }
}

/// 出力を溜めない通知。
pub struct SilentReporter {
    pub warnings: Mutex<Vec<Diagnostic>>,
}

impl SilentReporter {
    pub fn new() -> Self {
        Self {
            warnings: Mutex::new(Vec::new()),
        }
    }
}

impl Default for SilentReporter {
    fn default() -> Self {
        Self::new()
    }
}

impl Reporter for SilentReporter {
    fn info(&self, _message: &str) {}
    fn warn(&self, diagnostic: &Diagnostic) {
        if let Ok(mut w) = self.warnings.lock() {
            w.push(diagnostic.clone());
        }
    }
    fn verbose(&self, _message: &str) {}
}

/// テスト用の環境変数。
pub struct FakeEnv {
    pub vars: BTreeMap<String, String>,
    pub state_dir: PathBuf,
}

impl FakeEnv {
    pub fn new(state_dir: PathBuf) -> Self {
        Self {
            vars: BTreeMap::new(),
            state_dir,
        }
    }

    pub fn with(mut self, key: &str, value: &str) -> Self {
        self.vars.insert(key.to_string(), value.to_string());
        self
    }
}

impl Env for FakeEnv {
    fn var(&self, name: &str) -> Option<String> {
        match name {
            "LLMSEARCH_STATE_DIR" => Some(self.state_dir.display().to_string()),
            other => self.vars.get(other).cloned(),
        }
    }

    fn var_names(&self) -> Vec<String> {
        self.vars.keys().cloned().collect()
    }

    fn is_stdin_tty(&self) -> bool {
        true
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
        Some(self.state_dir.clone())
    }

    fn config_home(&self) -> Option<PathBuf> {
        None
    }
}

/// URL ごとの応答を返す偽 HTTP。
pub struct FakeHttp {
    pub routes: Mutex<BTreeMap<String, (u16, String, String)>>,
    pub requests: Mutex<Vec<HttpRequest>>,
}

impl FakeHttp {
    pub fn new() -> Self {
        Self {
            routes: Mutex::new(BTreeMap::new()),
            requests: Mutex::new(Vec::new()),
        }
    }

    /// URL の一部（`key`）で応答を登録する。
    pub fn route(&self, key: &str, status: u16, content_type: &str, body: &str) {
        self.routes.lock().unwrap().insert(
            key.to_string(),
            (status, content_type.to_string(), body.to_string()),
        );
    }

    pub fn request_count(&self) -> usize {
        self.requests.lock().map(|r| r.len()).unwrap_or(0)
    }
}

impl Default for FakeHttp {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpClient for FakeHttp {
    fn send_blocking(
        &self,
        _profile: &NetworkProfile,
        request: &HttpRequest,
    ) -> Result<HttpResponse, AppError> {
        self.requests.lock().unwrap().push(request.clone());
        let full = request.url.to_string();
        let routes = self.routes.lock().unwrap();
        let hit = routes
            .iter()
            .find(|(key, _)| full.contains(key.as_str()))
            .map(|(_, v)| v.clone());
        match hit {
            Some((status, content_type, body)) => Ok(HttpResponse {
                status,
                final_url: request.url.clone(),
                headers: vec![("content-type".to_string(), content_type)],
                body: body.into_bytes(),
                truncated: false,
                redirects: 0,
            }),
            None => Err(AppError::with_detail(
                ErrorCode::E2001,
                format!("テストで未登録の URL: {full}"),
            )),
        }
    }

    fn describe(&self, profile: &NetworkProfile) -> String {
        format!("fake:{}", profile.id)
    }
}

/// テスト用のアプリ一式。
pub struct Harness {
    pub app: App,
    /// 設定の読み込み結果（`config explain` の由来表示に使う）。
    pub resolved: llmsearch::usecases::config::Resolved,
    pub http: Arc<FakeHttp>,
    pub dir: tempfile::TempDir,
}

/// テスト用のアプリを組み立てる。
pub fn harness(extra_engines: Option<&str>) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");
    std::fs::create_dir_all(&state_dir).unwrap();

    let mut engines_dirs = Vec::new();
    if let Some(body) = extra_engines {
        let engine_dir = dir.path().join("engines");
        std::fs::create_dir_all(&engine_dir).unwrap();
        std::fs::write(engine_dir.join("test.json"), body).unwrap();
        engines_dirs.push(engine_dir);
    }

    let env = Arc::new(FakeEnv::new(state_dir));
    let clock = Arc::new(FixedClock::new("2026-11-07T05:00:00Z"));
    let reporter = Arc::new(SilentReporter::new());
    let bootstrap = Bootstrap {
        engines_dirs,
        engine: Some("duckduckgo".to_string()),
        quiet: true,
        ..Bootstrap::default()
    };
    let (app, resolved) =
        build(&bootstrap, clock, env, Arc::new(SeqIds), reporter).expect("アプリの組み立てに失敗");

    let http = Arc::new(FakeHttp::new());
    let mut app = app;
    app.set_http(http.clone());
    app.session_id.lock().unwrap().clear();
    // 既定エンジンは偽 HTTP の応答を使って検証するため duckduckgo を使う
    app.default_engine = "duckduckgo".to_string();
    Harness {
        app,
        resolved,
        http,
        dir,
    }
}

/// 検索結果 1 ページ分の HTML を組み立てる。
pub fn duckduckgo_html(items: &[(usize, &str, &str)]) -> String {
    let mut body = String::from("<html><body><div id=\"links\">");
    for (i, (n, title, url)) in items.iter().enumerate() {
        let _ = i;
        body.push_str(&format!(
            r#"<div class="result results_links"><div class="links_main">
                 <h2><a class="result__a" href="{url}">{title}</a></h2>
                 <a class="result__snippet">スニペット {n}</a></div></div>"#
        ));
    }
    body.push_str("</div></body></html>");
    body
}

/// 記事 HTML。
pub const ARTICLE: &str = r#"<html lang="ja"><head><title>適応策の手引き</title>
<meta property="og:site_name" content="環境省"></head><body>
<main><article>
<h1>気候変動適応策の手引き</h1>
<p>この記事は気候変動の適応策について、地域での取り組みをまとめたものである。全体像を把握するための導入として読まれたい。</p>
<h2>1. 影響評価</h2>
<p>まず地域の気候リスクを評価する。気温・降水・海面水位の変化を整理し、影響の大きい分野を特定する。</p>
<h2>2. 適応計画の策定</h2>
<p>評価結果に基づき、優先順位を付けて計画を策定する。庁内の体制と予算確保が鍵となる。</p>
<h3>2.1 地域適応計画</h3>
<p>地域ごとの計画は、住民参加の場を設けて策定する。合意形成の記録を残すことが重要である。</p>
<h2>3. 推進と評価</h2>
<p>進捗は毎年確認し、必要に応じて計画を見直す。指標を決めておくと評価が容易になる。</p>
</article></main><footer>© 2026 環境省</footer></body></html>"#;
