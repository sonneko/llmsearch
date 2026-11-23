//! M10: CLI メッセージの英語化・`ui.locale` の廃止・`search.language` の配線。
//! 外部通信は行わない（偽 HTTP のみ）。

mod support;

use clap::Parser;
use llmsearch::core::config::NetworkProfile;
use llmsearch::core::errors::ErrorCode;
use llmsearch::ports::HttpClient;
use llmsearch::ports::http::HttpRequest;
use llmsearch::ui::cli::Cli;
use llmsearch::ui::commands;
use llmsearch::ui::render::Renderer;
use std::sync::Arc;

fn renderer(h: &support::Harness, json: bool) -> Renderer {
    Renderer::new(json, false, true, 80, Arc::clone(&h.app.clock))
}

fn parse(args: &[&str]) -> Cli {
    Cli::try_parse_from(std::iter::once("llmsearch").chain(args.iter().copied())).unwrap()
}

fn run(h: &mut support::Harness, args: &[&str]) -> i32 {
    let json = args.contains(&"--json");
    let cli = parse(args);
    let r = renderer(h, json);
    commands::run(&mut h.app, &r, &h.resolved, cli.command.as_ref().unwrap()).unwrap()
}

fn header<'a>(req: &'a HttpRequest, name: &str) -> Option<&'a str> {
    req.headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

fn query_param(req: &HttpRequest, name: &str) -> Option<String> {
    req.url
        .query_pairs()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.into_owned())
}

fn route_results(h: &support::Harness) {
    let html = support::duckduckgo_html(&[
        (1, "First", "https://one.example/a"),
        (2, "Second", "https://two.example/a"),
    ]);
    h.http.route("html.duckduckgo.com", 200, "text/html", &html);
}

/// `search.language` の既定値は `en` で、`Accept-Language` に載る。
#[test]
fn default_language_is_english() {
    let mut h = support::harness(None);
    route_results(&h);
    run(&mut h, &["search", "climate"]);
    let req = &h.http.requests.lock().unwrap()[0];
    assert_eq!(header(req, "Accept-Language"), Some("en"));
}

/// 言語設定を変えると `Accept-Language` が変わる（main は `--language` を
/// `Bootstrap.language` として `build` に渡す。ここでは env 経由で同じ経路を通す）。
#[test]
fn language_setting_changes_accept_language() {
    let mut h = support::harness(None);
    route_results(&h);
    h.app.settings.search.language = "ja".to_string();
    run(&mut h, &["search", "climate"]);
    let req = &h.http.requests.lock().unwrap()[0];
    assert_eq!(header(req, "Accept-Language"), Some("ja"));
}

/// `--language` は実際に設定値として解決される（Bootstrap.language 経由）。
#[test]
fn language_flag_reaches_settings() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    let env = Arc::new(support::FakeEnv::new(state_dir));
    let bootstrap = llmsearch::usecases::config::Bootstrap {
        language: Some("fr-CA".to_string()),
        ..Default::default()
    };
    let built = llmsearch::usecases::config::build(
        &bootstrap,
        Arc::new(support::FixedClock::new("2026-11-07T05:00:00Z")),
        env,
        Arc::new(support::SeqIds),
        Arc::new(support::SilentReporter::new()),
    );
    let (app, resolved) = built.expect("a valid --language must be accepted");
    assert_eq!(app.settings.search.language, "fr-CA");
    let rows = llmsearch::usecases::config::describe(&app, &resolved);
    let row = rows.iter().find(|r| r.key == "search.language").unwrap();
    assert_eq!(row.origin, llmsearch::usecases::config::Origin::Argument);
}

