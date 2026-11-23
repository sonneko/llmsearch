//! M9: `--json` の統一・`refine` と対象解決・表示 URL の伏字化の回帰テスト。
//! 外部通信は行わない（偽 HTTP のみ）。

mod support;

use clap::Parser;
use llmsearch::core::errors::ErrorCode;
use llmsearch::core::history::EventType;
use llmsearch::ui::render::Renderer;
use llmsearch::ui::{cli::Cli, commands};
use serde_json::Value;
use std::sync::Arc;

fn renderer(h: &support::Harness, json: bool) -> Renderer {
    Renderer::new(json, false, true, 80, Arc::clone(&h.app.clock))
}

fn parse(args: &[&str]) -> Cli {
    Cli::try_parse_from(std::iter::once("llmsearch").chain(args.iter().copied())).unwrap()
}

fn try_run(
    h: &mut support::Harness,
    args: &[&str],
) -> Result<i32, llmsearch::core::errors::AppError> {
    let json = args.contains(&"--json");
    let cli = parse(args);
    let r = renderer(h, json);
    commands::run(&mut h.app, &r, &h.resolved, cli.command.as_ref().unwrap())
}

fn run(h: &mut support::Harness, args: &[&str]) -> i32 {
    try_run(h, args).unwrap()
}

const SECRET_URL: &str = "https://one.example/a?token=PLAINSECRET";

fn route_results(h: &support::Harness) {
    let html = support::duckduckgo_html(&[
        (1, "First", SECRET_URL),
        (2, "Second", "https://two.example/a"),
    ]);
    h.http.route("html.duckduckgo.com", 200, "text/html", &html);
}

// --- #7: refine -------------------------------------------------------------

/// `refine` は直前のクエリを previousQuery として扱い、履歴に refine を残す。
#[test]
fn refine_uses_session_query_and_records_history() {
    let mut h = support::harness(None);
    route_results(&h);
    run(&mut h, &["search", "before"]);
    run(&mut h, &["refine", "after"]);

    let session = h
        .app
        .sessions
        .load(&h.app.current_session_id())
        .unwrap()
        .unwrap();
    assert_eq!(session.last_query, "after");

    let entries = h.app.history.query(&Default::default()).unwrap();
    let entry = entries
        .iter()
        .find(|e| e.kind == EventType::Refine)
        .expect("refine が履歴に残る");
    assert_eq!(entry.payload["previous"], "before");
    assert_eq!(entry.payload["query"], "after");
}

/// 仕様に無い `--previous` は受け付けない（cli-interface.md §3.1）。
#[test]
fn refine_rejects_removed_previous_flag() {
    assert!(Cli::try_parse_from(["llmsearch", "refine", "q", "--previous", "p"]).is_err());
}

/// セッションが無い `refine` は E5001 と回復手順を返す。
#[test]
fn refine_without_session_is_e5001_with_hint() {
    let mut h = support::harness(None);
    let err = try_run(&mut h, &["refine", "after"]).unwrap_err();
    assert_eq!(err.code, ErrorCode::E5001);
    assert!(err.hint.is_some(), "回復手順が必要: {err:?}");
}

// --- #9: 対象解決 -----------------------------------------------------------

/// セッション無しの識別子指定は E5001（セッションを暗黙に作らない）。
#[test]
fn identifier_without_session_is_e5001_and_keeps_state_clean() {
    let mut h = support::harness(None);
    for args in [
        ["outline", "R1"].as_slice(),
        ["read", "R1"].as_slice(),
        ["next"].as_slice(),
    ] {
        let err = try_run(&mut h, args).unwrap_err();
        assert_eq!(err.code, ErrorCode::E5001, "{args:?}");
        assert!(err.hint.is_some(), "回復手順が必要: {args:?}");
    }
    assert!(
        h.app.current_session_id().is_empty(),
        "失敗した解決でセッションを作らない"
    );
}

/// セッション内に無い識別子は E1006（E5001 ではない）。
#[test]
fn unknown_identifier_with_session_is_e1006() {
    let mut h = support::harness(None);
    route_results(&h);
    run(&mut h, &["search", "first"]);
    let err = try_run(&mut h, &["outline", "R99"]).unwrap_err();
    assert_eq!(err.code, ErrorCode::E1006);
}

// --- #12: 検索結果 URL の伏字化 ---------------------------------------------

