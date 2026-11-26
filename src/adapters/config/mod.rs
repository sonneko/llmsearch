//! 設定ファイルの探索・読み込み・マージ。

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::core::config::{
    FilterSet, NetworkProfile, QuotaPolicy, Settings, merge_json, resolve_refs,
};
use crate::core::errors::{AppError, ErrorCode};
use crate::ports::Env;

/// 設定探索の結果。
#[derive(Debug, Clone)]
pub struct LoadedSettings {
    pub settings: Settings,
    /// 読み込んだファイル（後ろほど優先）。
    pub sources: Vec<PathBuf>,
    /// 読み込んだファイルと、その内容（`sources` と同じ順）。
    ///
    /// 由来（`file`）の判定に使う。マージ後の型からはどのファイルが
    /// キーを指定したかを復元できないため、生の JSON を保持する。
    pub layers: Vec<(PathBuf, Value)>,
    /// 設定ディレクトリ（`network/` などの兄弟）。
    pub base_dir: PathBuf,
    /// 探索したすべての候補（存在しないものを含む。弱い順）。
    pub candidates: Vec<PathBuf>,
}

/// 読み込みのオプション。
#[derive(Debug, Clone, Default)]
pub struct LoadOptions {
    pub config_path: Option<PathBuf>,
    /// `--state-dir` が指す `settings.json` の探索先。
    pub state_dir: Option<PathBuf>,
    /// `--no-color` / `--quiet` 等の CLI 上書き。
    pub overrides: Vec<(String, Value)>,
}

/// 既定の探索順で設定を読み込む。
pub fn load(env: &dyn Env, options: &LoadOptions) -> Result<LoadedSettings, AppError> {
    let candidates = candidate_paths(env, options)?;
    let mut merged = serde_json::to_value(Settings::default())
        .map_err(|e| AppError::with_detail(ErrorCode::E1003, format!("defaults: {e}")))?;
    let mut sources = Vec::new();
    let mut layers = Vec::new();
    let mut base_dir = PathBuf::from(".");

    for path in &candidates {
        if !path.is_file() {
            continue;
        }
        let mut value = read_json(path)?;
        resolve_refs(
            &mut value,
            path.parent().unwrap_or(Path::new(".")),
            &mut std::collections::HashSet::new(),
            &read_json,
        )?;
        merge_json(&mut merged, value.clone());
        sources.push(path.clone());
        layers.push((path.clone(), value));
        base_dir = path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
    }

    for (key, value) in &options.overrides {
        apply_override(&mut merged, key, value.clone())?;
    }

    let settings: Settings = serde_json::from_value(merged).map_err(|e| {
        AppError::with_detail(
            ErrorCode::E1004,
            format!("failed to interpret the configuration: {e}"),
        )
    })?;
    Ok(LoadedSettings {
        settings,
        sources,
        layers,
        base_dir,
        candidates,
    })
}

/// 設定の探索候補（弱い順。後ろほど優先）。
///
/// `--config` と `LLMSEARCH_CONFIG` は「そのファイルだけを使う」指定であり、
/// 指定先が無い場合は既定値へ落とさず `E1003` を返す（仕様 §2.3）。
fn candidate_paths(env: &dyn Env, options: &LoadOptions) -> Result<Vec<PathBuf>, AppError> {
    if let Some(path) = &options.config_path {
        if !path.is_file() {
            return Err(AppError::with_detail(
                ErrorCode::E1003,
                format!(
                    "--config points at a file that does not exist: {}",
                    path.display()
                ),
            ));
        }
        return Ok(vec![path.clone()]);
    }
    if let Some(raw) = env.var("LLMSEARCH_CONFIG") {
        let path = PathBuf::from(&raw);
        if !path.is_file() {
            return Err(AppError::with_detail(
                ErrorCode::E1003,
                format!("LLMSEARCH_CONFIG points at a file that does not exist: {raw}"),
            ));
        }
        return Ok(vec![path]);
    }
    let user_settings = env
        .config_home()
        .map(|dir| dir.join("llmsearch/settings.json"));

    // state.dir は利用者設定からも指定できる。明示 CLI / 環境変数が
    // 無いときに限り、状態設定ファイルの候補を先に導出する。
    let configured_state =
        if options.state_dir.is_none() && env.var("LLMSEARCH_STATE_DIR").is_none() {
            match user_settings.as_ref().filter(|path| path.is_file()) {
                Some(path) => read_json(path)?
                    .get("state")
                    .and_then(|state| state.get("dir"))
                    .and_then(Value::as_str)
                    .filter(|dir| !dir.trim().is_empty())
                    .map(PathBuf::from),
                None => None,
            }
        } else {
            None
        };

    let state_candidate = options
        .state_dir
        .clone()
        .or_else(|| env.var("LLMSEARCH_STATE_DIR").map(PathBuf::from))
        .or(configured_state)
        .or_else(|| env.state_home().map(|dir| dir.join("llmsearch")));

    let mut out = Vec::new();
    // 強い順は「状態ディレクトリ → 利用者設定ディレクトリ」なので、
    // 後勝ちで畳み込むこの一覧では利用者設定を先に置く。
    if let Some(path) = user_settings {
        out.push(path);
    }
    if let Some(dir) = state_candidate {
        out.push(dir.join("settings.json"));
    }
    Ok(out)
}