/// エンジン定義の言語パラメータにも `search.language` が届く
/// （duckduckgo は `kl` の直書きを廃止して `Accept-Language` で届ける）。
#[test]
fn duckduckgo_sends_no_hardcoded_region_language() {
    let mut h = support::harness(None);
    route_results(&h);
    h.app.settings.search.language = "de".to_string();
    run(&mut h, &["search", "climate"]);
    let req = &h.http.requests.lock().unwrap()[0];
    assert_eq!(query_param(req, "kl"), None, "kl must not be hardcoded");
    assert_eq!(header(req, "Accept-Language"), Some("de"));
}

/// Brave は `search_lang={{language}}` に主言語タグが入る（地域は従来どおり）。
#[test]
fn brave_receives_search_lang() {
    let mut h = support::harness(None);
    // Brave は API キーが要るため、定義を読むだけでリクエストを組み立てる。
    let def = h
        .app
        .engines
        .get("brave@api")
        .expect("brave definition")
        .clone();
    let mut secrets = llmsearch::core::secret::Secrets::new();
    secrets.insert("BRAVE_API_KEY", "k");
    h.app.secrets = secrets;
    h.app.settings.search.language = "pt".to_string();
    let run = llmsearch::usecases::engines::dry_run(&h.app, &def, "climate", 1).unwrap();
    assert!(
        run.url.contains("search_lang=pt"),
        "brave must receive search_lang: {}",
        run.url
    );
}

/// SearXNG は `language={{locale}}` に正規化した言語タグが入る。
#[test]
fn searxng_receives_language() {
    let mut h = support::harness(None);
    let def = h
        .app
        .engines
        .get("searxng@json")
        .expect("searxng definition")
        .clone();
    h.app.settings.search.language = "ja".to_string();
    let run = llmsearch::usecases::engines::dry_run(&h.app, &def, "climate", 1).unwrap();
    assert!(
        run.url.contains("language=ja"),
        "searxng must receive language: {}",
        run.url
    );
}

/// 定義に `Accept-Language` があれば定義側を優先する。
#[test]
fn definition_accept_language_wins() {
    let mut h = support::harness(None);
    let mut secrets = llmsearch::core::secret::Secrets::new();
    secrets.insert("BRAVE_API_KEY", "k");
    h.app.secrets = secrets;
    let mut def = h
        .app
        .engines
        .get("brave@api")
        .expect("brave definition")
        .clone();
    def.request.headers.insert(
        "Accept-Language".to_string(),
        llmsearch::core::engine::TmplValue::Value("fr".to_string()),
    );
    let run = llmsearch::usecases::engines::dry_run(&h.app, &def, "q", 1).unwrap();
    let lang = run
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("accept-language"))
        .map(|(_, v)| v.as_str());
    assert_eq!(lang, Some("fr"));
}

/// `--language` の不正値は E1004 で止まる（黙って既定値へ落ちない）。
#[test]
fn invalid_language_flag_is_rejected() {
    assert_eq!(
        llmsearch::core::lang::parse("english!"),
        None,
        "malformed tags must be rejected"
    );
    let h = support::harness(None);
    let bootstrap = llmsearch::usecases::config::Bootstrap {
        language: Some("engl".to_string()),
        ..Default::default()
    };
    let result = llmsearch::usecases::config::build(
        &bootstrap,
        Arc::clone(&h.app.clock),
        Arc::clone(&h.app.env),
        Arc::new(support::SeqIds),
        Arc::new(support::SilentReporter::new()),
    );
    match result {
        Ok(_) => panic!("a malformed --language must be rejected"),
        Err(err) => assert_eq!(err.code, ErrorCode::E1004),
    }
}

/// 設定の `search.language` も検証される（起動時に E1004）。
#[test]
fn invalid_language_in_settings_is_rejected() {
    let mut settings = llmsearch::core::config::Settings::default();
    settings.search.language = "e".to_string();
    assert!(settings.has_fail());
    settings.search.language = "pt-BR".to_string();
    assert!(!settings.has_fail());
}

