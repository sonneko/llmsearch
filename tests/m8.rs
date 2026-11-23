//! M8: 設定の表示・探索順・クォータ監査・検索統合の回帰テスト。
//!
//! 外部ネットワークは使わない（偽 HTTP のみ）。
mod support;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::Parser;
use llmsearch::core::errors::ErrorCode;
use llmsearch::ports::Env;
use llmsearch::ui::{cli::Cli, commands, render::Renderer};
use llmsearch::usecases::config::{Bootstrap, Origin, Resolved, build, describe, paths};
use llmsearch::usecases::{App, search};
use serde_json::json;

// ---------------------------------------------------------------------------
// テスト用の環境（利用者設定ディレクトリを指定できる）
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct TestEnv {
    vars: BTreeMap<String, String>,
    state_dir: PathBuf,
    config_home: Option<PathBuf>,
}

impl TestEnv {
    fn new(state_dir: &Path, config_home: Option<PathBuf>) -> Self {
        Self {
            vars: BTreeMap::new(),
            state_dir: state_dir.to_path_buf(),
            config_home,
        }
    }

    fn with(mut self, key: &str, value: &str) -> Self {
        self.vars.insert(key.to_string(), value.to_string());
        self
    }
}

impl Env for TestEnv {
    fn var(&self, name: &str) -> Option<String> {
        self.vars.get(name).cloned()
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
        self.config_home.clone()
    }

    /// 同梱アセットはリポジトリの `assets/` を使う（M11 の上方探索）。
    fn current_dir(&self) -> Option<PathBuf> {
        Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")))
    }
}

/// 組み立て結果一式。
struct Ctx {
    app: App,
    resolved: Resolved,
    http: Arc<support::FakeHttp>,
    dir: tempfile::TempDir,
}

fn context(env: TestEnv, extra_engines: Option<&str>, bootstrap: Bootstrap) -> Ctx {
    let dir = tempfile::tempdir().unwrap();
    let mut engines_dirs = Vec::new();
    if let Some(body) = extra_engines {
        let engine_dir = dir.path().join("engines");
        std::fs::create_dir_all(&engine_dir).unwrap();
        std::fs::write(engine_dir.join("m8.json"), body).unwrap();
        engines_dirs.push(engine_dir);
    }
    let mut bootstrap = Bootstrap {
        engines_dirs,
        engine: Some("duckduckgo".to_string()),
        quiet: true,
        ..bootstrap
    };
    if bootstrap.overrides.is_empty() {
        bootstrap.overrides = llmsearch::usecases::config::env_overrides(&env).unwrap();
    }
    let clock = Arc::new(support::FixedClock::new("2026-11-07T05:00:00Z"));
    let reporter = Arc::new(support::SilentReporter::new());
    let (app, resolved) = build(
        &bootstrap,
        clock,
        Arc::new(env),
        Arc::new(support::SeqIds),
        reporter,
    )
    .expect("アプリの組み立てに失敗");
    let http = Arc::new(support::FakeHttp::new());
    let mut app = app;
    app.set_http(http.clone());
    app.session_id.lock().unwrap().clear();
    Ctx {
        app,
        resolved,
        http,
        dir,
    }
}

fn renderer(ctx: &Ctx) -> Renderer {
    Renderer::new(false, false, true, 100, Arc::clone(&ctx.app.clock))
}

fn execute(ctx: &mut Ctx, args: &[&str]) {
    let cli =
        Cli::try_parse_from(std::iter::once("llmsearch").chain(args.iter().copied())).unwrap();
    let r = renderer(ctx);
    commands::run(
        &mut ctx.app,
        &r,
        &ctx.resolved,
        cli.command.as_ref().unwrap(),
    )
    .unwrap();
}

fn write_settings(state_dir: &Path, body: &str) {
    std::fs::create_dir_all(state_dir).unwrap();
    std::fs::write(state_dir.join("settings.json"), body).unwrap();
}

