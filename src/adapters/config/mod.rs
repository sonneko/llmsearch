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
    /// 設定ディレクトリ（`network/` などの兄弟）。
    pub base_dir: PathBuf,
}

/// 読み込みのオプション。
#[derive(Debug, Clone, Default)]
pub struct LoadOptions {
    pub config_path: Option<PathBuf>,
    /// `--no-color` / `--quiet` 等の CLI 上書き。
    pub overrides: Vec<(String, Value)>,
}

/// 既定の探索順で設定を読み込む。
pub fn load(env: &dyn Env, options: &LoadOptions) -> Result<LoadedSettings, AppError> {
    let candidates = candidate_paths(env, options);
    let mut merged = serde_json::to_value(Settings::default())
        .map_err(|e| AppError::with_detail(ErrorCode::E1003, format!("既定値: {e}")))?;
    let mut sources = Vec::new();
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
        )?;
        merge_json(&mut merged, value);
        sources.push(path.clone());
        base_dir = path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
    }

    for (key, value) in &options.overrides {
        apply_override(&mut merged, key, value.clone())?;
    }

    let settings: Settings = serde_json::from_value(merged)
        .map_err(|e| AppError::with_detail(ErrorCode::E1004, format!("設定の解釈に失敗: {e}")))?;
    Ok(LoadedSettings {
        settings,
        sources,
        base_dir,
    })
}

fn candidate_paths(env: &dyn Env, options: &LoadOptions) -> Vec<PathBuf> {
    if let Some(path) = &options.config_path {
        return vec![path.clone()];
    }
    let mut out = Vec::new();
    if let Some(dir) = env.config_home() {
        out.push(dir.join("llmsearch/settings.json"));
    }
    if let Some(dir) = state_dir(env, None) {
        out.push(dir.join("config/settings.json"));
    }
    out
}

/// 状態ディレクトリを解決する。
pub fn state_dir(env: &dyn Env, explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(dir) = explicit {
        return Some(dir.to_path_buf());
    }
    if let Some(dir) = env.var("LLMSEARCH_STATE_DIR") {
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
            format!("{}: 行 {} 桁 {}: {e}", path.display(), e.line(), e.column()),
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
        out.push(state.join("config").join(subdir));
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
        None => Err(AppError::with_detail(
            ErrorCode::E1003,
            format!("network プロファイル '{id}' が見つからない"),
        )),
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
        None => Err(AppError::with_detail(
            ErrorCode::E1003,
            format!("filters 集合 '{id}' が見つからない"),
        )),
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
        None => Err(AppError::with_detail(
            ErrorCode::E1003,
            format!("quota ポリシー '{id}' が見つからない"),
        )),
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
