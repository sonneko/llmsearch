//! 設定の解決（既定値・利用者設定・環境変数・引数の合成）。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::adapters::config::{self as cfg, LoadOptions};
use crate::adapters::engine::loader::{engine_dirs, load_registry};
use crate::adapters::state::StatePaths;
use crate::adapters::state::history_jsonl::JsonlHistory;
use crate::adapters::state::session_store::{FsCounters, FsDocumentStore, FsSessionStore};
use crate::core::config::filter::CompiledFilters;
use crate::core::errors::{AppError, Check, Diagnostic, ErrorCode, WarningCode};
use crate::core::secret::Secrets;
use crate::ports::system::{SystemClock, SystemEnv, SystemIdGen};
use crate::ports::{Clock, Env, IdGen, Reporter};
use crate::usecases::App;

/// 起動時のオプション（CLI から受け取る）。
#[derive(Debug, Clone, Default)]
pub struct Bootstrap {
    pub config_path: Option<PathBuf>,
    pub state_dir: Option<PathBuf>,
    pub engines_dirs: Vec<PathBuf>,
    pub network: Option<String>,
    pub filters: Option<String>,
    pub quota: Option<String>,
    pub engine: Option<String>,
    pub no_filter: bool,
    pub no_cache: bool,
    pub refresh: bool,
    pub json: bool,
    pub quiet: bool,
    pub verbose: bool,
    pub overrides: Vec<(String, serde_json::Value)>,
}

/// 解決の結果に付随する診断。
#[derive(Debug, Clone, Default)]
pub struct Resolved {
    pub checks: Vec<Check>,
    pub warnings: Vec<Diagnostic>,
    pub sources: Vec<PathBuf>,
    pub settings_path: Option<PathBuf>,
}

/// 同梱リソースのディレクトリ。
pub fn bundled_root(env: &dyn Env) -> PathBuf {
    if let Some(dir) = env.executable_dir() {
        let assets = dir.join("assets");
        if assets.is_dir() {
            return assets;
        }
    }
    // ソースからの開発実行では従来の配置を使う。
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets")
}