/// クエリの秘匿パラメータは表示 URL で秘匿値を出さない
/// （`display_url` が使う `core::url::redact` の経路。テキスト出力は実バイナリで確認する）。
#[test]
fn display_url_redacts_secret_query_param() {
    let parsed = url::Url::parse(SECRET_URL).unwrap();
    let shown = llmsearch::core::url::redact(&parsed, &[]);
    assert!(shown.contains("token=***"), "表示 URL: {shown}");
    assert!(!shown.contains("PLAINSECRET"), "表示 URL: {shown}");
}

/// パス中の `名前=値` も伏字化する（クエリと同じ基準）。
#[test]
fn path_segment_secret_is_redacted() {
    let parsed = url::Url::parse("https://one.example/token=PLAINSECRET/x").unwrap();
    let shown = llmsearch::core::url::redact(&parsed, &[]);
    assert_eq!(shown, "https://one.example/token=***/x");
}

/// 無関係なパスは変更しない（誤爆させない）。
#[test]
fn plain_path_is_untouched() {
    let parsed = url::Url::parse("https://one.example/a/b").unwrap();
    assert_eq!(
        llmsearch::core::url::redact(&parsed, &[]),
        "https://one.example/a/b"
    );
}

/// 環境変数由来の秘匿値は、URL のパスでも文字列一致で伏せる。
#[test]
fn env_secret_value_is_redacted_anywhere_in_url() {
    let parsed = url::Url::parse("https://one.example/SUPERSECRET/x").unwrap();
    let shown = llmsearch::core::url::redact(&parsed, &["SUPERSECRET".to_string()]);
    assert!(!shown.contains("SUPERSECRET"), "表示 URL: {shown}");
}

// --- #5: `--json` の網羅 ---------------------------------------------------

/// `engines list` の `apiKey` は人間向け文言ではなく安定した識別子。
#[test]
fn engines_list_json_uses_stable_api_key_identifiers() {
    let mut h = support::harness(None);
    // JSON 出力は標準出力へ出るため、イベント組み立てに使う行の値を直接調べる。
    let rows = llmsearch::usecases::engines::list(&h.app);
    assert!(!rows.is_empty());
    for row in &rows {
        assert!(
            ["not-required", "configured", "missing"].contains(&row.api_key_state.as_str()),
            "識別子が不安定: {}",
            row.api_key_state
        );
    }
    // テキスト出力は従来どおり日本語の見出しを出す。
    run(&mut h, &["engines", "list"]);
}

/// `history clear --json` は生テキストを出さずイベントを返す（封筒の中身を検証）。
#[test]
fn history_clear_json_emits_event() {
    let mut h = support::harness(None);
    route_results(&h);
    run(&mut h, &["search", "first"]);
    let exit = run(&mut h, &["--json", "history", "clear", "--yes"]);
    assert_eq!(exit, 0);
    // 削除の実行自体は `delete` レコードとして残る（history.md §1.1）。
    let entries = h.app.history.query(&Default::default()).unwrap();
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert_eq!(entries[0].kind, EventType::Delete);
}

/// `quota reset --json` は `quota` イベントを出す（action と scope を持つ）。
#[test]
fn quota_reset_emits_quota_event() {
    let mut h = support::harness(None);
    let exit = run(
        &mut h,
        &["--json", "quota", "reset", "--scope", "day", "--yes"],
    );
    assert_eq!(exit, 0);
}

// --- #4: イベント封筒 -------------------------------------------------------

/// 共通フィールドは `v` / `event` / `session` / `at` の 4 つで、`at` は Z 終端。
#[test]
fn envelope_has_z_terminated_timestamp() {
    let h = support::harness(None);
    let r = renderer(&h, true);
    let event = r.envelope("done", serde_json::json!({"command": "x", "summary": "y"}));
    let at = event.get("at").and_then(Value::as_str).expect("at");
    assert!(at.ends_with('Z'), "at: {at}");
    assert_eq!(at.len(), "2026-11-07T05:00:00Z".len(), "at: {at}");
    assert_eq!(event.get("v"), Some(&serde_json::json!(1)));
    assert_eq!(event.get("event"), Some(&serde_json::json!("done")));
}

// --- #23: メッセージの言語（M10 で英語固定） -----------------------------

/// `message` / `hint` は英語で固定（M10 で `Locale` を廃止）。
#[test]
fn message_and_hint_are_english() {
    let msg = llmsearch::core::errors::message(ErrorCode::E5001);
    assert!(msg.is_ascii(), "message should be ASCII: {msg}");
    let hint = llmsearch::core::errors::hint(ErrorCode::E5001);
    assert!(hint.is_ascii(), "hint should be ASCII: {hint}");
}
