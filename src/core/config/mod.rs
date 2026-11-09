//! 設定の型（JSON）。

pub mod filter;
pub mod network;
pub mod quota;
pub mod settings;

pub use filter::FilterSet;
pub use network::NetworkProfile;
pub use quota::QuotaPolicy;
pub use settings::Settings;

use serde::{Deserialize, Serialize};

/// ID 参照またはインライン定義。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum RefOr<T> {
    Id(String),
    Inline(T),
}

impl<T> RefOr<T> {
    pub fn id(&self) -> Option<&str> {
        match self {
            RefOr::Id(id) => Some(id),
            RefOr::Inline(_) => None,
        }
    }
}

/// JSON オブジェクトの深いマージ（配列とスカラーは置換）。
pub fn merge_json(base: &mut serde_json::Value, over: serde_json::Value) {
    use serde_json::Value;
    match (base, over) {
        (Value::Object(b), Value::Object(o)) => {
            for (k, v) in o {
                match b.get_mut(&k) {
                    Some(existing) => merge_json(existing, v),
                    None => {
                        b.insert(k, v);
                    }
                }
            }
        }
        (b, o) => *b = o,
    }
}

/// 参照先を読み込む関数。I/O は `core` に置かないため、呼び出し側から渡す。
pub type ReadJson =
    dyn Fn(&std::path::Path) -> Result<serde_json::Value, crate::core::errors::AppError>;

/// `{"$ref": "./x.json"}` を解決する。
///
/// パス解決と循環検出のみを担い、ファイル読み込みは `read` に委ねる。
pub fn resolve_refs(
    value: &mut serde_json::Value,
    base_dir: &std::path::Path,
    visited: &mut std::collections::HashSet<std::path::PathBuf>,
    read: &ReadJson,
) -> Result<(), crate::core::errors::AppError> {
    use crate::core::errors::{AppError, ErrorCode};
    use serde_json::Value;
    match value {
        Value::Object(map) => {
            if let Some(Value::String(target)) = map.get("$ref") {
                if target.starts_with("http://") || target.starts_with("https://") {
                    return Err(AppError::with_detail(
                        ErrorCode::E1003,
                        format!("リモート参照は禁止: {target}"),
                    ));
                }
                let path = base_dir.join(target);
                let canonical = path.canonicalize().unwrap_or(path.clone());
                if !visited.insert(canonical.clone()) {
                    return Err(AppError::with_detail(
                        ErrorCode::E1003,
                        format!("$ref の循環参照: {}", path.display()),
                    ));
                }
                let mut loaded: Value = read(&path)?;
                let dir = path.parent().unwrap_or(base_dir).to_path_buf();
                resolve_refs(&mut loaded, &dir, visited, read)?;
                *value = loaded;
                return Ok(());
            }
            for (_, v) in map.iter_mut() {
                resolve_refs(v, base_dir, visited, read)?;
            }
            Ok(())
        }
        Value::Array(items) => {
            for v in items.iter_mut() {
                resolve_refs(v, base_dir, visited, read)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merges_objects_recursively_and_replaces_arrays() {
        let mut base = json!({"a": {"b": 1, "c": [1,2]}, "d": 1});
        merge_json(&mut base, json!({"a": {"c": [3], "e": 5}, "d": 2}));
        assert_eq!(base, json!({"a": {"b":1, "c":[3], "e":5}, "d": 2}));
    }
}