/// 依存を組み立ててアプリを作る。
pub fn build(
    bootstrap: &Bootstrap,
    clock: Arc<dyn Clock>,
    env: Arc<dyn Env>,
    ids: Arc<dyn IdGen>,
    reporter: Arc<dyn Reporter>,
) -> Result<(App, Resolved), AppError> {
    let bundled = bundled_root(env.as_ref());
    let loaded = cfg::load(
        env.as_ref(),
        &LoadOptions {
            config_path: bootstrap.config_path.clone(),
            overrides: bootstrap.overrides.clone(),
        },
    )?;
    loaded.settings.ensure_valid()?;

    let mut resolved = Resolved {
        sources: loaded.sources.clone(),
        settings_path: loaded.sources.last().cloned(),
        ..Default::default()
    };
    resolved.checks.extend(loaded.settings.validate());

    let base_dir = loaded.base_dir.clone();
    let (network, network_path) = cfg::resolve_network(
        env.as_ref(),
        &loaded.settings,
        &base_dir,
        &bundled,
        bootstrap.network.as_deref(),
    )?;
    if let Some(path) = &network_path {
        resolved.sources.push(path.clone());
    }
    resolved.checks.extend(network.validate());
    for w in crate::adapters::http::profile_warnings(&network) {
        resolved.warnings.push(w);
    }

    let (filter_set, filter_path) = cfg::resolve_filters(
        env.as_ref(),
        &loaded.settings,
        &base_dir,
        &bundled,
        bootstrap.filters.as_deref(),
    )?;
    if let Some(path) = &filter_path {
        resolved.sources.push(path.clone());
    }
    let filters = CompiledFilters::compile(filter_set)?;

    let (quota, quota_path) = cfg::resolve_quota(
        env.as_ref(),
        &loaded.settings,
        &base_dir,
        &bundled,
        bootstrap.quota.as_deref(),
    )?;
    if let Some(path) = &quota_path {
        resolved.sources.push(path.clone());
    }
    resolved
        .checks
        .extend(crate::core::config::quota::validate(&quota));

    // エンジン定義
    let mut dirs = bootstrap.engines_dirs.clone();
    dirs.extend(engine_dirs(env.as_ref(), &base_dir, &bundled, &[]));
    let loaded_engines = load_registry(&dirs)?;
    resolved.warnings.extend(loaded_engines.warnings.clone());
    for engine in loaded_engines.registry.all() {
        resolved
            .checks
            .extend(engine.validate().into_iter().map(|c| match c {
                Check::Fail(t) => Check::Fail(format!("{}: {t}", engine.selector())),
                Check::Warn(t) => Check::Warn(format!("{}: {t}", engine.selector())),
                Check::Ok(t) => Check::Ok(t),
            }));
    }

    let default_engine = bootstrap
        .engine
        .clone()
        .or_else(|| env.var("LLMSEARCH_ENGINE"))
        .or_else(|| loaded.settings.search.engine.clone())
        .or_else(|| {
            // 認証情報が不要なエンジンを優先し、その中では定義順の先頭を選ぶ
            loaded_engines
                .registry
                .enabled()
                .find(|e| !e.limits.requires_api_key)
                .or_else(|| loaded_engines.registry.enabled().next())
                .map(|e| e.selector())
        })
        .ok_or_else(|| {
            AppError::with_detail(ErrorCode::E4002, "利用可能なエンジンが無い".to_string())
        })?;
    if loaded_engines.registry.get(&default_engine).is_none() {
        return Err(AppError::with_detail(
            ErrorCode::E4002,
            format!("エンジン '{default_engine}' が見つからない"),
        ));
    }

    let state_dir = cfg::state_dir(env.as_ref(), bootstrap.state_dir.as_deref())
        .unwrap_or_else(|| PathBuf::from(".llmsearch"));
    let paths = StatePaths::new(state_dir);
    paths.ensure()?;

    let secrets = collect_secrets(env.as_ref(), &loaded_engines.registry, &network);

    let app = App {
        settings: loaded.settings.clone(),
        network,
        filters,
        quota,
        secrets,
        paths: paths.clone(),
        bundled,
        clock: Arc::clone(&clock),
        env: Arc::clone(&env),
        ids,
        reporter: Arc::clone(&reporter),
        http: Arc::new(
            crate::adapters::http::ReqwestClient::new(Arc::clone(&clock))
                .with_reporter(Arc::clone(&reporter)),
        ),
        sessions: Arc::new(FsSessionStore::new(paths.clone(), Arc::clone(&clock))),
        documents: Arc::new(
            FsDocumentStore::new(paths.clone()).with_max_bytes(
                loaded
                    .settings
                    .cache
                    .max_size_mb
                    .saturating_mul(1024 * 1024),
            ),
        ),
        history: Arc::new(
            JsonlHistory::new(paths.clone())
                .with_limits(
                    loaded.settings.history.retention_days,
                    loaded.settings.history.max_entries,
                )
                .with_clock(Arc::clone(&clock)),
        ),
        counters: Arc::new(FsCounters::new(paths.clone(), Arc::clone(&clock))),
        engines: loaded_engines.registry,
        no_filter: bootstrap.no_filter,
        no_cache: bootstrap.no_cache,
        refresh: bootstrap.refresh,
        json: bootstrap.json,
        quiet: bootstrap.quiet,
        verbose: bootstrap.verbose,
        tracking: default_tracking(),
        default_engine,
        session_id: std::sync::Mutex::new(String::new()),
    };
    Ok((app, resolved))
}

/// 既定の追跡パラメータ。
pub fn default_tracking() -> Vec<String> {
    crate::core::url::DEFAULT_TRACKING
        .iter()
        .map(|s| s.to_string())
        .collect()
}

fn collect_secrets(
    env: &dyn Env,
    registry: &crate::core::engine::EngineRegistry,
    network: &crate::core::config::NetworkProfile,
) -> Secrets {
    let mut secrets = Secrets::new();
    for name in referenced_envs(registry, network) {
        if let Some(value) = env.var(&name) {
            secrets.insert(name, value);
        }
    }
    secrets
}