// ---------------------------------------------------------------------------
// #11: config show / path / init
// ---------------------------------------------------------------------------

/// `config show` は全キーを出し、由来を `argv` / `env` / `file` / `default` で示す。
#[test]
fn config_show_reports_origin_for_every_key() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    // 状態ディレクトリの settings.json に 1 キーだけ置く
    write_settings(&state.join("llmsearch"), r#"{"history":{"enabled":false}}"#);

    let env = TestEnv::new(&state, None).with("LLMSEARCH_SEARCH_LIMIT", "5");
    let ctx = context(
        env,
        None,
        Bootstrap {
            engine: Some("duckduckgo".into()),
            ..Bootstrap::default()
        },
    );

    let rows = describe(&ctx.app, &ctx.resolved);
    let find = |key: &str| rows.iter().find(|r| r.key == key).expect(key).clone();

    // 引数（`search.engine` は `--engine` 相当で決まる）
    assert_eq!(find("search.engine").origin, Origin::Argument);
    // 環境変数
    assert_eq!(find("search.limit").value, "5");
    assert_eq!(
        find("search.limit").origin,
        Origin::Env("LLMSEARCH_SEARCH_LIMIT".into())
    );
    // 設定ファイル
    match find("history.enabled").origin {
        Origin::File(path) => assert!(path.ends_with("settings.json")),
        other => panic!("file 由来になるはず: {other:?}"),
    }
    // 既定
    assert_eq!(find("context.maxChars").origin, Origin::Default);
    // 全キーに由来判定が付く（未設定のキーは default）
    assert!(rows.len() > 20, "全キーを出す: {}", rows.len());
    assert!(rows.iter().all(|r| !r.origin.kind().is_empty()));
}

/// `config explain` は `config show` と同じ由来を返し、未知キーは `E1004`。
#[test]
fn config_explain_matches_show_and_rejects_unknown_keys() {
    let dir = tempfile::tempdir().unwrap();
    let env =
        TestEnv::new(&dir.path().join("state"), None).with("LLMSEARCH_CONTEXT_MAX_CHARS", "4000");
    let ctx = context(env, None, Bootstrap::default());

    let (value, origin) = llmsearch::usecases::config::explain(
        &ctx.app,
        &ctx.resolved,
        ctx.app.env.as_ref(),
        "context.maxChars",
    )
    .unwrap();
    assert_eq!(value, "4000");
    assert_eq!(
        origin,
        Origin::Env("LLMSEARCH_CONTEXT_MAX_CHARS".to_string())
    );

    let err = llmsearch::usecases::config::explain(
        &ctx.app,
        &ctx.resolved,
        ctx.app.env.as_ref(),
        "context.maxChar",
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::E1004);
    assert!(
        err.detail
            .as_deref()
            .unwrap_or_default()
            .contains("context.maxChars"),
        "近いキーを候補に出す: {err:?}"
    );
}

/// `config path` は読み込んだファイルと存在しない候補の双方を返す。
#[test]
fn config_path_lists_used_and_missing_candidates() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let config_home = dir.path().join("config");
    std::fs::create_dir_all(&config_home).unwrap();
    let env = TestEnv::new(&state, Some(config_home.clone()));
    let ctx = context(env, None, Bootstrap::default());

    let rows = paths(&ctx.app, &ctx.resolved);
    let user = config_home.join("llmsearch/settings.json");
    let state_settings = ctx.app.paths.root.join("settings.json");
    let user_row = rows
        .iter()
        .find(|r| r.path == user)
        .expect("利用者設定の候補");
    assert!(!user_row.used, "存在しない候補として出す");
    let state_row = rows
        .iter()
        .find(|r| r.path == state_settings)
        .expect("状態ディレクトリの候補");
    assert!(!state_row.used);

    // 資源とエンジンの探索先も並ぶ
    assert!(rows.iter().any(|r| r.label == "network"));
    assert!(rows.iter().any(|r| r.label == "engines"));
    assert!(rows.iter().any(|r| r.label == "state" && r.used));

    // 書き出すと `used` になる
    write_settings(&ctx.app.paths.root, "{}");
    let ctx = context(
        TestEnv::new(&state, Some(config_home)),
        None,
        Bootstrap::default(),
    );
    let rows = paths(&ctx.app, &ctx.resolved);
    let state_row = rows
        .iter()
        .find(|r| r.path == ctx.app.paths.root.join("settings.json"))
        .expect("状態ディレクトリの候補");
    assert!(state_row.used);
}

