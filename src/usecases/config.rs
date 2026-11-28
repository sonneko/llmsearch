//! 設定の解決（既定値・利用者設定・環境変数・引数の合成）。

use serde_json::json;
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
    /// 検索エンジンへ送る言語（`search.language` の上書き）。
    pub language: Option<String>,
    pub no_filter: bool,
    pub no_cache: bool,
    pub refresh: bool,
    pub json: bool,
    pub quiet: bool,
    pub verbose: bool,
    pub overrides: Vec<(String, serde_json::Value)>,
}

/// 解決の結果に付随する診断と、由来の判定材料。
#[derive(Debug, Clone, Default)]
pub struct Resolved {
    pub checks: Vec<Check>,
    pub warnings: Vec<Diagnostic>,
    pub sources: Vec<PathBuf>,
    pub settings_path: Option<PathBuf>,
    /// 読み込んだ設定ファイルと内容（弱い順）。
    pub layers: Vec<(PathBuf, serde_json::Value)>,
    /// 探索したすべての候補（存在しないものを含む。弱い順）。
    pub candidates: Vec<PathBuf>,
    /// 解決に使った資源ファイル（`network` / `filters` / `quota`）。
    pub network_path: Option<PathBuf>,
    pub filters_path: Option<PathBuf>,
    pub quota_path: Option<PathBuf>,
    /// エンジン定義の探索ディレクトリ（優先順）。
    pub engine_dirs: Vec<PathBuf>,
    /// エンジン定義として実際に読み込んだファイル。
    pub engine_sources: Vec<PathBuf>,
    /// 起動時の引数（由来 `argv` の判定に使う）。
    pub argv: Bootstrap,
}

/// 同梱リソースのディレクトリ。
///
/// 探索順は仕様 `configuration.md` §1.1 / `network.md` §1.3 に従い、
/// 「バイナリの隣の `assets/`」→「作業ディレクトリから上方へたどった `assets/`」。
/// どちらも無ければ空のパスを返し、資源は埋め込み既定（`crate::assets`）で解決する。
///
/// 以前はビルドした機械のソースパス（`CARGO_MANIFEST_DIR`）を最後の砦にしていたが、
/// そのパスが存在する環境（CI のランナー）では壊れた配布物を隠してしまうため廃止した。
pub fn bundled_root(env: &dyn Env) -> PathBuf {
    if let Some(dir) = env.executable_dir() {
        let assets = dir.join("assets");
        if assets.is_dir() {
            return assets;
        }
    }
    if let Some(dir) = env.current_dir() {
        // 完全な `assets/` だけを拾う（無関係な `assets` ディレクトリを避ける）。
        for candidate in dir.ancestors() {
            let assets = candidate.join("assets");
            if assets.join("config/settings.json").is_file() {
                return assets;
            }
        }
    }
    PathBuf::new()
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
            state_dir: bootstrap.state_dir.clone(),
            overrides: bootstrap.overrides.clone(),
        },
    )?;
    let mut loaded = loaded;
    if let Some(tag) = &bootstrap.language {
        // 引数は最も強い指定なので、検証の前に設定値へ重ねる。
        if crate::core::lang::parse(tag).is_none() {
            return Err(AppError::with_detail(
                ErrorCode::E1004,
                format!("--language {tag} (expected a language tag such as `en` or `pt-BR`)"),
            ));
        }
        loaded.settings.search.language = tag.clone();
    }
    loaded.settings.ensure_valid()?;

    let mut resolved = Resolved {
        sources: loaded.sources.clone(),
        settings_path: loaded.sources.last().cloned(),
        layers: loaded.layers.clone(),
        candidates: loaded.candidates.clone(),
        argv: bootstrap.clone(),
        ..Default::default()
    };
    resolved.checks.extend(loaded.settings.validate());

    let base_dir = loaded.base_dir.clone();
    let (mut network, network_path) = cfg::resolve_network(
        env.as_ref(),
        &loaded.settings,
        &base_dir,
        &bundled,
        bootstrap.network.as_deref(),
    )?;
    // `LLMSEARCH_TIMEOUT_MS` / `LLMSEARCH_USER_AGENT` は解決後のプロファイルへ重ねる
    network_env_overrides(env.as_ref(), &mut network)?;
    if let Some(path) = &network_path {
        resolved.sources.push(path.clone());
    }
    resolved.network_path = network_path.clone();
    // NetworkProfile の検証失敗（範囲外の値・禁止ヘッダ上書き・不正な
    // `schemaVersion` 等）は起動を止めないまま継続されていた（仕様
    // configuration.md §6.1 / network.md §2.2「検証に失敗した場合は処理を開始しない」）。
    let network_checks = network.validate();
    if let Some(fail) = network_checks.iter().find(|c| c.is_fail()) {
        return Err(AppError::with_detail(
            ErrorCode::E1004,
            format!("network profile: {}", fail.text()),
        ));
    }
    resolved.checks.extend(network_checks);
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
    resolved.filters_path = filter_path.clone();
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
    resolved.quota_path = quota_path.clone();
    // QuotaPolicy の検証失敗（不正な `schemaVersion` 等）は無視されて起動・実行が
    // 継続していた（仕様 configuration.md §6.1「検証に失敗した場合は処理を開始しない」）。
    let quota_checks = crate::core::config::quota::validate(&quota);
    if let Some(fail) = quota_checks.iter().find(|c| c.is_fail()) {
        return Err(AppError::with_detail(
            ErrorCode::E1004,
            format!("quota policy: {}", fail.text()),
        ));
    }
    resolved.checks.extend(quota_checks);

    // エンジン定義
    let dirs = engine_dirs(env.as_ref(), &base_dir, &bundled, &bootstrap.engines_dirs);
    let loaded_engines = load_registry(&dirs)?;
    resolved.engine_dirs = dirs;
    resolved.engine_sources = loaded_engines.sources.clone();
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
            AppError::with_detail(ErrorCode::E4002, "no engine is available".to_string())
        })?;
    if loaded_engines.registry.get(&default_engine).is_none() {
        return Err(AppError::with_detail(
            ErrorCode::E4002,
            format!("engine '{default_engine}' was not found"),
        ));
    }

    let state_dir = cfg::state_dir_with(
        env.as_ref(),
        bootstrap.state_dir.as_deref(),
        Some(loaded.settings.state.dir.as_str()),
    )
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
    // 検証で出た警告（未知キー・廃止したキーなど）は開始前に利用者へ知らせる。
    // 仕様 configuration.md §6.2 と errors.md §4.5。
    let from_checks: Vec<Diagnostic> = resolved
        .checks
        .iter()
        .filter_map(|c| match c {
            Check::Warn(t) => Some(Diagnostic::new(WarningCode::W1002, t.clone())),
            _ => None,
        })
        .collect();
    resolved.warnings.extend(from_checks);

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

