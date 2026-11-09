//! エンジン定義（JSON）。コアは定義を評価するだけで、エンジン名で分岐しない。

use serde::{Deserialize, Serialize};

use crate::core::errors::{AppError, Check, ErrorCode};

pub const SUPPORTED_SCHEMA_VERSION: u32 = 1;

/// エンジン定義。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct EngineDef {
    pub schema_version: u32,
    pub id: String,
    pub variant: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub enabled: bool,
    pub tags: Vec<String>,
    pub network: Option<String>,
    pub request: RequestSpec,
    pub response: ResponseSpec,
    pub paging: Option<PagingSpec>,
    pub limits: Limits,
    pub defaults: Defaults,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// `id@variant` 形式の表示名。
impl EngineDef {
    pub fn selector(&self) -> String {
        match &self.variant {
            Some(v) if v != "default" => format!("{}@{}", self.id, v),
            _ => self.id.clone(),
        }
    }

    pub fn variant(&self) -> &str {
        self.variant.as_deref().unwrap_or("default")
    }

    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.id)
    }

    /// 内容ハッシュ（キャッシュキー用）。
    pub fn hash(&self) -> String {
        use sha2::{Digest, Sha256};
        let json = serde_json::to_string(self).unwrap_or_default();
        let mut hasher = Sha256::new();
        hasher.update(json.as_bytes());
        let digest = hasher.finalize();
        digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
    }
}