/// `search.language` を変えても表示文言は英語のまま（表示言語の設定は無い）。
#[test]
fn language_setting_does_not_change_ui_text() {
    let mut h = support::harness(None);
    h.app.settings.search.language = "ja".to_string();
    let msg = llmsearch::core::errors::message(ErrorCode::E5001);
    assert!(
        msg.is_ascii(),
        "the message catalog must stay English: {msg}"
    );
    assert!(msg.contains("session"));
}

/// 旧 `ui.locale` は未知キーとして温存し、W1002 で廃止を知らせる。
#[test]
fn legacy_ui_locale_is_a_warning() {
    let settings: llmsearch::core::config::Settings =
        serde_json::from_str(r#"{"ui": {"color": "auto", "locale": "ja", "pageSize": 7}}"#)
            .unwrap();
    assert!(settings.ui.extra.contains_key("locale"), "must be kept");
    let checks = settings.validate();
    let warn = checks
        .iter()
        .find(|c| c.text().contains("ui.locale was removed"))
        .expect("ui.locale must be reported");
    assert!(matches!(warn, llmsearch::core::errors::Check::Warn(_)));
    assert!(!settings.has_fail(), "a legacy key must not stop the run");

    // 書き戻しても値が消えない
    let back = serde_json::to_value(&settings).unwrap();
    assert_eq!(back["ui"]["locale"], "ja");
}

/// 旧 `LLMSEARCH_UI_LOCALE` は無視する（E1004 にしない）。
#[test]
fn legacy_ui_locale_env_is_ignored() {
    struct FakeEnv(std::collections::BTreeMap<String, String>);
    impl llmsearch::ports::Env for FakeEnv {
        fn var(&self, name: &str) -> Option<String> {
            self.0.get(name).cloned()
        }
        fn var_names(&self) -> Vec<String> {
            self.0.keys().cloned().collect()
        }
        fn is_stdin_tty(&self) -> bool {
            true
        }
        fn is_stdout_tty(&self) -> bool {
            false
        }
        fn terminal_width(&self) -> usize {
            80
        }
        fn state_home(&self) -> Option<std::path::PathBuf> {
            None
        }
        fn config_home(&self) -> Option<std::path::PathBuf> {
            None
        }
        fn executable_dir(&self) -> Option<std::path::PathBuf> {
            None
        }
        fn home_dir(&self) -> Option<std::path::PathBuf> {
            None
        }
    }
    let mut map = std::collections::BTreeMap::new();
    map.insert("LLMSEARCH_UI_LOCALE".to_string(), "ja".to_string());
    let overrides = llmsearch::usecases::config::env_overrides(&FakeEnv(map)).unwrap();
    assert!(
        overrides.iter().all(|(k, _)| k != "ui.locale"),
        "ui.locale must not be an override anymore"
    );
}

/// 旧 `defaults.locale` / `acceptLanguage` も W1002 で知らせる。
#[test]
fn removed_engine_and_profile_keys_warn() {
    let def: llmsearch::core::engine::EngineDef = serde_json::from_str(
        r#"{"id":"x","request":{"url":"https://e.example/"},"response":{"kind":"json","items":{"path":"a"},"fields":{"title":{"path":"t"},"url":{"path":"u"}}},"defaults":{"locale":"ja","region":"jp"}}"#,
    )
    .unwrap();
    let checks = def.validate();
    assert!(
        checks
            .iter()
            .any(|c| c.text().contains("defaults.locale was removed")),
        "defaults.locale must warn: {checks:?}"
    );

    let profile: NetworkProfile =
        serde_json::from_str(r#"{"id":"p","acceptLanguage":"ja,en;q=0.8"}"#).unwrap();
    let checks = profile.validate();
    assert!(
        checks
            .iter()
            .any(|c| c.text().contains("acceptLanguage was removed")),
        "acceptLanguage must warn: {checks:?}"
    );
}

/// `config show` に `search.language` が出て、`ui.locale` は出ない。
#[test]
fn config_show_lists_search_language_only() {
    let h = support::harness(None);
    let rows = llmsearch::usecases::config::describe(&h.app, &h.resolved);
    let keys: Vec<&str> = rows.iter().map(|r| r.key.as_str()).collect();
    assert!(keys.contains(&"search.language"));
    assert!(!keys.contains(&"ui.locale"));
    let row = rows.iter().find(|r| r.key == "search.language").unwrap();
    assert_eq!(row.value, "en");
}

/// `--language` の由来は `argv` として出る。
#[test]
fn config_show_marks_language_origin() {
    let mut h = support::harness(None);
    h.resolved.argv.language = Some("fr".to_string());
    let rows = llmsearch::usecases::config::describe(&h.app, &h.resolved);
    let row = rows.iter().find(|r| r.key == "search.language").unwrap();
    assert_eq!(row.origin, llmsearch::usecases::config::Origin::Argument);
}

/// `--json` のメッセージも英語である。
#[test]
fn json_error_message_is_english() {
    let mut h = support::harness(None);
    let r = renderer(&h, true);
    let err = commands::run(
        &mut h.app,
        &r,
        &h.resolved,
        &llmsearch::ui::cli::Command::Refine {
            query: "q".to_string(),
        },
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::E5001);
    let msg = llmsearch::core::errors::message(err.code);
    assert!(msg.is_ascii(), "{msg}");
    let hint = llmsearch::core::errors::hint(err.code);
    assert!(hint.is_ascii(), "{hint}");
}

/// カタログ全体（全 E コードの message / hint）に日本語が無い。
#[test]
fn catalog_is_ascii_only() {
    use llmsearch::core::errors::ErrorCode::*;
    let codes = [
        E1001, E1002, E1003, E1004, E1005, E1006, E1007, E1008, E2001, E2002, E2003, E2004, E2005,
        E2006, E2007, E2008, E3001, E3002, E3003, E3004, E4001, E4002, E4003, E4004, E4005, E4006,
        E5001, E5002, E5003, E5004, E5005,
    ];
    for code in codes {
        let m = llmsearch::core::errors::message(code);
        assert!(m.is_ascii(), "{code:?}: {m}");
        let h = llmsearch::core::errors::hint(code);
        assert!(h.is_ascii(), "{code:?}: {h}");
    }
}

/// 偽 HTTP は `HttpClient` を満たす（未使用警告を避けるための型チェック）。
#[test]
fn fake_http_is_an_http_client() {
    let h = support::harness(None);
    let client: &dyn HttpClient = h.http.as_ref();
    let profile = llmsearch::core::config::NetworkProfile::default();
    assert_eq!(client.describe(&profile), "fake:default");
}

/// 廃止キーの `W1002` は開始前に利用者へ届く（`resolved.warnings` に載る）。
#[test]
fn removed_key_warning_is_emitted_to_the_user() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    let settings_path = dir.path().join("settings.json");
    std::fs::write(
        &settings_path,
        r#"{"schemaVersion": 2, "ui": {"color": "auto", "locale": "ja", "pageSize": 7}}"#,
    )
    .unwrap();
    let env = Arc::new(support::FakeEnv::new(state_dir));
    let bootstrap = llmsearch::usecases::config::Bootstrap {
        config_path: Some(settings_path),
        ..Default::default()
    };
    let built = llmsearch::usecases::config::build(
        &bootstrap,
        Arc::new(support::FixedClock::new("2026-11-07T05:00:00Z")),
        env,
        Arc::new(support::SeqIds),
        Arc::new(support::SilentReporter::new()),
    );
    let (_app, resolved) = built.expect("a legacy key must not stop the run");
    let warning = resolved
        .warnings
        .iter()
        .find(|w| w.code == llmsearch::core::errors::WarningCode::W1002)
        .expect("W1002 must be emitted");
    assert!(warning.message.contains("ui.locale was removed"));
}