/// 環境変数 1 件の写像。
///
/// `env_overrides` と `env_keys` が同じ表を共有し、実装と表示の食い違いを作らない。
#[derive(Debug, Clone, Copy)]
pub struct EnvMapping {
    /// 環境変数名。
    pub name: &'static str,
    /// 設定のキー（`a.b.c`）。
    pub key: &'static str,
    pub kind: Kind,
    /// 真偽値を反転して設定へ写す（`LLMSEARCH_NO_CACHE` 用）。
    pub invert: bool,
}

/// 環境変数の値の種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Number,
    Bool,
    Text,
}

/// 環境変数と設定キーの対応表（仕様 §4）。並び順がそのまま優先順である
/// （後ろの指定が勝つ）。
///
/// `LLMSEARCH_CONFIG` / `LLMSEARCH_STATE_DIR` / `LLMSEARCH_ENGINE` /
/// `LLMSEARCH_NETWORK` / `LLMSEARCH_FILTERS` / `LLMSEARCH_QUOTA` /
/// `LLMSEARCH_ENGINES_DIR` は設定ファイルへ落とさず、解決の経路が別にある
/// （探索順・参照 ID・定義ディレクトリ）。
pub const ENV_MAPPINGS: &[EnvMapping] = &[
    EnvMapping {
        name: "LLMSEARCH_NO_CACHE",
        key: "cache.enabled",
        kind: Kind::Bool,
        invert: true,
    },
    EnvMapping {
        name: "LLMSEARCH_SEARCH_LIMIT",
        key: "search.limit",
        kind: Kind::Number,
        invert: false,
    },
    EnvMapping {
        name: "LLMSEARCH_SEARCH_MAX_RESULTS",
        key: "search.maxResults",
        kind: Kind::Number,
        invert: false,
    },
    EnvMapping {
        name: "LLMSEARCH_CONTEXT_MAX_CHARS",
        key: "context.maxChars",
        kind: Kind::Number,
        invert: false,
    },
    EnvMapping {
        name: "LLMSEARCH_CONTEXT_OUTLINE_DEPTH",
        key: "context.outlineDepth",
        kind: Kind::Number,
        invert: false,
    },
    EnvMapping {
        name: "LLMSEARCH_HISTORY_ENABLED",
        key: "history.enabled",
        kind: Kind::Bool,
        invert: false,
    },
    EnvMapping {
        name: "LLMSEARCH_SEARCH_LANGUAGE",
        key: "search.language",
        kind: Kind::Text,
        invert: false,
    },
    EnvMapping {
        name: "LLMSEARCH_UI_COLOR",
        key: "ui.color",
        kind: Kind::Text,
        invert: false,
    },
    // `NO_COLOR` は最後に評価して `ui.color` を `never` で上書きする
    EnvMapping {
        name: "NO_COLOR",
        key: "ui.color",
        kind: Kind::Text,
        invert: false,
    },
];