/// `config init` は 4 ファイルを書き出し、既存は `--force` 無しで飛ばす。
#[test]
fn config_init_writes_four_templates_and_keeps_existing() {
    let dir = tempfile::tempdir().unwrap();
    let env = TestEnv::new(&dir.path().join("state"), None);
    let mut ctx = context(env, None, Bootstrap::default());

    execute(&mut ctx, &["config", "init"]);
    let root = ctx.app.paths.root.clone();
    for rel in [
        "settings.json",
        "network/default.json",
        "filters/default.json",
        "quotas/default.json",
    ] {
        let path = root.join(rel);
        assert!(path.is_file(), "雛形が無い: {}", path.display());
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["schemaVersion"], json!(2), "{rel} に schemaVersion");
    }

    // 1 ファイルを書き換えてから再実行しても上書きしない
    std::fs::write(root.join("settings.json"), "{\"custom\":true}").unwrap();
    let report = llmsearch::usecases::config::init(&root, false).unwrap();
    assert!(report.written.is_empty());
    assert_eq!(report.skipped.len(), 4);
    assert!(
        std::fs::read_to_string(root.join("settings.json"))
            .unwrap()
            .contains("custom")
    );

    // `--force` はすべて上書きする
    let report = llmsearch::usecases::config::init(&root, true).unwrap();
    assert_eq!(report.written.len(), 4);
    assert!(
        !std::fs::read_to_string(root.join("settings.json"))
            .unwrap()
            .contains("custom")
    );
    let _ = ctx.dir;
}

// ---------------------------------------------------------------------------
// #19: quota reset の履歴
// ---------------------------------------------------------------------------

/// `quota reset` は範囲・ポリシー・前後の消費量を履歴に残す。
#[test]
fn quota_reset_is_recorded_in_history() {
    let dir = tempfile::tempdir().unwrap();
    let env = TestEnv::new(&dir.path().join("state"), None);
    let mut ctx = context(env, None, Bootstrap::default());

    // まず 1 回検索して消費を作る
    let html = support::duckduckgo_html(&[(1, "First", "https://one.example/a")]);
    ctx.http
        .route("html.duckduckgo.com", 200, "text/html", &html);
    execute(&mut ctx, &["search", "first"]);
    let before = ctx.app.guard().unwrap().counters().session.searches;
    assert!(before > 0, "検索で消費している");

    execute(&mut ctx, &["quota", "reset", "--scope", "day", "--yes"]);

    let entries = ctx.app.history.query(&Default::default()).unwrap();
    let reset = entries
        .iter()
        .find(|e| {
            e.kind == llmsearch::core::history::EventType::Quota
                && e.payload.get("action").and_then(|v| v.as_str()) == Some("reset")
        })
        .expect("quota reset の履歴が無い");
    assert_eq!(reset.payload["scope"], json!("day"));
    assert_eq!(reset.payload["policy"], json!(ctx.app.quota.id));
    assert!(reset.cost.is_some(), "リセット前の消費量を残す");
    assert!(reset.remaining.is_some(), "リセット後の消費量を残す");
    assert!(
        reset.cost.as_ref().unwrap().searches >= 1,
        "リセット前は検索 1 回以上"
    );

    // 消費量は 0 に戻っている
    let after = ctx.app.guard().unwrap().counters().day.searches;
    assert_eq!(after, 0);
}