/// 定義が参照している環境変数名を集める。
pub fn referenced_envs(
    registry: &crate::core::engine::EngineRegistry,
    network: &crate::core::config::NetworkProfile,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for engine in registry.all() {
        collect_tmpl_envs(&engine.request.query, &mut out);
        collect_tmpl_envs(&engine.request.headers, &mut out);
        for value in engine
            .request
            .query
            .values()
            .chain(engine.request.headers.values())
        {
            let _ = value;
        }
    }
    if let Some(crate::core::config::network::ProxySpec::Detailed(d)) = &network.proxy
        && let Some(crate::core::config::network::ProxyAuth::Env { value_env }) = &d.auth
    {
        out.push(value_env.clone());
    }
    out.sort();
    out.dedup();
    out
}

fn collect_tmpl_envs(
    map: &std::collections::BTreeMap<String, crate::core::engine::TmplValue>,
    out: &mut Vec<String>,
) {
    for value in map.values() {
        if let crate::core::engine::TmplValue::Env { value_env } = value {
            out.push(value_env.clone());
        }
    }
}

/// 環境変数から設定の上書きを作る（`LLMSEARCH_*`）。
pub fn env_overrides(env: &dyn Env) -> Vec<(String, serde_json::Value)> {
    let mut out = Vec::new();
    let mapping: &[(&str, &str, Kind)] = &[
        ("LLMSEARCH_SEARCH_LIMIT", "search.limit", Kind::Number),
        (
            "LLMSEARCH_SEARCH_MAX_RESULTS",
            "search.maxResults",
            Kind::Number,
        ),
        (
            "LLMSEARCH_CONTEXT_MAX_CHARS",
            "context.maxChars",
            Kind::Number,
        ),
        (
            "LLMSEARCH_CONTEXT_OUTLINE_DEPTH",
            "context.outlineDepth",
            Kind::Number,
        ),
        ("LLMSEARCH_CACHE_ENABLED", "cache.enabled", Kind::Bool),
        ("LLMSEARCH_HISTORY_ENABLED", "history.enabled", Kind::Bool),
        ("LLMSEARCH_UI_LOCALE", "ui.locale", Kind::Text),
        ("LLMSEARCH_UI_COLOR", "ui.color", Kind::Text),
    ];
    for (name, key, kind) in mapping {
        let Some(raw) = env.var(name) else { continue };
        let value = match kind {
            Kind::Number => match raw.parse::<u64>() {
                Ok(n) => serde_json::json!(n),
                Err(_) => continue,
            },
            Kind::Bool => match raw.as_str() {
                "1" | "true" => serde_json::json!(true),
                "0" | "false" => serde_json::json!(false),
                _ => continue,
            },
            Kind::Text => serde_json::json!(raw),
        };
        out.push(((*key).to_string(), value));
    }
    out
}

enum Kind {
    Number,
    Bool,
    Text,
}

/// 現行の設定を表示用にまとめる。
pub fn describe(app: &App) -> Vec<(String, String)> {
    vec![
        ("search.engine".into(), app.default_engine.clone()),
        ("search.limit".into(), app.settings.search.limit.to_string()),
        (
            "context.maxChars".into(),
            app.settings.context.max_chars.to_string(),
        ),
        ("network".into(), app.network.id.clone()),
        ("filters".into(), app.filters.set.id.clone()),
        ("quota".into(), app.quota.id.clone()),
        ("state.dir".into(), app.paths.root.display().to_string()),
        ("ui.locale".into(), app.settings.ui.locale.clone()),
    ]
}

/// キーの由来（`config explain` の表示用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// 引数（コマンドライン）で指定された。
    Argument,
    /// 環境変数で指定された。
    Env(String),
    /// 設定ファイルで指定された。
    File(PathBuf),
    /// 同梱の既定値。
    Default,
}

impl Origin {
    /// 1 行の説明。
    pub fn label(&self) -> String {
        match self {
            Origin::Argument => "引数".to_string(),
            Origin::Env(name) => format!("env: {name}"),
            Origin::File(path) => format!("file: {}", path.display()),
            Origin::Default => "同梱既定".to_string(),
        }
    }
}