/// 設定ファイルへ落とさない環境変数（解決の経路が別）。`doctor` の表示に使う。
pub const PASSTHROUGH_ENVS: &[&str] = &[
    "LLMSEARCH_CONFIG",
    "LLMSEARCH_STATE_DIR",
    "LLMSEARCH_ENGINE",
    "LLMSEARCH_NETWORK",
    "LLMSEARCH_FILTERS",
    "LLMSEARCH_QUOTA",
    "LLMSEARCH_ENGINES_DIR",
    "LLMSEARCH_TIMEOUT_MS",
    "LLMSEARCH_USER_AGENT",
];

/// 環境変数から設定の上書きを作る（`LLMSEARCH_*`）。
///
/// 解釈できない値は捨てずに `E1004` とする（仕様 §4）。`LLMSEARCH_TIMEOUT_MS`
/// と `LLMSEARCH_USER_AGENT` はネットワークプロファイルへ、それ以外は
/// `settings.json` のキーへ写す。
pub fn env_overrides(env: &dyn Env) -> Result<Vec<(String, serde_json::Value)>, AppError> {
    let mut out: Vec<(String, serde_json::Value)> = Vec::new();
    for mapping in ENV_MAPPINGS {
        let Some(raw) = env.var(mapping.name) else {
            continue;
        };
        let value = match mapping.kind {
            Kind::Number => json!(parse_env_number(mapping.name, mapping.key, &raw)?),
            Kind::Bool => {
                let b = parse_env_bool(mapping.name, mapping.key, &raw)?;
                json!(if mapping.invert { !b } else { b })
            }
            Kind::Text => {
                if mapping.name == "NO_COLOR" {
                    if raw.is_empty() {
                        continue;
                    }
                    json!("never")
                } else {
                    json!(raw)
                }
            }
        };
        // 同じキーへの指定は後勝ち（`NO_COLOR` を最後に評価するため）
        out.retain(|(k, _)| k != mapping.key);
        out.push((mapping.key.to_string(), value));
    }
    Ok(out)
}

fn parse_env_number(name: &str, key: &str, raw: &str) -> Result<u64, AppError> {
    raw.trim().parse::<u64>().map_err(|_| {
        AppError::with_detail(
            ErrorCode::E1004,
            format!("{name} = '{raw}' ({key} must be a positive integer)"),
        )
    })
}

fn parse_env_bool(name: &str, key: &str, raw: &str) -> Result<bool, AppError> {
    match raw.trim() {
        "1" | "true" => Ok(true),
        "0" | "false" => Ok(false),
        other => Err(AppError::with_detail(
            ErrorCode::E1004,
            format!("{name} = '{other}' ({key} must be 1 / 0 / true / false)"),
        )),
    }
}

/// キーと環境変数の対応（`config explain` の由来表示用）。
pub fn env_keys() -> Vec<(&'static str, &'static str)> {
    ENV_MAPPINGS
        .iter()
        .map(|m| (m.name, m.key))
        .collect::<Vec<_>>()
}