/// `--yes` が無ければリセットも履歴も行わない。
#[test]
fn quota_reset_requires_yes_and_records_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let env = TestEnv::new(&dir.path().join("state"), None);
    let mut ctx = context(env, None, Bootstrap::default());
    let count_before = ctx.app.history.query(&Default::default()).unwrap().len();

    let cli = Cli::try_parse_from(["llmsearch", "quota", "reset", "--scope", "day"]).unwrap();
    let r = renderer(&ctx);
    let err = commands::run(
        &mut ctx.app,
        &r,
        &ctx.resolved,
        cli.command.as_ref().unwrap(),
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::E1004);
    assert_eq!(
        ctx.app.history.query(&Default::default()).unwrap().len(),
        count_before
    );
}

// ---------------------------------------------------------------------------
// #20: 探索順と環境変数
// ---------------------------------------------------------------------------

/// 状態ディレクトリの設定が利用者設定ディレクトリより強い（仕様 §2.3）。
#[test]
fn state_dir_settings_win_over_user_config_dir() {
    let dir = tempfile::tempdir().unwrap();
    let config_home = dir.path().join("config");
    std::fs::create_dir_all(config_home.join("llmsearch")).unwrap();
    std::fs::write(
        config_home.join("llmsearch/settings.json"),
        r#"{"history":{"enabled":false},"search":{"limit":3}}"#,
    )
    .unwrap();
    let state = dir.path().join("state");
    write_settings(&state.join("llmsearch"), r#"{"history":{"enabled":true}}"#);

    let ctx = context(
        TestEnv::new(&state, Some(config_home)),
        None,
        Bootstrap::default(),
    );
    assert!(ctx.app.settings.history.enabled, "状態ディレクトリが強い");
    assert_eq!(ctx.app.settings.search.limit, 3, "弱い側のキーも生きる");

    // 逆順（状態ディレクトリだけが false）でも状態ディレクトリが勝つ
    write_settings(&state.join("llmsearch"), r#"{"history":{"enabled":false}}"#);
    let ctx = context(
        TestEnv::new(&state, Some(dir.path().join("config"))),
        None,
        Bootstrap::default(),
    );
    assert!(!ctx.app.settings.history.enabled);
}

/// `LLMSEARCH_CONFIG` が探索順 2 番目として効き、不在は `E1003`。
#[test]
fn llmsearch_config_env_selects_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let target = dir.path().join("custom.json");
    std::fs::write(&target, r#"{"search":{"limit":2}}"#).unwrap();
    // 状態ディレクトリ側の設定は読まれない（`LLMSEARCH_CONFIG` はそのファイルだけを使う）
    write_settings(&state.join("llmsearch"), r#"{"search":{"limit":9}}"#);

    let env = TestEnv::new(&state, None).with("LLMSEARCH_CONFIG", &target.display().to_string());
    let ctx = context(env, None, Bootstrap::default());
    assert_eq!(ctx.app.settings.search.limit, 2);
    assert!(ctx.resolved.sources.contains(&target));

    let missing = dir.path().join("nope.json");
    let env = TestEnv::new(&state, None).with("LLMSEARCH_CONFIG", &missing.display().to_string());
    let clock = Arc::new(support::FixedClock::new("2026-11-07T05:00:00Z"));
    let err = build(
        &Bootstrap {
            engine: Some("duckduckgo".into()),
            quiet: true,
            ..Bootstrap::default()
        },
        clock,
        Arc::new(env),
        Arc::new(support::SeqIds),
        Arc::new(support::SilentReporter::new()),
    )
    .map(|_| ())
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::E1003);
}

/// 解釈できない環境変数は捨てずに `E1004`。
#[test]
fn unparsable_env_value_is_e1004() {
    let env = TestEnv::new(Path::new("/tmp/m8"), None).with("LLMSEARCH_SEARCH_LIMIT", "many");
    let err = llmsearch::usecases::config::env_overrides(&env).unwrap_err();
    assert_eq!(err.code, ErrorCode::E1004);
    assert!(err.detail.as_deref().unwrap_or_default().contains("many"));
}

/// `LLMSEARCH_TIMEOUT_MS` / `LLMSEARCH_USER_AGENT` はネットワークプロファイルへ写る。
#[test]
fn network_env_overrides_apply() {
    let dir = tempfile::tempdir().unwrap();
    let env = TestEnv::new(&dir.path().join("state"), None)
        .with("LLMSEARCH_TIMEOUT_MS", "9000")
        .with("LLMSEARCH_USER_AGENT", "llmsearch-test/1");
    let ctx = context(env, None, Bootstrap::default());
    assert_eq!(ctx.app.network.timeout_total_ms, 9000);
    assert_eq!(ctx.app.network.user_agent, "llmsearch-test/1");

    let env = TestEnv::new(&dir.path().join("state"), None).with("LLMSEARCH_USER_AGENT", " ");
    let err = build(
        &Bootstrap {
            engine: Some("duckduckgo".into()),
            quiet: true,
            ..Bootstrap::default()
        },
        Arc::new(support::FixedClock::new("2026-11-07T05:00:00Z")),
        Arc::new(env),
        Arc::new(support::SeqIds),
        Arc::new(support::SilentReporter::new()),
    )
    .map(|_| ())
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::E1004);
}

// ---------------------------------------------------------------------------
// #22: search.merge のラウンドロビン統合
// ---------------------------------------------------------------------------

/// 2 エンジン分の定義（HTML 応答を 1 つずつ返す）。
const TWO_ENGINES: &str = r#"[
  {"schemaVersion":1,"id":"alpha",
   "request":{"url":"https://alpha.example/s","query":{"q":"{{query}}"},"headers":{}},
   "response":{"kind":"html","items":{"selector":"div.r"},
     "fields":{"title":{"selector":"a"},"url":{"selector":"a","attr":"href"}}},
   "limits":{"maxPageSize":10,"maxResults":30,"minIntervalMs":0}},
  {"schemaVersion":1,"id":"beta",
   "request":{"url":"https://beta.example/s","query":{"q":"{{query}}"},"headers":{}},
   "response":{"kind":"html","items":{"selector":"div.r"},
     "fields":{"title":{"selector":"a"},"url":{"selector":"a","attr":"href"}}},
   "limits":{"maxPageSize":10,"maxResults":30,"minIntervalMs":0}}
]"#;