/// キーと環境変数の対応（`env_overrides` と同じ表を使う）。
pub fn env_keys() -> &'static [(&'static str, &'static str)] {
    const MAPPING: &[(&str, &str)] = &[
        ("LLMSEARCH_SEARCH_LIMIT", "search.limit"),
        ("LLMSEARCH_SEARCH_MAX_RESULTS", "search.maxResults"),
        ("LLMSEARCH_CONTEXT_MAX_CHARS", "context.maxChars"),
        ("LLMSEARCH_CONTEXT_OUTLINE_DEPTH", "context.outlineDepth"),
        ("LLMSEARCH_CACHE_ENABLED", "cache.enabled"),
        ("LLMSEARCH_HISTORY_ENABLED", "history.enabled"),
        ("LLMSEARCH_UI_LOCALE", "ui.locale"),
        ("LLMSEARCH_UI_COLOR", "ui.color"),
    ];
    MAPPING
}

/// 指定したキーの解決値と由来を返す。
///
/// `resolved.sources` は読み込んだ設定ファイル（後勝ち）、`env` は環境変数の照会用。
pub fn explain(
    app: &App,
    resolved: &Resolved,
    env: &dyn Env,
    key: &str,
) -> Result<(String, Origin), AppError> {
    let Some((_, value)) = describe(app).into_iter().find(|(k, _)| k == key) else {
        return Err(AppError::with_detail(
            ErrorCode::E1004,
            format!("キー '{key}' は説明できません"),
        ));
    };
    let env_name = env_keys()
        .iter()
        .find(|(_, k)| *k == key)
        .map(|(name, _)| *name);
    let origin = if let Some(name) = env_name
        && env.var(name).is_some()
    {
        Origin::Env(name.to_string())
    } else if let Some(path) = resolved.settings_path.clone() {
        // キーの実在までは追わない（マージ後は由来が失われるため、読込元を示す）
        Origin::File(path)
    } else {
        Origin::Default
    };
    Ok((value, origin))
}

/// 環境変数の警告（未知キー等）。
pub fn unknown_key_warnings(app: &App) -> Vec<Diagnostic> {
    app.settings
        .validate()
        .into_iter()
        .filter_map(|c| match c {
            Check::Warn(t) => Some(Diagnostic::new(WarningCode::W1002, t)),
            _ => None,
        })
        .collect()
}

/// 設定の保存（`config init`）。
pub fn write_settings(
    path: &Path,
    settings: &crate::core::config::Settings,
    force: bool,
) -> Result<(), AppError> {
    if path.exists() && !force {
        return Err(AppError::with_detail(
            ErrorCode::E1004,
            format!("{} は既に存在する（--force で上書き）", path.display()),
        ));
    }
    let text = serde_json::to_vec_pretty(settings)
        .map_err(|e| AppError::with_detail(ErrorCode::E1004, format!("直列化: {e}")))?;
    crate::adapters::state::write_atomic(path, &text)
}

/// 既定の依存一式を作る（実環境向けの入口）。
pub fn default_deps() -> (Arc<dyn Clock>, Arc<dyn Env>, Arc<dyn IdGen>) {
    (
        Arc::new(SystemClock::new()),
        Arc::new(SystemEnv::new()),
        Arc::new(SystemIdGen::new()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracking_defaults_include_utm() {
        assert!(default_tracking().iter().any(|t| t == "utm_source"));
    }

    #[test]
    fn env_overrides_reads_numbers() {
        struct FakeEnv(std::collections::BTreeMap<String, String>);
        impl Env for FakeEnv {
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
                true
            }
            fn terminal_width(&self) -> usize {
                80
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
        let mut map = std::collections::BTreeMap::new();
        map.insert("LLMSEARCH_SEARCH_LIMIT".to_string(), "5".to_string());
        map.insert("LLMSEARCH_UI_LOCALE".to_string(), "en".to_string());
        map.insert("LLMSEARCH_CACHE_ENABLED".to_string(), "maybe".to_string());
        let overrides = env_overrides(&FakeEnv(map));
        assert_eq!(overrides.len(), 2);
        assert!(
            overrides
                .iter()
                .any(|(k, v)| k == "search.limit" && v == &serde_json::json!(5))
        );
    }
}