/// 環境変数によるネットワークプロファイルの上書き（仕様 §4）。
///
/// `LLMSEARCH_TIMEOUT_MS` は `timeoutTotalMs`、`LLMSEARCH_USER_AGENT` は
/// `userAgent` へ写す。空の UA は `E1004`。
pub fn network_env_overrides(
    env: &dyn Env,
    profile: &mut crate::core::config::NetworkProfile,
) -> Result<(), AppError> {
    if let Some(raw) = env.var("LLMSEARCH_TIMEOUT_MS") {
        let value = parse_env_number("LLMSEARCH_TIMEOUT_MS", "network.timeoutTotalMs", &raw)?;
        profile.timeout_total_ms = value;
    }
    if let Some(raw) = env.var("LLMSEARCH_USER_AGENT") {
        if raw.trim().is_empty() {
            return Err(AppError::with_detail(
                ErrorCode::E1004,
                "LLMSEARCH_USER_AGENT must not be empty".to_string(),
            ));
        }
        profile.user_agent = raw;
    }
    Ok(())
}

/// `config show` の 1 行（キー・値・由来）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigRow {
    pub key: String,
    pub value: String,
    pub origin: Origin,
    /// 由来の補足（環境変数名・ファイルパスなど）。
    pub source: String,
}

/// キーの由来（`config show` / `config explain` の表示用）。
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
    /// 仕様 §7.1 の表記（`argv` / `env` / `file` / `default`）。
    pub fn kind(&self) -> &'static str {
        match self {
            Origin::Argument => "argv",
            Origin::Env(_) => "env",
            Origin::File(_) => "file",
            Origin::Default => "default",
        }
    }

    /// 1 行の説明。
    pub fn label(&self) -> String {
        match self {
            Origin::Argument => self.kind().to_string(),
            Origin::Env(name) => format!("{}: {name}", self.kind()),
            Origin::File(path) => format!("{}: {}", self.kind(), path.display()),
            Origin::Default => self.kind().to_string(),
        }
    }
}

/// 現行の設定を表示用にまとめる（`config show`。仕様 §7.1）。
///
/// すべてのキーを 1 か所で組み立て、値と由来を対にして返す。
pub fn describe(app: &App, resolved: &Resolved) -> Vec<ConfigRow> {
    let s = &app.settings;
    let rows: Vec<(&str, String)> = vec![
        ("search.engine", app.default_engine.clone()),
        ("search.limit", s.search.limit.to_string()),
        ("search.maxResults", s.search.max_results.to_string()),
        (
            "search.fallback",
            if s.search.fallback.is_empty() {
                "(none)".to_string()
            } else {
                s.search.fallback.join(",")
            },
        ),
        ("search.merge", s.search.merge.to_string()),
        (
            "search.engines",
            if s.search.engines.is_empty() {
                "(none)".to_string()
            } else {
                s.search.engines.join(",")
            },
        ),
        ("search.snippetChars", s.search.snippet_chars.to_string()),
        ("search.titleChars", s.search.title_chars.to_string()),
        ("search.language", s.search.language.clone()),
        ("context.maxChars", s.context.max_chars.to_string()),
        ("context.outlineDepth", s.context.outline_depth.to_string()),
        (
            "context.includeChildren",
            s.context.include_children.to_string(),
        ),
        (
            "context.minSectionChars",
            s.context.min_section_chars.to_string(),
        ),
        ("cache.enabled", s.cache.enabled.to_string()),
        ("cache.ttlHours", s.cache.ttl_hours.to_string()),
        ("cache.maxSizeMb", s.cache.max_size_mb.to_string()),
        ("cache.searchResults", s.cache.search_results.to_string()),
        ("history.enabled", s.history.enabled.to_string()),
        (
            "history.retentionDays",
            s.history.retention_days.to_string(),
        ),
        ("history.maxEntries", s.history.max_entries.to_string()),
        ("session.ttlDays", s.session.ttl_days.to_string()),
        ("session.maxSessions", s.session.max_sessions.to_string()),
        ("ui.color", effective_color(&s.ui.color, app.env.as_ref())),
        ("ui.pageSize", s.ui.page_size.to_string()),
        ("state.dir", app.paths.root.display().to_string()),
        ("network", app.network.id.clone()),
        ("filters", app.filters.set.id.clone()),
        ("quota", app.quota.id.clone()),
        (
            "engines.dir",
            resolved
                .engine_dirs
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(","),
        ),
    ];
    rows.into_iter()
        .map(|(key, value)| {
            let (origin, source) = origin_of(app, resolved, key);
            ConfigRow {
                key: key.to_string(),
                value,
                origin,
                source,
            }
        })
        .collect()
}