/// 状態ディレクトリを解決する。
///
/// 優先順は `--state-dir` → `LLMSEARCH_STATE_DIR` → 設定 `state.dir` →
/// `state.home`。`settings` は設定 `state.dir` を評価するときにだけ渡す
/// （設定の読み込み自体が状態ディレクトリを必要とするため、ここでは引数で受ける）。
pub fn state_dir(env: &dyn Env, explicit: Option<&Path>) -> Option<PathBuf> {
    state_dir_with(env, explicit, None)
}

/// `state.dir` を加味した状態ディレクトリの解決。
pub fn state_dir_with(
    env: &dyn Env,
    explicit: Option<&Path>,
    configured: Option<&str>,
) -> Option<PathBuf> {
    if let Some(dir) = explicit {
        return Some(dir.to_path_buf());
    }
    if let Some(dir) = env.var("LLMSEARCH_STATE_DIR") {
        return Some(PathBuf::from(dir));
    }
    if let Some(dir) = configured.filter(|d| !d.trim().is_empty()) {
        return Some(PathBuf::from(dir));
    }
    env.state_home().map(|d| d.join("llmsearch"))
}

fn read_json(path: &Path) -> Result<Value, AppError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| AppError::with_detail(ErrorCode::E1003, format!("{}: {e}", path.display())))?;
    serde_json::from_str(&text).map_err(|e| {
        AppError::with_detail(
            ErrorCode::E1003,
            format!(
                "{}: line {} column {}: {e}",
                path.display(),
                e.line(),
                e.column()
            ),
        )
    })
}

/// `a.b.c` 形式の上書きを適用する。
fn apply_override(root: &mut Value, key: &str, value: Value) -> Result<(), AppError> {
    let parts: Vec<&str> = key.split('.').collect();
    if parts.is_empty() {
        return Err(AppError::with_detail(ErrorCode::E1004, key.to_string()));
    }
    let mut cursor = root;
    for (i, part) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            if let Value::Object(map) = cursor {
                map.insert((*part).to_string(), value.clone());
            }
            return Ok(());
        }
        if !cursor.is_object() {
            return Err(AppError::with_detail(ErrorCode::E1004, key.to_string()));
        }
        let next = cursor
            .as_object_mut()
            .and_then(|m| m.get_mut(*part))
            .ok_or_else(|| AppError::with_detail(ErrorCode::E1004, key.to_string()))?;
        if next.is_null() {
            return Err(AppError::with_detail(ErrorCode::E1004, key.to_string()));
        }
        cursor = next;
    }
    Ok(())
}

/// 種別ごとのファイル（`network/<id>.json` など）を探索する。
pub fn resolve_resource_dir(
    env: &dyn Env,
    base_dir: &Path,
    subdir: &str,
    bundled: &Path,
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    out.push(base_dir.join(subdir));
    if let Some(state) = state_dir(env, None) {
        out.push(state.join(subdir));
    }
    if let Some(config) = env.config_home() {
        out.push(config.join("llmsearch").join(subdir));
    }
    out.push(bundled.join(subdir));
    out
}