fn engine_html(items: &[(&str, &str)]) -> String {
    let mut body = String::from("<html><body>");
    for (title, url) in items {
        body.push_str(&format!(
            r#"<div class="r"><a href="{url}">{title}</a></div>"#
        ));
    }
    body.push_str("</body></html>");
    body
}

fn merge_context() -> Ctx {
    let dir = tempfile::tempdir().unwrap();
    let env = TestEnv::new(&dir.path().join("state"), None);
    let mut ctx = context(env, Some(TWO_ENGINES), Bootstrap::default());
    ctx.app.settings.search.merge = true;
    ctx.app.settings.search.engines = vec!["alpha".into(), "beta".into()];
    ctx.app.settings.search.limit = 7;
    ctx.http.route(
        "alpha.example",
        200,
        "text/html",
        &engine_html(&[
            ("a1", "https://one.example/1"),
            ("a2", "https://one.example/2"),
            ("a3", "https://one.example/3"),
        ]),
    );
    ctx.http.route(
        "beta.example",
        200,
        "text/html",
        &engine_html(&[
            ("b1", "https://two.example/1"),
            ("b2", "https://two.example/2"),
        ]),
    );
    ctx
}

/// 全エンジンへ送り、結果を 1 件ずつ交互に統合する。
#[test]
fn merge_alternates_engines_and_tags_owner() {
    let mut ctx = merge_context();
    let outcome = search::run(&mut ctx.app, "topic", 1).unwrap();

    let titles: Vec<&str> = outcome.hits.iter().map(|h| h.title.as_str()).collect();
    assert_eq!(titles, vec!["a1", "b1", "a2", "b2", "a3"]);
    let engines: Vec<&str> = outcome.hits.iter().map(|h| h.engine.as_str()).collect();
    assert_eq!(engines, vec!["alpha", "beta", "alpha", "beta", "alpha"]);
    // 識別子は統合後に 1 から連番
    assert_eq!(
        outcome.hits.iter().map(|h| h.id).collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5]
    );
    assert_eq!(ctx.http.request_count(), 2, "両方へ送る");

    // クォータは送信した回数だけ消費する
    let counters = ctx.app.guard().unwrap().counters().clone();
    assert_eq!(counters.session.searches, 2);
    assert_eq!(counters.session.results, 5, "フィルタ前の取得件数");
}