/// `NO_COLOR` を反映した実効の色設定。
fn effective_color(configured: &str, env: &dyn Env) -> String {
    if env.var("NO_COLOR").is_some_and(|v| !v.is_empty()) {
        "never".to_string()
    } else {
        configured.to_string()
    }
}

/// キーの由来を決める（引数 → 環境変数 → ファイル → 既定）。
fn origin_of(app: &App, resolved: &Resolved, key: &str) -> (Origin, String) {
    // 1. 引数
    let argv = &resolved.argv;
    let by_argv = match key {
        "search.engine" => argv.engine.clone(),
        "network" => argv.network.clone(),
        "filters" => argv.filters.clone(),
        "quota" => argv.quota.clone(),
        "search.language" => argv.language.clone(),
        "state.dir" => argv.state_dir.as_ref().map(|p| p.display().to_string()),
        _ => None,
    };
    if let Some(_value) = by_argv {
        return (Origin::Argument, String::new());
    }
    // 引数だが `Option<String>` では表現できないものをここで扱う。
    // `--engines-dir`（複数可）と `--no-cache`。
    if key == "engines.dir" && !argv.engines_dirs.is_empty() {
        return (Origin::Argument, String::new());
    }
    if key == "cache.enabled" && argv.no_cache {
        return (Origin::Argument, String::new());
    }

    // 2. 環境変数（同一キーへ写す複数の変数のうち、実効のものを選ぶ）
    let env = app.env.as_ref();
    if key == "ui.color" && env.var("NO_COLOR").is_some_and(|v| !v.is_empty()) {
        return (Origin::Env("NO_COLOR".to_string()), "NO_COLOR".to_string());
    }
    // 表の末尾ほど優先なので、逆順に走査して最初に見つかったものを採る
    for mapping in ENV_MAPPINGS.iter().filter(|m| m.key == key).rev() {
        if env.var(mapping.name).is_some() {
            return (
                Origin::Env(mapping.name.to_string()),
                mapping.name.to_string(),
            );
        }
    }
    match key {
        "network" if env.var("LLMSEARCH_NETWORK").is_some() => {
            return (
                Origin::Env("LLMSEARCH_NETWORK".into()),
                "LLMSEARCH_NETWORK".into(),
            );
        }
        "filters" if env.var("LLMSEARCH_FILTERS").is_some() => {
            return (
                Origin::Env("LLMSEARCH_FILTERS".into()),
                "LLMSEARCH_FILTERS".into(),
            );
        }
        "quota" if env.var("LLMSEARCH_QUOTA").is_some() => {
            return (
                Origin::Env("LLMSEARCH_QUOTA".into()),
                "LLMSEARCH_QUOTA".into(),
            );
        }
        "search.engine" if env.var("LLMSEARCH_ENGINE").is_some() => {
            return (
                Origin::Env("LLMSEARCH_ENGINE".into()),
                "LLMSEARCH_ENGINE".into(),
            );
        }
        "state.dir" if env.var("LLMSEARCH_STATE_DIR").is_some() => {
            return (
                Origin::Env("LLMSEARCH_STATE_DIR".into()),
                "LLMSEARCH_STATE_DIR".into(),
            );
        }
        "engines.dir" if env.var("LLMSEARCH_ENGINES_DIR").is_some() => {
            return (
                Origin::Env("LLMSEARCH_ENGINES_DIR".into()),
                "LLMSEARCH_ENGINES_DIR".into(),
            );
        }
        _ => {}
    }

    // 3. 設定ファイル（弱い順に走査し、最後に指定したファイルを採る）
    if let Some(path) = file_origin(resolved, key) {
        return (Origin::File(path.clone()), path.display().to_string());
    }
    if let Some(path) = resource_origin(resolved, key) {
        return (Origin::File(path.clone()), path.display().to_string());
    }
    // 資源が埋め込み既定から解決された場合は、その旨を隠さない（M11）。
    if is_resource_key(key) {
        return (Origin::Default, crate::assets::EMBEDDED_SOURCE.to_string());
    }
    (Origin::Default, String::new())
}

/// キーを指定している最も強い設定ファイル。
fn file_origin(resolved: &Resolved, key: &str) -> Option<PathBuf> {
    let mut found: Option<PathBuf> = None;
    for (path, value) in &resolved.layers {
        if json_has_key(value, key) {
            found = Some(path.clone());
        }
    }
    found
}

