//! 同梱アセットの既定値（埋め込み）。
//!
//! 配布物は `assets/` をバイナリの隣に置くが、ステージ漏れ・部分コピー・
//! 入れ子コピーで既定値が消えることがある（M11 の事故）。その場合でも起動
//! できるよう、既定値をバイナリへ焼き込んでおく。ファイル探索で見つかった
//! 場合はそちらを優先する（利用者の編集を尊重するため。仕様 configuration.md §1.1、
//! network.md §1.3）。

use serde_json::Value;

use crate::core::errors::{AppError, ErrorCode};

/// 資源の種別。埋め込みの持ち方を揃えるために使う。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resource {
    /// ネットワークプロファイル（`network/<id>.json`）。
    Network,
    /// フィルタ集合（`filters/<id>.json`）。
    Filters,
    /// クォータポリシー（`quotas/<id>.json`）。
    Quota,
    /// エンジン定義（`engines/*.json`）。
    Engines,
}

impl Resource {
    /// ディレクトリ名（`assets/` の下）。
    pub fn dir(self) -> &'static str {
        match self {
            Resource::Network => "network",
            Resource::Filters => "filters",
            Resource::Quota => "quotas",
            Resource::Engines => "engines",
        }
    }

    /// 埋め込み既定の一覧（`(id, JSON 本文)`）。
    ///
    /// エンジン定義はファイル名ではなく定義の `id` で引く。
    pub fn embedded(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Resource::Network => &[
                ("default", include_str!("../assets/network/default.json")),
                (
                    "self-hosted",
                    include_str!("../assets/network/self-hosted.json"),
                ),
            ],
            Resource::Filters => &[("default", include_str!("../assets/filters/default.json"))],
            Resource::Quota => &[("default", include_str!("../assets/quotas/default.json"))],
            Resource::Engines => &[
                ("brave@api", include_str!("../assets/engines/brave.json")),
                (
                    "duckduckgo@default",
                    include_str!("../assets/engines/duckduckgo.json"),
                ),
                (
                    "searxng@json",
                    include_str!("../assets/engines/searxng.json"),
                ),
            ],
        }
    }
}

/// 埋め込み既定の JSON を 1 件返す（無ければ `None`）。
pub fn embedded_json(resource: Resource, id: &str) -> Option<Value> {
    resource
        .embedded()
        .iter()
        .find(|(name, _)| *name == id)
        .and_then(|(_, text)| serde_json::from_str(text).ok())
}

/// 埋め込み既定を型へ変換する。解析失敗は `E1003`（同梱物の破損）。
pub fn embedded_as<T: serde::de::DeserializeOwned>(
    resource: Resource,
    id: &str,
    what: &str,
) -> Result<Option<T>, AppError> {
    match embedded_json(resource, id) {
        Some(value) => serde_json::from_value(value).map(Some).map_err(|e| {
            AppError::with_detail(
                ErrorCode::E1003,
                format!("embedded {what} '{id}' is unreadable: {e}"),
            )
        }),
        None => Ok(None),
    }
}

/// `config show` の `source` に出す印（埋め込みから解決した）。
pub const EMBEDDED_SOURCE: &str = "embedded";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_embedded_asset_is_valid_json() {
        for resource in [
            Resource::Network,
            Resource::Filters,
            Resource::Quota,
            Resource::Engines,
        ] {
            for (id, text) in resource.embedded() {
                let parsed: Result<Value, _> = serde_json::from_str(text);
                assert!(parsed.is_ok(), "{} {id} must be valid JSON", resource.dir());
            }
        }
    }

    #[test]
    fn embedded_defaults_exist() {
        assert!(embedded_json(Resource::Network, "default").is_some());
        assert!(embedded_json(Resource::Filters, "default").is_some());
        assert!(embedded_json(Resource::Quota, "default").is_some());
        assert!(embedded_json(Resource::Engines, "duckduckgo@default").is_some());
        assert!(embedded_json(Resource::Network, "no-such-profile").is_none());
    }
}