/// 重複 URL は除去し、`W4005` を記録する。
#[test]
fn merge_removes_duplicates_and_warns() {
    let mut ctx = merge_context();
    ctx.http.route(
        "beta.example",
        200,
        "text/html",
        &engine_html(&[
            ("dup", "https://one.example/1"),
            ("b2", "https://two.example/2"),
        ]),
    );
    let outcome = search::run(&mut ctx.app, "topic", 1).unwrap();
    let urls: Vec<&str> = outcome.hits.iter().map(|h| h.url.as_str()).collect();
    assert_eq!(
        urls.iter()
            .filter(|u| **u == "https://one.example/1")
            .count(),
        1
    );
    assert!(
        outcome
            .warnings
            .iter()
            .any(|w| w.code == llmsearch::core::errors::WarningCode::W4005),
        "重複除去の警告が無い: {:?}",
        outcome.warnings
    );
}

/// 片方が失敗しても、残りの結果で成功する（失敗は警告 1 行）。
#[test]
fn merge_survives_one_engine_failure() {
    let mut ctx = merge_context();
    ctx.http.route("beta.example", 500, "text/html", "");
    let outcome = search::run(&mut ctx.app, "topic", 1).unwrap();
    assert_eq!(outcome.hits.len(), 3);
    assert!(outcome.hits.iter().all(|h| h.engine == "alpha"));
    assert!(
        outcome.warnings.iter().any(|w| w.message.contains("beta")),
        "失敗したエンジンを示す: {:?}",
        outcome.warnings
    );
}

/// 全滅した場合は消費せずにエラーを返す（仕様 quotas.md §3.2）。
#[test]
fn merge_all_failed_consumes_nothing() {
    let mut ctx = merge_context();
    ctx.http.route("alpha.example", 500, "text/html", "");
    ctx.http.route("beta.example", 503, "text/html", "");
    let err = search::run(&mut ctx.app, "topic", 1).unwrap_err();
    assert!(
        matches!(err.code, ErrorCode::E2003 | ErrorCode::E4001),
        "{err:?}"
    );
    let counters = ctx.app.guard().unwrap().counters().clone();
    assert_eq!(counters.session.searches, 0, "失敗では消費しない");
}

/// `search.merge=false` では従来どおりフォールバック（1 エンジン目で成功）。
#[test]
fn without_merge_first_engine_is_used_alone() {
    let mut ctx = merge_context();
    ctx.app.settings.search.merge = false;
    ctx.app.default_engine = "alpha".to_string();
    let outcome = search::run(&mut ctx.app, "topic", 1).unwrap();
    assert_eq!(outcome.hits.len(), 3);
    assert!(outcome.hits.iter().all(|h| h.engine == "alpha"));
    assert_eq!(ctx.http.request_count(), 1);
}

/// `Resolved` の既定値が壊れていないこと（他テストの前提）。
#[test]
fn resolved_defaults_are_empty() {
    let resolved = Resolved::default();
    assert!(resolved.sources.is_empty());
    assert!(resolved.argv.config_path.is_none());
}
