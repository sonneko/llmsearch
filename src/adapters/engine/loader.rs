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
    if let Some(state) = crate::adapters::config::state_dir(env, None) {
        out.push(state.join("config/engines"));
    }
    if let Some(config) = env.config_home() {
        out.push(config.join("llmsearch/engines"));
    }
    let _ = base_dir;
    out.push(bundled.join("engines"));
    out
}

/// ディレクトリ群を走査して定義を読み込む（同一 `id@variant` は先勝ち）。
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
                    format!("{}: 行 {} 桁 {}: {e}", path.display(), e.line(), e.column()),
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
                            "{} の定義 '{}' は既に読み込まれているため無視した",
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

    if registry.is_empty() {
        return Err(AppError::with_detail(
            ErrorCode::E4002,
            format!(
                "エンジン定義が見つからない（探索: {}）",
                dirs.iter()
                    .map(|d| d.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
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
    fn empty_dir_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = load_registry(&[dir.path().to_path_buf()]).unwrap_err();
        assert_eq!(err.code, ErrorCode::E4002);
    }
}