/// 資源のキーか（埋め込みで解決されたときの表示に使う）。
fn is_resource_key(key: &str) -> bool {
    matches!(key, "network" | "filters" | "quota" | "engines.dir")
}

/// 資源（`network` / `filters` / `quota`）の解決ファイル。
fn resource_origin(resolved: &Resolved, key: &str) -> Option<PathBuf> {
    match key {
        "network" => resolved.network_path.clone(),
        "filters" => resolved.filters_path.clone(),
        "quota" => resolved.quota_path.clone(),
        "engines.dir" => resolved.engine_sources.first().cloned(),
        _ => None,
    }
}

/// `a.b.c` 形式のキーが JSON に存在するか。
fn json_has_key(value: &serde_json::Value, key: &str) -> bool {
    let mut cursor = value;
    for part in key.split('.') {
        match cursor {
            serde_json::Value::Object(map) => match map.get(part) {
                Some(next) => cursor = next,
                None => return false,
            },
            _ => return false,
        }
    }
    true
}

/// 指定したキーの解決値と由来を返す（`config explain`。仕様 §7.5）。
pub fn explain(
    app: &App,
    resolved: &Resolved,
    _env: &dyn Env,
    key: &str,
) -> Result<(String, Origin), AppError> {
    let rows = describe(app, resolved);
    match rows.into_iter().find(|row| row.key == key) {
        Some(row) => Ok((row.value, row.origin)),
        None => {
            let candidates = near_keys(key, describe(app, resolved).iter().map(|r| r.key.clone()));
            let detail = if candidates.is_empty() {
                format!("key '{key}' cannot be explained")
            } else {
                format!(
                    "key '{key}' cannot be explained (did you mean: {})",
                    candidates.join(", ")
                )
            };
            Err(AppError::with_detail(ErrorCode::E1004, detail))
        }
    }
}