impl Default for EngineDef {
    fn default() -> Self {
        Self {
            schema_version: SUPPORTED_SCHEMA_VERSION,
            id: String::new(),
            variant: None,
            name: None,
            description: None,
            enabled: true,
            tags: Vec::new(),
            network: None,
            request: RequestSpec::default(),
            response: ResponseSpec::default(),
            paging: None,
            limits: Limits::default(),
            defaults: Defaults::default(),
            extra: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Defaults {
    pub locale: Option<String>,
    pub region: Option<String>,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct RequestSpec {
    pub method: String,
    pub url: String,
    pub query: std::collections::BTreeMap<String, TmplValue>,
    pub headers: std::collections::BTreeMap<String, TmplValue>,
    pub body: Option<String>,
    pub content_type: Option<String>,
    pub encoding: String,
    pub timeout_ms: Option<u64>,
}

impl Default for RequestSpec {
    fn default() -> Self {
        Self {
            method: "GET".into(),
            url: String::new(),
            query: Default::default(),
            headers: Default::default(),
            body: None,
            content_type: None,
            encoding: "utf-8".into(),
            timeout_ms: None,
        }
    }
}

/// テンプレート値（文字列または環境変数参照）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum TmplValue {
    /// テンプレート文字列（`{{query}}` 等）。
    Value(String),
    /// 環境変数からの参照。
    Env { value_env: String },
    /// 固定値（キー以外の用途）。
    Explicit { value: String },
}

impl From<&str> for TmplValue {
    fn from(s: &str) -> Self {
        TmplValue::Value(s.to_string())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum ResponseKind {
    #[default]
    Html,
    Json,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ResponseSpec {
    pub kind: ResponseKind,
    pub charset: Option<String>,
    pub items: ItemSpec,
    pub fields: std::collections::BTreeMap<String, FieldSpec>,
    pub error_when: Vec<ErrorCond>,
    pub transform_args: TransformArgs,
}

impl Default for ResponseSpec {
    fn default() -> Self {
        Self {
            kind: ResponseKind::Html,
            charset: None,
            items: ItemSpec::default(),
            fields: Default::default(),
            error_when: Vec::new(),
            transform_args: TransformArgs::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ItemSpec {
    pub selector: Option<String>,
    pub path: Option<String>,
}

/// フィールドの抽出規則（§4.3）。JSON では「キーの有無」で形式を判別する。
#[derive(Debug, Clone, PartialEq)]
pub enum FieldSpec {
    Selector {
        selector: String,
        attr: Option<String>,
        transform: Vec<Transform>,
    },
    Path {
        path: String,
        transform: Vec<Transform>,
    },
    From {
        from: String,
        transform: Vec<Transform>,
    },
    Const {
        value: String,
    },
    Join {
        join: Vec<String>,
        separator: Option<String>,
        transform: Vec<Transform>,
    },
}

fn get_str(obj: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<String> {
    obj.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

impl FieldSpec {
    pub fn from_json(v: &serde_json::Value) -> Result<Self, String> {
        let obj = v
            .as_object()
            .ok_or("フィールド定義はオブジェクトであること")?;
        let transform = match obj.get("transform") {
            None => Vec::new(),
            Some(serde_json::Value::Array(items)) => {
                let mut out = Vec::new();
                for item in items {
                    let name = item.as_str().ok_or("transform の要素は文字列であること")?;
                    out.push(Transform::parse(name).ok_or_else(|| format!("未知の変換 {name}"))?);
                }
                out
            }
            Some(_) => return Err("transform は配列であること".into()),
        };
        if let Some(selector) = get_str(obj, "selector") {
            return Ok(FieldSpec::Selector {
                selector,
                attr: get_str(obj, "attr"),
                transform,
            });
        }
        if let Some(join) = obj.get("join") {
            let items: Vec<String> = join
                .as_array()
                .ok_or("join は配列であること")?
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(|s| s.to_string())
                        .ok_or("join の要素は文字列であること")
                })
                .collect::<Result<_, _>>()?;
            return Ok(FieldSpec::Join {
                join: items,
                separator: get_str(obj, "separator"),
                transform,
            });
        }
        if let Some(path) = get_str(obj, "path") {
            return Ok(FieldSpec::Path { path, transform });
        }
        if let Some(from) = get_str(obj, "from") {
            return Ok(FieldSpec::From { from, transform });
        }
        if let Some(value) = get_str(obj, "const") {
            return Ok(FieldSpec::Const { value });
        }
        Err("selector / path / from / const / join のいずれかが必要".into())
    }

    fn to_json(&self) -> serde_json::Value {
        use serde_json::json;
        let transforms = |t: &[Transform]| -> serde_json::Value {
            serde_json::Value::Array(
                t.iter()
                    .map(|x| serde_json::Value::String(x.as_str().to_string()))
                    .collect(),
            )
        };
        match self {
            FieldSpec::Selector {
                selector,
                attr,
                transform,
            } => {
                let mut m = serde_json::Map::new();
                m.insert("selector".into(), json!(selector));
                if let Some(a) = attr {
                    m.insert("attr".into(), json!(a));
                }
                if !transform.is_empty() {
                    m.insert("transform".into(), transforms(transform));
                }
                serde_json::Value::Object(m)
            }
            FieldSpec::Path { path, transform } => {
                let mut m = serde_json::Map::new();
                m.insert("path".into(), json!(path));
                if !transform.is_empty() {
                    m.insert("transform".into(), transforms(transform));
                }
                serde_json::Value::Object(m)
            }
            FieldSpec::From { from, transform } => {
                let mut m = serde_json::Map::new();
                m.insert("from".into(), json!(from));
                if !transform.is_empty() {
                    m.insert("transform".into(), transforms(transform));
                }
                serde_json::Value::Object(m)
            }
            FieldSpec::Const { value } => json!({ "const": value }),
            FieldSpec::Join {
                join,
                separator,
                transform,
            } => {
                let mut m = serde_json::Map::new();
                m.insert("join".into(), json!(join));
                if let Some(s) = separator {
                    m.insert("separator".into(), json!(s));
                }
                if !transform.is_empty() {
                    m.insert("transform".into(), transforms(transform));
                }
                serde_json::Value::Object(m)
            }
        }
    }

    pub fn transform(&self) -> &[Transform] {
        match self {
            FieldSpec::Selector { transform, .. }
            | FieldSpec::Path { transform, .. }
            | FieldSpec::From { transform, .. }
            | FieldSpec::Join { transform, .. } => transform,
            FieldSpec::Const { .. } => &[],
        }
    }
}

impl serde::Serialize for FieldSpec {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.to_json().serialize(s)
    }
}

impl<'de> serde::Deserialize<'de> for FieldSpec {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = serde_json::Value::deserialize(d)?;
        FieldSpec::from_json(&v).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Transform {
    Trim,
    CollapseWs,
    StripHtml,
    DecodeEntities,
    Absolutize,
    UnwrapRedirect,
    StripTracking,
    Truncate,
    Replace,
    Lower,
    Upper,
    ToDate,
}

impl Transform {
    pub fn as_str(self) -> &'static str {
        match self {
            Transform::Trim => "trim",
            Transform::CollapseWs => "collapse_ws",
            Transform::StripHtml => "strip_html",
            Transform::DecodeEntities => "decode_entities",
            Transform::Absolutize => "absolutize",
            Transform::UnwrapRedirect => "unwrap_redirect",
            Transform::StripTracking => "strip_tracking",
            Transform::Truncate => "truncate",
            Transform::Replace => "replace",
            Transform::Lower => "lower",
            Transform::Upper => "upper",
            Transform::ToDate => "to_date",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "trim" => Transform::Trim,
            "collapse_ws" => Transform::CollapseWs,
            "strip_html" => Transform::StripHtml,
            "decode_entities" => Transform::DecodeEntities,
            "absolutize" => Transform::Absolutize,
            "unwrap_redirect" => Transform::UnwrapRedirect,
            "strip_tracking" => Transform::StripTracking,
            "truncate" => Transform::Truncate,
            "replace" => Transform::Replace,
            "lower" => Transform::Lower,
            "upper" => Transform::Upper,
            "to_date" => Transform::ToDate,
            _ => return None,
        })
    }
}

impl serde::Serialize for Transform {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> serde::Deserialize<'de> for Transform {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Transform::parse(&s).ok_or_else(|| serde::de::Error::custom(format!("未知の変換 {s}")))
    }
}

/// 変換の補助パラメータ（`chars` / `pattern` / `with` 等）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct TransformArgs {
    pub chars: Option<usize>,
    pub keys: Vec<String>,
    pub formats: Vec<String>,
    pub pattern: Option<String>,
    pub with: Option<String>,
}

/// 「結果なし」とみなす条件（§4.5）。
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorCond {
    FieldEmpty { field: String },
    Contains { contains: String },
    PathExists { path: String },
    Status { status: u16 },
}

impl ErrorCond {
    pub fn from_json(v: &serde_json::Value) -> Result<Self, String> {
        let obj = v
            .as_object()
            .ok_or("errorWhen の要素はオブジェクトであること")?;
        if let Some(field) = get_str(obj, "field") {
            let empty = obj.get("empty").and_then(|b| b.as_bool()).unwrap_or(true);
            if empty {
                return Ok(ErrorCond::FieldEmpty { field });
            }
        }
        if let Some(contains) = get_str(obj, "contains") {
            return Ok(ErrorCond::Contains { contains });
        }
        if let Some(path) = get_str(obj, "path") {
            let exists = obj.get("exists").and_then(|b| b.as_bool()).unwrap_or(true);
            if exists {
                return Ok(ErrorCond::PathExists { path });
            }
        }
        if let Some(status) = obj.get("status").and_then(|s| s.as_u64()) {
            return Ok(ErrorCond::Status {
                status: status as u16,
            });
        }
        Err("field / contains / path / status のいずれかが必要".into())
    }

    fn to_json(&self) -> serde_json::Value {
        use serde_json::json;
        match self {
            ErrorCond::FieldEmpty { field } => json!({"field": field, "empty": true}),
            ErrorCond::Contains { contains } => json!({"contains": contains}),
            ErrorCond::PathExists { path } => json!({"path": path, "exists": true}),
            ErrorCond::Status { status } => json!({"status": status}),
        }
    }
}

impl serde::Serialize for ErrorCond {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.to_json().serialize(s)
    }
}

impl<'de> serde::Deserialize<'de> for ErrorCond {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = serde_json::Value::deserialize(d)?;
        ErrorCond::from_json(&v).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct PagingSpec {
    pub style: PagingStyle,
    pub offset_param: String,
    pub page_param: String,
    pub page_base: u32,
    pub page_size: Option<u32>,
    pub max_results: u32,
    pub offset_step: Option<u32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PagingStyle {
    Offset,
    Page,
}

impl Default for PagingSpec {
    fn default() -> Self {
        Self {
            style: PagingStyle::Offset,
            offset_param: "s".into(),
            page_param: "page".into(),
            page_base: 1,
            page_size: None,
            max_results: 30,
            offset_step: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Limits {
    pub max_results: u32,
    pub min_interval_ms: Option<u64>,
    pub max_page_size: u32,
    pub requires_api_key: bool,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_results: 30,
            min_interval_ms: None,
            max_page_size: 10,
            requires_api_key: false,
        }
    }
}

/// `id@variant` を分解する。
pub fn split_selector(selector: &str) -> (String, String) {
    match selector.split_once('@') {
        Some((id, variant)) => (id.to_string(), variant.to_string()),
        None => (selector.to_string(), "default".to_string()),
    }
}

const ALLOWED_VARS: &[&str] = &[
    "query", "queryRaw", "offset", "page", "limit", "locale", "region", "apiKey", "dateFrom",
    "dateTo",
];

fn template_vars(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        match after.find("}}") {
            Some(end) => {
                out.push(after[..end].trim().to_string());
                rest = &after[end + 2..];
            }
            None => break,
        }
    }
    out
}

impl EngineDef {
    /// 定義の検証。
    pub fn validate(&self) -> Vec<Check> {
        let mut out = Vec::new();
        if self.schema_version != SUPPORTED_SCHEMA_VERSION {
            out.push(Check::Fail(format!(
                "schemaVersion = {}（対応版は {SUPPORTED_SCHEMA_VERSION}）",
                self.schema_version
            )));
        }
        let id_ok = !self.id.is_empty()
            && self
                .id
                .chars()
                .next()
                .map(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
                .unwrap_or(false)
            && self.id.chars().all(|c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-')
            });
        if !id_ok {
            out.push(Check::Fail(format!(
                "id = '{}' は [a-z0-9._-] のみ使用できる",
                self.id
            )));
        }
        if self.request.url.is_empty() {
            out.push(Check::Fail("request.url は必須".into()));
        } else if !(self.request.url.starts_with("http://")
            || self.request.url.starts_with("https://")
            || self.request.url.contains("{{"))
        {
            out.push(Check::Fail(
                "request.url は http(s) で始まる必要がある".into(),
            ));
        }
        // テンプレート変数
        let mut tmpls: Vec<&str> = vec![&self.request.url];
        for v in self
            .request
            .query
            .values()
            .chain(self.request.headers.values())
        {
            if let TmplValue::Value(s) = v {
                tmpls.push(s);
            }
        }
        for t in tmpls {
            for var in template_vars(t) {
                if !ALLOWED_VARS.contains(&var.as_str()) {
                    out.push(Check::Fail(format!("未知のテンプレート変数 {{{{{var}}}}}")));
                }
            }
        }
        let uses_api_key = template_vars(&self.request.url).contains(&"apiKey".to_string());
        if self.limits.requires_api_key && !uses_api_key && self.request.query.is_empty() {
            out.push(Check::Warn(
                "requiresApiKey だが {{apiKey}} を使用していない".into(),
            ));
        }
        // items と kind の整合
        match self.response.kind {
            ResponseKind::Html => {
                if self.response.items.selector.is_none() {
                    out.push(Check::Fail(
                        "response.kind=html には items.selector が必要".into(),
                    ));
                }
            }
            ResponseKind::Json => {
                if self.response.items.path.is_none() {
                    out.push(Check::Fail(
                        "response.kind=json には items.path が必要".into(),
                    ));
                }
            }
        }
        // 必須フィールド
        for required in ["title", "url"] {
            if !self.response.fields.contains_key(required) {
                out.push(Check::Fail(format!("response.fields.{required} は必須")));
            }
        }
        // セレクタの構文
        if let Some(sel) = &self.response.items.selector
            && scraper::Selector::parse(sel).is_err()
        {
            out.push(Check::Fail(format!("items.selector が不正: {sel}")));
        }
        for (name, field) in &self.response.fields {
            if let FieldSpec::Selector { selector, .. } = field
                && scraper::Selector::parse(selector).is_err()
            {
                out.push(Check::Fail(format!(
                    "fields.{name}.selector が不正: {selector}"
                )));
            }
            for t in field.transform() {
                match t {
                    Transform::Truncate if self.response.transform_args.chars.is_none() => {
                        out.push(Check::Fail(format!(
                            "fields.{name}: truncate には chars が必要"
                        )));
                    }
                    Transform::Replace if self.response.transform_args.pattern.is_none() => {
                        out.push(Check::Fail(format!(
                            "fields.{name}: replace には pattern が必要"
                        )));
                    }
                    _ => {}
                }
            }
        }
        if self.response.kind == ResponseKind::Html
            && self.response.items.selector.is_none()
            && self.response.items.path.is_some()
        {
            out.push(Check::Warn(
                "response.kind=html で items.path が指定されている".into(),
            ));
        }
        // ページング整合
        if let Some(p) = &self.paging {
            if p.style == PagingStyle::Offset && p.offset_step == Some(0) {
                out.push(Check::Fail("paging.offsetStep は 1 以上".into()));
            }
            if p.max_results == 0 {
                out.push(Check::Fail("paging.maxResults は 1 以上".into()));
            }
        }
        if self.limits.max_page_size == 0 {
            out.push(Check::Fail("limits.maxPageSize は 1 以上".into()));
        }
        for key in self.extra.keys() {
            out.push(Check::Warn(format!("未知キー {key}（保持される）")));
        }
        out
    }

    pub fn has_fail(&self) -> bool {
        self.validate().iter().any(|c| c.is_fail())
    }

    pub fn ensure_valid(&self) -> Result<(), AppError> {
        match self.validate().into_iter().find(|c| c.is_fail()) {
            None => Ok(()),
            Some(fail) => Err(AppError::with_detail(
                ErrorCode::E1004,
                format!("{}: {}", self.selector(), fail.text()),
            )),
        }
    }

    /// 実効的な最大取得件数。
    pub fn max_results(&self, global_max: usize) -> usize {
        let paging = self
            .paging
            .as_ref()
            .map(|p| p.max_results as usize)
            .unwrap_or(u32::MAX as usize);
        (self.limits.max_results as usize)
            .min(paging)
            .min(global_max)
    }
}

/// エンジン定義のレジストリ。
#[derive(Debug, Clone, Default)]
pub struct EngineRegistry {
    engines: std::collections::BTreeMap<(String, String), EngineDef>,
}

impl EngineRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 追加する。既に同じ (id, variant) がある場合は `false` を返す。
    pub fn insert(&mut self, def: EngineDef) -> bool {
        let key = (def.id.clone(), def.variant().to_string());
        if self.engines.contains_key(&key) {
            return false;
        }
        self.engines.insert(key, def);
        true
    }

    pub fn get(&self, selector: &str) -> Option<&EngineDef> {
        let (id, variant) = split_selector(selector);
        self.engines.get(&(id, variant))
    }

    pub fn all(&self) -> impl Iterator<Item = &EngineDef> {
        self.engines.values()
    }

    pub fn enabled(&self) -> impl Iterator<Item = &EngineDef> {
        self.engines.values().filter(|d| d.enabled)
    }

    pub fn len(&self) -> usize {
        self.engines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.engines.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> EngineDef {
        serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "id": "example",
            "request": {"url": "https://example.com/s", "query": {"q": "{{query}}"}},
            "response": {
                "kind": "html",
                "items": {"selector": "li.result"},
                "fields": {
                    "title": {"selector": "h2", "transform": ["trim", "collapse_ws"]},
                    "url": {"selector": "a", "attr": "href", "transform": ["absolutize"]}
                }
            },
            "paging": {"style": "offset", "offsetParam": "s", "maxResults": 30}
        }))
        .unwrap()
    }

    #[test]
    fn parses_and_validates() {
        let def = sample();
        assert!(!def.has_fail(), "{:?}", def.validate());
        assert_eq!(def.hash().len(), 16);
    }

    #[test]
    fn detects_unknown_template_variable() {
        let mut def = sample();
        def.request.url = "https://example.com/s?x={{nope}}".into();
        assert!(def.has_fail());
    }

    #[test]
    fn registry_resolves_variant() {
        let mut reg = EngineRegistry::new();
        assert!(reg.insert(sample()));
        assert!(!reg.insert(sample()));
        assert!(reg.get("example").is_some());
        assert_eq!(reg.get("example").unwrap().variant(), "default");
    }

    #[test]
    fn selector_splitting() {
        assert_eq!(split_selector("a@b"), ("a".into(), "b".into()));
        assert_eq!(split_selector("a"), ("a".into(), "default".into()));
    }
}
#[test]
fn dbg_fields() {
    let r: Result<crate::core::engine::FieldSpec, _> =
        serde_json::from_str(r#"{"selector":"h2","transform":["trim"]}"#);
    println!("selector-only: {r:?}");
    let r2: Result<crate::core::engine::FieldSpec, _> =
        serde_json::from_str(r#"{"selector":"a","attr":"href","transform":["absolutize"]}"#);
    println!("with attr: {r2:?}");
    let q: Result<std::collections::BTreeMap<String, crate::core::engine::TmplValue>, _> =
        serde_json::from_str(r#"{"q":"{{query}}"}"#);
    println!("query: {q:?}");
}
