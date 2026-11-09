//! エンジン定義の参照・検証・試行。

use crate::core::engine::{EngineDef, PagingStyle};
use crate::core::errors::{AppError, Check, ErrorCode};
use crate::core::template::{self, TmplCtx};
use crate::usecases::App;

/// 一覧の 1 行。
#[derive(Debug, Clone)]
pub struct EngineRow {
    pub id: String,
    pub variant: String,
    pub kind: String,
    pub enabled: bool,
    pub tags: Vec<String>,
    pub requires_api_key: bool,
    pub api_key_state: String,
    pub description: String,
}

/// 一覧を作る。
pub fn list(app: &App) -> Vec<EngineRow> {
    let mut rows: Vec<EngineRow> = app
        .engines
        .all()
        .map(|def| EngineRow {
            id: def.id.clone(),
            variant: def.variant().to_string(),
            kind: match def.response.kind {
                crate::core::engine::ResponseKind::Html => "html".to_string(),
                crate::core::engine::ResponseKind::Json => "json".to_string(),
            },
            enabled: def.enabled,
            tags: def.tags.clone(),
            requires_api_key: def.limits.requires_api_key,
            api_key_state: api_key_state(app, def),
            description: def.description.clone().unwrap_or_default(),
        })
        .collect();
    rows.sort_by(|a, b| a.id.cmp(&b.id).then(a.variant.cmp(&b.variant)));
    rows
}

fn api_key_state(app: &App, def: &EngineDef) -> String {
    let envs = collect_envs(def);
    if envs.is_empty() {
        return "不要".to_string();
    }
    let missing: Vec<&String> = envs.iter().filter(|e| !app.secrets.contains(e)).collect();
    if missing.is_empty() {
        format!("設定済み（{}）", envs.join(","))
    } else {
        format!(
            "未設定（{}）",
            missing
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

fn collect_envs(def: &EngineDef) -> Vec<String> {
    let mut out = Vec::new();
    for value in def
        .request
        .headers
        .values()
        .chain(def.request.query.values())
    {
        if let crate::core::engine::TmplValue::Env { value_env } = value {
            out.push(value_env.clone());
        }
    }
    out
}

/// 定義 1 件を取得する。
pub fn get(app: &App, selector: &str) -> Result<EngineDef, AppError> {
    app.engines.get(selector).cloned().ok_or_else(|| {
        AppError::with_detail(
            ErrorCode::E4002,
            format!("エンジン '{selector}' が見つからない"),
        )
    })
}

/// 検証。
pub fn validate(app: &App, selectors: &[String]) -> Vec<(String, Vec<Check>)> {
    let targets: Vec<EngineDef> = if selectors.is_empty() {
        app.engines.all().cloned().collect()
    } else {
        selectors
            .iter()
            .filter_map(|s| app.engines.get(s).cloned())
            .collect()
    };
    let mut out = Vec::new();
    for def in targets {
        let mut checks = def.validate();
        for env in collect_envs(&def) {
            if app.secrets.contains(&env) {
                checks.push(Check::Ok(format!("環境変数 {env} は設定済み")));
            } else {
                checks.push(Check::Fail(format!("環境変数 {env} が未設定")));
            }
        }
        if checks.iter().all(|c| !c.is_fail()) && !checks.iter().any(|c| matches!(c, Check::Ok(_)))
        {
            checks.push(Check::Ok("問題なし".to_string()));
        }
        out.push((def.selector(), checks));
    }
    out
}

/// リクエストの組み立て結果（`--dry-run`）。
#[derive(Debug, Clone)]
pub struct DryRun {
    pub url: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

/// リクエストを組み立てるのみ。
pub fn dry_run(app: &App, def: &EngineDef, query: &str, page: u32) -> Result<DryRun, AppError> {
    let limit = app.settings.search.limit as u32;
    let (offset, page_number) = template::paging_values(def.paging.as_ref(), page, limit);
    let ctx = TmplCtx {
        query: query.to_string(),
        query_raw: query.to_string(),
        offset,
        page: page_number,
        limit,
        locale: def.defaults.locale.clone().unwrap_or_default(),
        region: def.defaults.region.clone().unwrap_or_default(),
        ..Default::default()
    };
    let prepared = template::prepare(def, &ctx, &app.secrets)?;
    let secrets: Vec<String> = app
        .secrets
        .names()
        .filter_map(|n| app.secrets.value_of(n))
        .map(|s| s.to_string())
        .collect();
    let headers = prepared
        .headers
        .iter()
        .map(|(k, v)| {
            let masked = if secrets.iter().any(|s| v.contains(s.as_str())) {
                "***".to_string()
            } else {
                v.clone()
            };
            (k.clone(), masked)
        })
        .collect();
    Ok(DryRun {
        url: crate::core::url::redact(&crate::core::url::parse(&prepared.url)?, &secrets),
        method: prepared.method,
        headers,
        body: prepared.body,
    })
}

/// 定義の要約（`engines show`）。
pub fn describe(def: &EngineDef) -> Vec<(String, String)> {
    vec![
        ("id".into(), def.id.clone()),
        ("variant".into(), def.variant().to_string()),
        ("name".into(), def.display_name().to_string()),
        (
            "kind".into(),
            match def.response.kind {
                crate::core::engine::ResponseKind::Html => "html".to_string(),
                crate::core::engine::ResponseKind::Json => "json".to_string(),
            },
        ),
        ("enabled".into(), def.enabled.to_string()),
        ("tags".into(), def.tags.join(",")),
        (
            "network".into(),
            def.network.clone().unwrap_or_else(|| "default".to_string()),
        ),
        ("url".into(), def.request.url.clone()),
        ("method".into(), def.request.method.clone()),
        (
            "paging".into(),
            match &def.paging {
                None => "なし".to_string(),
                Some(p) => format!(
                    "{} / {} 件ずつ / 上限 {}",
                    match p.style {
                        PagingStyle::Offset => "offset",
                        PagingStyle::Page => "page",
                    },
                    p.page_size.unwrap_or(10),
                    p.max_results
                ),
            },
        ),
        ("maxResults".into(), def.limits.max_results.to_string()),
        ("engineHash".into(), def.hash()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describe_lists_hash() {
        let def: EngineDef = serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "id": "x",
            "request": {"url": "https://e/x"},
            "response": {"kind":"html","items":{"selector":"li"},
                "fields":{"title":{"selector":"a"},"url":{"selector":"a"}}}
        }))
        .unwrap();
        let rows = describe(&def);
        assert!(rows.iter().any(|(k, v)| k == "engineHash" && v.len() == 16));
    }
}
