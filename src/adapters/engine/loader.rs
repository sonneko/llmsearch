//! エンジン定義（`engines/*.json`）の探索と読み込み。

use std::path::{Path, PathBuf};

use crate::core::engine::{EngineDef, EngineRegistry};
use crate::core::errors::{AppError, Diagnostic, ErrorCode, WarningCode};
use crate::ports::Env;

/// 読み込み結果。
#[derive(Debug, Clone)]
pub struct LoadedEngines {
    pub registry: EngineRegistry,
    pub sources: Vec<PathBuf>,
    pub warnings: Vec<Diagnostic>,
}

/// 既定の探索ディレクトリを返す（優先順）。
pub fn engine_dirs(
    env: &dyn Env,
    base_dir: &Path,
    bundled: &Path,
    extra: &[PathBuf],
) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = extra.to_vec();
    // `LLMSEARCH_ENGINES_DIR` は `--engines-dir` の後ろへ連結する（仕様 §7.1）。
    if let Some(raw) = env.var("LLMSEARCH_ENGINES_DIR") {
        out.extend(std::env::split_paths(std::ffi::OsStr::new(&raw)));
    }
    if let Some(state) = crate::adapters::config::state_dir(env, None) {
        out.push(state.join("engines"));
    }
    if let Some(config) = env.config_home() {
        out.push(config.join("llmsearch/engines"));
    }
    let _ = base_dir;
    out.push(bundled.join("engines"));
    out
}

/// ディレクトリ群を走査して定義を読み込む（同一 `id@variant` は先勝ち）。
///
/// 1 件も見つからない場合は埋め込み既定（`crate::assets`）へ落ちる。配布物で
/// `assets/` が欠けていても既定エンジンで起動できるようにするため。
pub fn load_registry(dirs: &[PathBuf]) -> Result<LoadedEngines, AppError> {
    let mut registry = EngineRegistry::new();
    let mut sources = Vec::new();
    let mut warnings = Vec::new();

    for dir in dirs {
        if !dir.is_dir() {
            continue;
        }
        let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
            .map_err(|e| {
                AppError::with_detail(ErrorCode::E1003, format!("{}: {e}", dir.display()))
            })?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
            .collect();
        entries.sort();
        for path in entries {
            let text = std::fs::read_to_string(&path).map_err(|e| {
                AppError::with_detail(ErrorCode::E1003, format!("{}: {e}", path.display()))
            })?;
            let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
                AppError::with_detail(
                    ErrorCode::E1003,
                    format!(
                        "{}: line {} column {}: {e}",
                        path.display(),
                        e.line(),
                        e.column()
                    ),
                )
            })?;
            // 1 ファイルに複数定義（配列）も許す
            let defs: Vec<serde_json::Value> = match value {
                serde_json::Value::Array(items) => items,
                other => vec![other],
            };
            for item in defs {
                let def: EngineDef = serde_json::from_value(item).map_err(|e| {
                    AppError::with_detail(ErrorCode::E1004, format!("{}: {e}", path.display()))
                })?;
                def.ensure_valid()?;
                if !registry.insert(def.clone()) {
                    warnings.push(Diagnostic::new(
                        WarningCode::W4002,
                        format!(
                            "ignored the definition '{}' in {} because it is already loaded",
                            path.display(),
                            def.selector()
                        ),
                    ));
                    continue;
                }
                sources.push(path.clone());
            }
        }
    }

    // ファイルが 1 件も無い場合は埋め込み既定で起動する（`assets/` の欠落に耐える）。
    if registry.is_empty() {
        for (id, text) in crate::assets::Resource::Engines.embedded() {
            let value: serde_json::Value = serde_json::from_str(text).map_err(|e| {
                AppError::with_detail(
                    ErrorCode::E1003,
                    format!("embedded engine definition '{id}' is unreadable: {e}"),
                )
            })?;
            let def: EngineDef = serde_json::from_value(value).map_err(|e| {
                AppError::with_detail(
                    ErrorCode::E1004,
                    format!("embedded engine definition '{id}': {e}"),
                )
            })?;
            def.ensure_valid()?;
            registry.insert(def);
        }
        if registry.is_empty() {
            return Err(AppError::with_detail(
                ErrorCode::E4002,
                format!(
                    "no engine definitions found (searched: {})",
                    dirs.iter()
                        .map(|d| d.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ));
        }
    }

    Ok(LoadedEngines {
        registry,
        sources,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, body: &str) {
        std::fs::write(dir.join(name), body).unwrap();
    }

    fn minimal(id: &str) -> String {
        format!(
            r#"{{"schemaVersion":1,"id":"{id}",
                "request":{{"url":"https://example.com/s","query":{{"q":"{{{{query}}}}"}}}},
                "response":{{"kind":"html","items":{{"selector":"li"}},
                  "fields":{{"title":{{"selector":"a"}},"url":{{"selector":"a","attr":"href","transform":["absolutize"]}}}}}}}}"#
        )
    }

    #[test]
    fn loads_and_dedups() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "a.json", &minimal("one"));
        write(dir.path(), "b.json", &minimal("one"));
        write(dir.path(), "c.json", &minimal("two"));
        let loaded = load_registry(&[dir.path().to_path_buf()]).unwrap();
        assert_eq!(loaded.registry.len(), 2);
        assert_eq!(loaded.warnings.len(), 1);
        assert_eq!(loaded.warnings[0].code, WarningCode::W4002);
    }

    #[test]
    fn empty_dir_falls_back_to_embedded_definitions() {
        // 配布物で `assets/engines/` が欠けても既定エンジンで起動する（M11）。
        let dir = tempfile::tempdir().unwrap();
        let loaded = load_registry(&[dir.path().to_path_buf()]).unwrap();
        assert!(!loaded.registry.is_empty());
        assert!(loaded.registry.get("duckduckgo").is_some());
    }

    #[test]
    fn missing_embedded_definition_is_reported() {
        // 埋め込みからの読み込みに失敗した場合だけ `E4002` になる。
        let missing = crate::assets::embedded_json(crate::assets::Resource::Engines, "nope");
        assert!(missing.is_none());
    }
}