/// 編集距離の近いキーを最大 3 件返す。
fn near_keys(key: &str, keys: impl Iterator<Item = String>) -> Vec<String> {
    let mut scored: Vec<(usize, String)> = keys
        .map(|k| (edit_distance(key, &k), k))
        .filter(|(d, _)| *d <= 6)
        .collect();
    scored.sort();
    scored.into_iter().take(3).map(|(_, k)| k).collect()
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            cur.push((prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

/// `config path` の 1 行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathRow {
    pub label: String,
    pub path: PathBuf,
    /// 実際に読み込んだ（使用する）パスか。`false` は存在しない候補。
    pub used: bool,
}

/// 設定・資源・エンジン・状態の探索先を 1 か所にまとめる（仕様 §7.2）。
///
/// 存在しない候補も `used=false` として含める。
pub fn paths(app: &App, resolved: &Resolved) -> Vec<PathRow> {
    let mut rows = Vec::new();
    let mut push = |label: &str, path: PathBuf, used: bool| {
        rows.push(PathRow {
            label: label.to_string(),
            path,
            used,
        });
    };

    // settings.json の探索候補（弱い順）。資源ディレクトリも候補として並べる。
    for candidate in &resolved.candidates {
        let used = resolved.layers.iter().any(|(p, _)| p == candidate);
        push("settings", candidate.clone(), used);
    }
    let base = if resolved.argv.config_path.is_some() {
        resolved.argv.config_path.clone()
    } else {
        resolved.settings_path.clone()
    };
    let base_dir = base
        .as_ref()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    let state_root = app.paths.root.clone();
    let resource_dirs = |subdir: &str| {
        let mut dirs = vec![base_dir.join(subdir)];
        if !dirs.contains(&state_root.join(subdir)) {
            dirs.push(state_root.join(subdir));
        }
        if let Some(config) = app.env.config_home() {
            let dir = config.join("llmsearch").join(subdir);
            if !dirs.contains(&dir) {
                dirs.push(dir);
            }
        }
        dirs.push(app.bundled.join(subdir));
        dirs
    };
    for (label, subdir, used_path) in [
        ("network", "network", resolved.network_path.clone()),
        ("filters", "filters", resolved.filters_path.clone()),
        ("quotas", "quotas", resolved.quota_path.clone()),
    ] {
        for dir in resource_dirs(subdir) {
            let used = used_path
                .as_ref()
                .and_then(|p| p.parent().map(Path::to_path_buf))
                .is_some_and(|parent| parent == dir);
            let path = match &used_path {
                Some(file) if file.parent().is_some_and(|p| p == dir) => file.clone(),
                _ => dir,
            };
            push(label, path, used);
        }
    }

    // エンジン定義ディレクトリ
    for dir in &resolved.engine_dirs {
        let used = resolved
            .engine_sources
            .iter()
            .any(|p| p.parent() == Some(dir));
        push("engines", dir.clone(), used);
    }

    // 状態ディレクトリと配下の実ファイル
    push("state", state_root.clone(), true);
    for (label, path) in [
        ("history", app.paths.history_file()),
        ("counters", app.paths.counters_file()),
        ("sessions", app.paths.sessions()),
        ("documents", app.paths.documents()),
        ("robots", app.paths.robots_file()),
    ] {
        push(label, path, true);
    }
    rows
}

/// `config init` の結果。
#[derive(Debug, Clone, Default)]
pub struct InitReport {
    /// 書き出したパス。
    pub written: Vec<PathBuf>,
    /// 既存のため書き出さなかったパス。
    pub skipped: Vec<PathBuf>,
}

/// 4 ファイルの雛形の内容（同梱アセット）。
pub const TEMPLATE_SETTINGS: &str = include_str!("../../assets/config/settings.json");
pub const TEMPLATE_NETWORK: &str = include_str!("../../assets/network/default.json");
pub const TEMPLATE_FILTERS: &str = include_str!("../../assets/filters/default.json");
pub const TEMPLATE_QUOTAS: &str = include_str!("../../assets/quotas/default.json");

/// 雛形を書き出す（`config init`。仕様 §7.3）。
///
/// 既存ファイルは `--force` が無ければ飛ばし、残りのファイルは書き出す。
pub fn init(state_dir: &Path, force: bool) -> Result<InitReport, AppError> {
    let targets = [
        (state_dir.join("settings.json"), TEMPLATE_SETTINGS),
        (state_dir.join("network/default.json"), TEMPLATE_NETWORK),
        (state_dir.join("filters/default.json"), TEMPLATE_FILTERS),
        (state_dir.join("quotas/default.json"), TEMPLATE_QUOTAS),
    ];
    let mut report = InitReport::default();
    for (path, body) in targets {
        if path.exists() && !force {
            report.skipped.push(path);
            continue;
        }
        crate::adapters::state::write_atomic(&path, body.as_bytes())?;
        report.written.push(path);
    }
    Ok(report)
}

/// 環境変数の警告（未知キー等）。
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
        map.insert("LLMSEARCH_SEARCH_LANGUAGE".to_string(), "de".to_string());
        map.insert("LLMSEARCH_NO_CACHE".to_string(), "1".to_string());
        let overrides = env_overrides(&FakeEnv(map)).unwrap();
        assert_eq!(overrides.len(), 3);
        assert!(
            overrides
                .iter()
                .any(|(k, v)| k == "search.limit" && v == &serde_json::json!(5))
        );
        assert!(
            overrides
                .iter()
                .any(|(k, v)| k == "cache.enabled" && v == &serde_json::json!(false)),
            "LLMSEARCH_NO_CACHE=1 maps to cache.enabled=false"
        );
        assert!(
            overrides
                .iter()
                .any(|(k, v)| k == "search.language" && v == &serde_json::json!("de")),
            "LLMSEARCH_SEARCH_LANGUAGE maps to search.language"
        );
    }

    #[test]
    fn env_overrides_rejects_bad_values() {
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
        map.insert("LLMSEARCH_SEARCH_LIMIT".to_string(), "many".to_string());
        let err = env_overrides(&FakeEnv(map)).unwrap_err();
        assert_eq!(err.code, ErrorCode::E1004);
    }

    #[test]
    fn no_color_wins_over_ui_color() {
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
        map.insert("LLMSEARCH_UI_COLOR".to_string(), "always".to_string());
        map.insert("NO_COLOR".to_string(), "1".to_string());
        let overrides = env_overrides(&FakeEnv(map)).unwrap();
        let color: Vec<_> = overrides.iter().filter(|(k, _)| k == "ui.color").collect();
        assert_eq!(color.len(), 1, "the same key must collapse to one entry");
        assert_eq!(color[0].1, serde_json::json!("never"));
    }
}
