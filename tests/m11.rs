//! M11: 配布物のアセット解決（`E1003` の再発防止）。
//!
//! CI の Build & Distribution Compatibility は、ステージした `assets/` が
//! 入れ子になり空になった状態で `E1003` になった。ここでは
//! 「`assets/` が無い・壊れている場合でも埋め込み既定で起動できること」と
//! 「探索順（バイナリの隣 → 作業ディレクトリから上方）」を固定する。

mod support;

use std::path::PathBuf;
use std::sync::Arc;

use llmsearch::usecases::config::{Bootstrap, Origin, build, bundled_root, describe};
use support::{FakeEnv, FixedClock, SeqIds, SilentReporter};

/// `Bootstrap` を既定で組み立てる。
fn bootstrap() -> Bootstrap {
    Bootstrap {
        engine: Some("duckduckgo".to_string()),
        quiet: true,
        ..Bootstrap::default()
    }
}

/// 状態ディレクトリを 1 つ作る。
fn state_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("llmsearch-m11-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// 完全な `assets/` を指定先へ作る。
fn write_assets(root: &std::path::Path) {
    for (rel, body) in [
        ("config/settings.json", "{\"schemaVersion\": 2}"),
        (
            "network/default.json",
            r#"{"schemaVersion": 2, "id": "default", "description": "bundled default"}"#,
        ),
        (
            "filters/default.json",
            r#"{"schemaVersion": 2, "id": "default", "description": "bundled default"}"#,
        ),
        (
            "quotas/default.json",
            r#"{"schemaVersion": 2, "id": "default", "description": "bundled default"}"#,
        ),
    ] {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }
}

/// 埋め込み既定だけでもアプリを組み立てられる（`assets/` が無い配布物）。
#[test]
fn starts_without_any_assets_directory() {
    let env = Arc::new(FakeEnv::new(state_dir("no-assets")));
    let built = build(
        &bootstrap(),
        Arc::new(FixedClock::new("2026-11-07T05:00:00Z")),
        env,
        Arc::new(SeqIds),
        Arc::new(SilentReporter::new()),
    );
    let (app, resolved) = built.expect("embedded defaults must be enough to start");
    assert_eq!(app.network.id, "default");
    assert_eq!(app.filters.set.id, "default");
    assert_eq!(app.quota.id, "default");
    assert!(app.engines.get("duckduckgo").is_some());
    assert!(
        resolved.network_path.is_none(),
        "an embedded resource must not pretend to be a file"
    );
}

/// 埋め込みで解決した資源は `config show` で `embedded` と分かる。
#[test]
fn config_show_marks_embedded_resources() {
    let env = Arc::new(FakeEnv::new(state_dir("embedded-show")));
    let (app, resolved) = build(
        &bootstrap(),
        Arc::new(FixedClock::new("2026-11-07T05:00:00Z")),
        env,
        Arc::new(SeqIds),
        Arc::new(SilentReporter::new()),
    )
    .unwrap();
    let rows = describe(&app, &resolved);
    for key in ["network", "filters", "quota"] {
        let row = rows.iter().find(|r| r.key == key).unwrap();
        assert_eq!(row.origin, Origin::Default, "{key}");
        assert_eq!(row.source, "embedded", "{key} must not hide its origin");
    }
}

/// バイナリの隣に完全な `assets/` があれば、それを第一候補にする。
#[test]
fn adjacent_assets_win_over_embedded() {
    let root = state_dir("adjacent");
    let exe = root.join("bin");
    std::fs::create_dir_all(&exe).unwrap();
    write_assets(&exe.join("assets"));

    let env = Arc::new(FakeEnv::new(root.join("state")).with_executable_dir(exe.clone()));
    assert_eq!(bundled_root(env.as_ref()), exe.join("assets"));

    let (app, resolved) = build(
        &bootstrap(),
        Arc::new(FixedClock::new("2026-11-07T05:00:00Z")),
        env,
        Arc::new(SeqIds),
        Arc::new(SilentReporter::new()),
    )
    .unwrap();
    assert_eq!(app.network.id, "default");
    let row = describe(&app, &resolved)
        .into_iter()
        .find(|r| r.key == "network")
        .unwrap();
    assert!(
        row.source.ends_with("network/default.json"),
        "the adjacent file must be used, not the embedded copy: {}",
        row.source
    );
}

/// `assets/` が空（ステージ漏れ・入れ子コピー）なら埋め込みへ落ちる。
///
/// M11 の CI 事故そのもの。旧実装は空の `assets/` を選んで `E1003` になった。
#[test]
fn empty_adjacent_assets_falls_back_to_embedded() {
    let root = state_dir("empty-adjacent");
    let exe = root.join("bin");
    // 入れ子だけがある状態（`assets/assets/`）＝ CI の再現形
    write_assets(&exe.join("assets/assets"));
    let env = Arc::new(FakeEnv::new(root.join("state")).with_executable_dir(exe));

    let built = build(
        &bootstrap(),
        Arc::new(FixedClock::new("2026-11-07T05:00:00Z")),
        env,
        Arc::new(SeqIds),
        Arc::new(SilentReporter::new()),
    );
    let (app, _resolved) = built.expect("a nested assets tree must not stop the run");
    assert_eq!(app.network.id, "default");
    assert!(app.engines.get("duckduckgo").is_some());
}

/// バイナリの隣に無ければ、作業ディレクトリから上方へたどる。
#[test]
fn walks_up_from_the_working_directory() {
    let root = state_dir("walk-up");
    write_assets(&root.join("assets"));
    let nested = root.join("target/debug");
    std::fs::create_dir_all(&nested).unwrap();

    let env = Arc::new(FakeEnv::new(root.join("state")).with_current_dir(nested));
    assert_eq!(bundled_root(env.as_ref()), root.join("assets"));
}

/// 上方探索は「完全な `assets/`」だけを拾う（無関係な `assets` は無視する）。
#[test]
fn upward_search_ignores_unrelated_assets_directories() {
    let root = state_dir("walk-up-unrelated");
    // `assets/` はあるが `config/settings.json` が無い（別プロジェクトの残骸を想定）
    let decoy = root.join("decoy");
    std::fs::create_dir_all(decoy.join("assets/static")).unwrap();
    let nested = decoy.join("build/output");
    std::fs::create_dir_all(&nested).unwrap();

    let env = Arc::new(FakeEnv::new(root.join("state")).with_current_dir(nested));
    assert_eq!(
        bundled_root(env.as_ref()),
        PathBuf::new(),
        "a partial assets/ must not be taken as the bundled root"
    );
}

/// エンジン定義が 1 件も無ければ埋め込みの 3 エンジンで起動する。
#[test]
fn embedded_engines_are_loaded_when_none_are_found() {
    let root = state_dir("embedded-engines");
    let empty = root.join("engines");
    std::fs::create_dir_all(&empty).unwrap();
    let loaded =
        llmsearch::adapters::engine::loader::load_registry(&[empty]).expect("must not fail");
    let names: Vec<String> = loaded
        .registry
        .all()
        .map(|e| e.selector())
        .collect::<Vec<_>>();
    assert!(names.contains(&"duckduckgo".to_string()), "{names:?}");
    assert!(names.contains(&"brave@api".to_string()), "{names:?}");
    assert!(names.contains(&"searxng@json".to_string()), "{names:?}");
}