/// ディレクトリから `<id>.json` を探す。
pub fn find_resource(dirs: &[PathBuf], id: &str) -> Option<PathBuf> {
    for dir in dirs {
        let path = dir.join(format!("{id}.json"));
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

/// JSON を読む（公開版）。
pub fn read_json_file(path: &Path) -> Result<Value, AppError> {
    read_json(path)
}

/// `network` の設定値を解決する。
pub fn resolve_network(
    env: &dyn Env,
    settings: &Settings,
    base_dir: &Path,
    bundled: &Path,
    cli: Option<&str>,
) -> Result<(NetworkProfile, Option<PathBuf>), AppError> {
    let id = cli
        .map(|s| s.to_string())
        .or_else(|| env.var("LLMSEARCH_NETWORK"))
        .or_else(|| match &settings.network {
            crate::core::config::RefOr::Id(id) => Some(id.clone()),
            crate::core::config::RefOr::Inline(_) => None,
        });
    if let (None, crate::core::config::RefOr::Inline(profile)) = (&id, &settings.network) {
        return Ok((profile.clone(), None));
    }
    let id = id.unwrap_or_else(|| "default".to_string());
    let dirs = resolve_resource_dir(env, base_dir, "network", bundled);
    match find_resource(&dirs, &id) {
        Some(path) => {
            let value = read_json_file(&path)?;
            let profile: NetworkProfile = serde_json::from_value(value).map_err(|e| {
                AppError::with_detail(ErrorCode::E1004, format!("{}: {e}", path.display()))
            })?;
            Ok((profile, Some(path)))
        }
        None => match crate::assets::embedded_as::<NetworkProfile>(
            crate::assets::Resource::Network,
            &id,
            "network profile",
        )? {
            Some(profile) => Ok((profile, None)),
            None => Err(AppError::with_detail(
                ErrorCode::E1003,
                format!("network profile '{id}' was not found"),
            )),
        },
    }
}

/// `filters` の設定値を解決する。
pub fn resolve_filters(
    env: &dyn Env,
    settings: &Settings,
    base_dir: &Path,
    bundled: &Path,
    cli: Option<&str>,
) -> Result<(FilterSet, Option<PathBuf>), AppError> {
    let id = cli
        .map(|s| s.to_string())
        .or_else(|| env.var("LLMSEARCH_FILTERS"))
        .or_else(|| match &settings.filters {
            crate::core::config::RefOr::Id(id) => Some(id.clone()),
            crate::core::config::RefOr::Inline(_) => None,
        });
    if let (None, crate::core::config::RefOr::Inline(set)) = (&id, &settings.filters) {
        return Ok((set.clone(), None));
    }
    let id = id.unwrap_or_else(|| "default".to_string());
    let dirs = resolve_resource_dir(env, base_dir, "filters", bundled);
    match find_resource(&dirs, &id) {
        Some(path) => {
            let value = read_json_file(&path)?;
            let set: FilterSet = serde_json::from_value(value).map_err(|e| {
                AppError::with_detail(ErrorCode::E1004, format!("{}: {e}", path.display()))
            })?;
            Ok((set, Some(path)))
        }
        None => match crate::assets::embedded_as::<FilterSet>(
            crate::assets::Resource::Filters,
            &id,
            "filter set",
        )? {
            Some(set) => Ok((set, None)),
            None => Err(AppError::with_detail(
                ErrorCode::E1003,
                format!("filter set '{id}' was not found"),
            )),
        },
    }
}

/// `quota` の設定値を解決する。
pub fn resolve_quota(
    env: &dyn Env,
    settings: &Settings,
    base_dir: &Path,
    bundled: &Path,
    cli: Option<&str>,
) -> Result<(QuotaPolicy, Option<PathBuf>), AppError> {
    let id = cli
        .map(|s| s.to_string())
        .or_else(|| env.var("LLMSEARCH_QUOTA"))
        .or_else(|| match &settings.quota {
            crate::core::config::RefOr::Id(id) => Some(id.clone()),
            crate::core::config::RefOr::Inline(_) => None,
        });
    if let (None, crate::core::config::RefOr::Inline(policy)) = (&id, &settings.quota) {
        return Ok((policy.clone(), None));
    }
    let id = id.unwrap_or_else(|| "default".to_string());
    let dirs = resolve_resource_dir(env, base_dir, "quotas", bundled);
    match find_resource(&dirs, &id) {
        Some(path) => {
            let value = read_json_file(&path)?;
            let policy: QuotaPolicy = serde_json::from_value(value).map_err(|e| {
                AppError::with_detail(ErrorCode::E1004, format!("{}: {e}", path.display()))
            })?;
            Ok((policy, Some(path)))
        }
        None => match crate::assets::embedded_as::<QuotaPolicy>(
            crate::assets::Resource::Quota,
            &id,
            "quota policy",
        )? {
            Some(policy) => Ok((policy, None)),
            None => Err(AppError::with_detail(
                ErrorCode::E1003,
                format!("quota policy '{id}' was not found"),
            )),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::system::SystemEnv;

    #[test]
    fn override_merges_into_existing_key() {
        let mut v = serde_json::json!({"search": {"limit": 7}});
        apply_override(&mut v, "search.limit", serde_json::json!(5)).unwrap();
        assert_eq!(v["search"]["limit"], 5);
        assert!(apply_override(&mut v, "nope.x", serde_json::json!(1)).is_err());
    }

    #[test]
    fn state_dir_prefers_env_var() {
        let env = SystemEnv::new();
        let explicit = state_dir(&env, Some(Path::new("/tmp/x")));
        assert_eq!(explicit, Some(PathBuf::from("/tmp/x")));
    }

    #[test]
    fn resource_dirs_include_bundled_last() {
        let env = SystemEnv::new();
        let dirs = resolve_resource_dir(&env, Path::new("/b"), "network", Path::new("/w"));
        assert_eq!(dirs.last().unwrap(), &Path::new("/w/network"));
        assert_eq!(dirs.first().unwrap(), &Path::new("/b/network"));
    }
}
