//! 抽出規則の評価（CSS セレクタ / JSON パス）。
//!
//! エンジン固有の知識は持たない。`response.kind` による分岐はここに閉じる。

use std::collections::BTreeMap;

use scraper::{Html, Selector};

use crate::core::engine::{
    ErrorCond, FieldSpec, ItemSpec, ResponseKind, ResponseSpec, Transform, TransformArgs,
};
use crate::core::errors::{AppError, Diagnostic, ErrorCode, WarningCode};
use crate::core::search::RawHit;
use crate::core::{text, url};

/// 抽出の結果。
#[derive(Debug, Clone, Default)]
pub struct ExtractOutcome {
    pub hits: Vec<RawHit>,
    /// 応答から取り出せた要素数（抽出規則の一致判定に使う）。
    pub items_seen: usize,
    pub dropped: usize,
    pub no_results: bool,
    /// 抽出中に検出した警告。どのコードで出すかは検出した層が決める。
    pub warnings: Vec<Diagnostic>,
}

fn selector_of(pattern: &str) -> Result<Selector, AppError> {
    Selector::parse(pattern)
        .map_err(|e| AppError::with_detail(ErrorCode::E1004, format!("セレクタ {pattern}: {e:?}")))
}

/// 応答本文から結果を取り出す。
pub fn extract(
    spec: &ResponseSpec,
    body: &str,
    base_url: &url::Url,
    tracking: &[String],
) -> Result<ExtractOutcome, AppError> {
    match spec.kind {
        ResponseKind::Html => extract_html(spec, body, base_url, tracking),
        ResponseKind::Json => extract_json(spec, body, base_url, tracking),
    }
}

// ---------------------------------------------------------------------------
// HTML
// ---------------------------------------------------------------------------

fn extract_html(
    spec: &ResponseSpec,
    body: &str,
    base_url: &url::Url,
    tracking: &[String],
) -> Result<ExtractOutcome, AppError> {
    let doc = Html::parse_document(body);
    let mut out = ExtractOutcome::default();

    if let Some(cond) = trigger_contains(spec, &doc, body) {
        out.no_results = true;
        out.warnings.push(cond);
        return Ok(out);
    }

    let Some(item_selector) = &spec.items.selector else {
        return Err(AppError::with_detail(
            ErrorCode::E1004,
            "response.items.selector が無い",
        ));
    };
    let item_sel = selector_of(item_selector)?;
    let field_sels: BTreeMap<&str, Selector> = spec
        .fields
        .iter()
        .filter_map(|(name, f)| match f {
            FieldSpec::Selector { selector, .. } => {
                selector_of(selector).ok().map(|s| (name.as_str(), s))
            }
            _ => None,
        })
        .collect();

    for element in doc.select(&item_sel) {
        out.items_seen += 1;
        let mut values: BTreeMap<String, String> = BTreeMap::new();
        for (name, field) in &spec.fields {
            let raw = match field {
                FieldSpec::Selector { attr, .. } => {
                    let Some(sel) = field_sels.get(name.as_str()) else {
                        continue;
                    };
                    match element.select(sel).next() {
                        None => String::new(),
                        Some(node) => match attr {
                            Some(attr_name) => {
                                node.value().attr(attr_name).unwrap_or_default().to_string()
                            }
                            None => node.text().collect::<Vec<_>>().join(" "),
                        },
                    }
                }
                FieldSpec::Const { value } => value.clone(),
                _ => String::new(), // Path/From/Join は JSON 用。HTML では無視する
            };
            values.insert(name.clone(), raw);
        }
        let hit = finish_hit(spec, &mut values, base_url, tracking);
        match hit {
            Some(h) => out.hits.push(h),
            None => out.dropped += 1,
        }
    }

    finalize(&mut out);
    Ok(out)
}

fn trigger_contains(spec: &ResponseSpec, doc: &Html, body: &str) -> Option<Diagnostic> {
    for cond in &spec.error_when {
        match cond {
            ErrorCond::Contains { contains } => {
                if body.contains(contains) {
                    return Some(Diagnostic::new(
                        WarningCode::W4004,
                        format!("errorWhen.contains: {contains}"),
                    ));
                }
            }
            ErrorCond::FieldEmpty { field } => {
                if let Some(FieldSpec::Selector { selector, .. }) = spec.fields.get(field)
                    && let Ok(sel) = Selector::parse(selector)
                    && doc.select(&sel).next().is_none()
                {
                    return Some(Diagnostic::new(
                        WarningCode::W4004,
                        format!("errorWhen.fieldEmpty: {field}"),
                    ));
                }
            }
            ErrorCond::PathExists { .. } | ErrorCond::Status { .. } => {}
        }
    }
    None
}

// ---------------------------------------------------------------------------
// JSON
// ---------------------------------------------------------------------------

fn extract_json(
    spec: &ResponseSpec,
    body: &str,
    base_url: &url::Url,
    tracking: &[String],
) -> Result<ExtractOutcome, AppError> {
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| AppError::with_detail(ErrorCode::E4004, format!("JSON の解析に失敗: {e}")))?;
    let mut out = ExtractOutcome::default();

    for cond in &spec.error_when {
        match cond {
            ErrorCond::PathExists { path } => {
                if json_path(&value, path)
                    .map(|v| !v.is_null())
                    .unwrap_or(false)
                {
                    out.no_results = true;
                    out.warnings.push(Diagnostic::new(
                        WarningCode::W4004,
                        format!("errorWhen.pathExists: {path}"),
                    ));
                    return Ok(out);
                }
            }
            ErrorCond::Contains { contains } => {
                if body.contains(contains) {
                    out.no_results = true;
                    out.warnings.push(Diagnostic::new(
                        WarningCode::W4004,
                        format!("errorWhen.contains: {contains}"),
                    ));
                    return Ok(out);
                }
            }
            _ => {}
        }
    }

    let items: Vec<serde_json::Value> = match &spec.items.path {
        Some(p) => match json_path(&value, p) {
            Some(serde_json::Value::Array(items)) => items.clone(),
            Some(v) => vec![v.clone()],
            None => Vec::new(),
        },
        None => Vec::new(),
    };

    out.items_seen = items.len();
    for item in &items {
        let mut values: BTreeMap<String, String> = BTreeMap::new();
        for (name, field) in &spec.fields {
            let raw = match field {
                FieldSpec::Path { path, .. } => {
                    json_path(item, path).map(json_scalar).unwrap_or_default()
                }
                FieldSpec::Const { value } => value.clone(),
                _ => String::new(),
            };
            values.insert(name.clone(), raw);
        }
        match finish_hit(spec, &mut values, base_url, tracking) {
            Some(h) => out.hits.push(h),
            None => out.dropped += 1,
        }
    }

    finalize(&mut out);
    Ok(out)
}

fn json_scalar(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// ドット記法のパス解決（`a.b[0].c`、`[]` で全要素、`[*]` も同義）。
pub fn json_path<'a>(root: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    if path.is_empty() {
        return Some(root);
    }
    let mut cur = root;
    let mut buf = String::new();
    let mut chars = path.chars().peekable();
    let mut pending_index: Option<usize> = None;
    let mut pending_all = false;

    let apply = |cur: &'a serde_json::Value,
                 key: &str,
                 index: Option<usize>,
                 all: bool|
     -> Option<&'a serde_json::Value> {
        let mut node = if key.is_empty() { cur } else { cur.get(key)? };
        if all {
            return None; // 呼び出し側で配列展開する
        }
        if let Some(i) = index {
            node = node.get(i)?;
        }
        Some(node)
    };

    while let Some(c) = chars.next() {
        match c {
            '.' => {
                if !buf.is_empty() {
                    cur = apply(cur, &buf, pending_index.take(), pending_all)?;
                    buf.clear();
                    pending_all = false;
                }
            }
            '[' => {
                let mut inner = String::new();
                for ic in chars.by_ref() {
                    if ic == ']' {
                        break;
                    }
                    inner.push(ic);
                }
                if inner.is_empty() || inner == "*" {
                    // 配列展開: 先頭要素ではなく「配列そのもの」を返す特別扱い
                    let node = if buf.is_empty() {
                        cur
                    } else {
                        cur.get(buf.as_str())?
                    };
                    buf.clear();
                    let arr = node.as_array()?;
                    // 残りのパスを各要素に適用する必要があるため、ここでは先頭要素を採る
                    pending_all = true;
                    cur = arr.first()?;
                } else if let Ok(i) = inner.parse::<usize>() {
                    pending_index = Some(i);
                }
            }
            other => buf.push(other),
        }
    }
    if buf.is_empty() && pending_index.is_none() {
        return Some(cur);
    }
    apply(cur, &buf, pending_index, false)
}

// ---------------------------------------------------------------------------
// 共通
// ---------------------------------------------------------------------------

fn finish_hit(
    spec: &ResponseSpec,
    values: &mut BTreeMap<String, String>,
    base_url: &url::Url,
    tracking: &[String],
) -> Option<RawHit> {
    // From / Join を依存順に解決する（最大 3 回の反復で十分）
    for _ in 0..3 {
        for (name, field) in &spec.fields {
            match field {
                FieldSpec::From { from, .. } => {
                    let src = values.get(from).cloned().unwrap_or_default();
                    values.insert(name.clone(), src);
                }
                FieldSpec::Join {
                    join, separator, ..
                } => {
                    let sep = separator.as_deref().unwrap_or(" ");
                    let joined = join
                        .iter()
                        .map(|k| values.get(k).cloned().unwrap_or_default())
                        .filter(|s| !s.trim().is_empty())
                        .collect::<Vec<_>>()
                        .join(sep);
                    values.insert(name.clone(), joined);
                }
                _ => {}
            }
        }
    }

    for (name, field) in &spec.fields {
        if let Some(v) = values.get_mut(name) {
            let args = &spec.transform_args;
            *v = apply_transforms(v, field.transform(), args, base_url, tracking);
        }
    }

    let title = text::collapse_ws(values.get("title").map(String::as_str).unwrap_or(""));
    let mut url_str = text::trim(values.get("url").map(String::as_str).unwrap_or(""));
    if url_str.is_empty() || title.is_empty() {
        return None;
    }
    // 絶対化されていない相対 URL は破棄する（E4006 相当）
    if !(url_str.starts_with("http://") || url_str.starts_with("https://")) {
        if let Ok(abs) = base_url.join(&url_str) {
            url_str = abs.to_string();
        } else {
            return None;
        }
    }
    let parsed = url::parse(&url_str).ok()?;
    if url::validate_scheme(&parsed).is_err() {
        return None;
    }
    let normalized = url::normalize(&parsed, tracking).to_string();

    let snippet = values
        .get("snippet")
        .map(|s| text::collapse_ws(s))
        .filter(|s| !s.is_empty());
    let site = values
        .get("site")
        .map(|s| text::collapse_ws(s))
        .filter(|s| !s.is_empty())
        .or_else(|| url::host_of(&parsed));
    let published = values
        .get("published")
        .or_else(|| values.get("date"))
        .map(|s| text::trim(s))
        .filter(|s| !s.is_empty());

    Some(RawHit {
        title,
        url: normalized,
        snippet,
        site,
        published,
    })
}

fn apply_transforms(
    raw: &str,
    transforms: &[Transform],
    args: &TransformArgs,
    base_url: &url::Url,
    tracking: &[String],
) -> String {
    let mut value = raw.to_string();
    for t in transforms {
        value = match t {
            Transform::Trim => text::trim(&value),
            Transform::CollapseWs => text::collapse_ws(&value),
            Transform::StripHtml => strip_html(&value),
            Transform::DecodeEntities => text::decode_entities(&value),
            Transform::Absolutize => match base_url.join(value.trim()) {
                Ok(u) => u.to_string(),
                Err(_) => value,
            },
            Transform::UnwrapRedirect => unwrap_redirect(&value, &args.keys, base_url),
            Transform::StripTracking => match url::parse(&value) {
                Ok(u) => url::normalize(&u, tracking).to_string(),
                Err(_) => value,
            },
            Transform::Truncate => match args.chars {
                Some(n) => text::truncate_chars(&value, n),
                None => value,
            },
            Transform::Replace => match (&args.pattern, &args.with) {
                (Some(p), Some(w)) => match regex::Regex::new(p) {
                    Ok(re) => re.replace_all(&value, w.as_str()).to_string(),
                    Err(_) => value,
                },
                _ => value,
            },
            Transform::Lower => value.to_lowercase(),
            Transform::Upper => value.to_uppercase(),
            Transform::ToDate => normalize_date(&value, &args.formats),
        };
    }
    value
}

fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0usize;
    for c in s.chars() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    text::decode_entities(&out)
}

/// リダイレクト用 URL から実 URL を取り出す。
///
/// 検索エンジンは結果リンクを自前のホスト（`//relay.example/l/?uddg=...` など）へ
/// 包んで返す。そのまま取得するとエンジン側のページしか読めないため、
/// クエリの参照キーから実 URL を取り出す。
///
/// - プロトコル相対（`//host/path`）・相対パスは `base_url` で絶対化してから解釈する。
/// - 値は 1 段だけでなく、2 段まで入れ子に復号できるようにする
///   （エンジンのリダイレクトが別のリダイレクトを包む場合がある）。
/// - 候補キーは既定の一覧に加えて `keys` で指定できる。
fn unwrap_redirect(value: &str, keys: &[String], base_url: &url::Url) -> String {
    const MAX_DEPTH: usize = 2;
    let default_keys = [
        "uddg",
        "url",
        "u",
        "q",
        "target",
        "redirect",
        "redirect_url",
        "dest",
        "destination",
        "to",
        "link",
        "href",
    ];
    let candidates: Vec<&str> = if keys.is_empty() {
        default_keys.to_vec()
    } else {
        keys.iter().map(String::as_str).collect()
    };

    // 相対・プロトコル相対でも解釈できるよう、基準 URL で絶対化してから解析する
    let absolute = |s: &str| -> Option<url::Url> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return None;
        }
        if let Ok(parsed) = url::Url::parse(trimmed) {
            return Some(parsed);
        }
        base_url.join(trimmed).ok()
    };

    let mut current = match absolute(value) {
        Some(u) => u,
        None => return value.to_string(),
    };
    let mut result = value.to_string();
    for _ in 0..MAX_DEPTH {
        let Some(next) = extract_target(&current, &candidates).and_then(|v| absolute(&v)) else {
            break;
        };
        result = next.to_string();
        if next == current {
            break;
        }
        current = next;
    }
    result
}

/// クエリから参照キーの値（`http` / `https` の実 URL）を取り出す。
///
/// 値はパーセントエンコードされている場合と、素の URL がそのまま入っている
/// 場合があるため、復号してから再度判定する。
fn extract_target(url: &url::Url, candidates: &[&str]) -> Option<String> {
    for (key, value) in url.query_pairs() {
        if !candidates.iter().any(|c| key.eq_ignore_ascii_case(c)) {
            continue;
        }
        let decoded = value.trim().to_string();
        // 二重エンコード（`%253A`）にも 1 段だけ対応する
        let once = percent_decode(&decoded);
        for candidate in [decoded.as_str(), once.as_str()] {
            let candidate = candidate.trim();
            if candidate.starts_with("http://") || candidate.starts_with("https://") {
                return Some(candidate.to_string());
            }
            // エンジンが値をさらにリダイレクト URL で包んでいる場合
            if let Ok(inner) = url::Url::parse(candidate)
                && let Some(found) = extract_target(&inner, candidates)
            {
                return Some(found);
            }
        }
    }
    None
}

/// `%XX` を復号する（不正な並びはそのまま残す）。
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = &input[i + 1..i + 3];
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn normalize_date(value: &str, formats: &[String]) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let candidates: Vec<&str> = if formats.is_empty() {
        vec![
            "%Y-%m-%d",
            "%Y/%m/%d",
            "%Y年%m月%d日",
            "%d %b %Y",
            "%b %d, %Y",
            "%Y-%m-%dT%H:%M:%S%z",
            "%Y-%m-%dT%H:%M:%SZ",
            "%a, %d %b %Y %H:%M:%S %Z",
        ]
    } else {
        formats.iter().map(String::as_str).collect()
    };
    for f in &candidates {
        if let Ok(d) = chrono::NaiveDate::parse_from_str(trimmed, f) {
            return d.format("%Y-%m-%d").to_string();
        }
        if let Ok(dt) = chrono::DateTime::parse_from_str(trimmed, f) {
            return dt.date_naive().format("%Y-%m-%d").to_string();
        }
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(trimmed) {
            return dt.date_naive().format("%Y-%m-%d").to_string();
        }
        if let Ok(dt) = chrono::DateTime::parse_from_rfc2822(trimmed) {
            return dt.date_naive().format("%Y-%m-%d").to_string();
        }
    }
    // 先頭 10 文字が YYYY-MM-DD なら採用
    if trimmed.len() >= 10 && chrono::NaiveDate::parse_from_str(&trimmed[..10], "%Y-%m-%d").is_ok()
    {
        return trimmed[..10].to_string();
    }
    trimmed.to_string()
}

fn finalize(out: &mut ExtractOutcome) {
    if out.hits.is_empty() && out.dropped > 0 {
        out.warnings.push(Diagnostic::new(
            WarningCode::W4001,
            format!(
                "抽出に失敗した結果を {} 件破棄した（title / url が空）",
                out.dropped
            ),
        ));
    }
    if out.hits.is_empty() && out.items_seen == 0 {
        // 抽出規則が応答と一致していない可能性（呼び出し側で E4004 にする）
        return;
    }
    if out.hits.is_empty() {
        out.no_results = true;
    }
}

/// `items` の指定が応答種別と噛み合っているか（検証用）。
///
/// 応答種別による分岐は `extract` の 1 か所に閉じるため、ここでは
/// 「どちらかの指定があれば良い」という緩い判定にとどめる。
pub fn items_for(kind: ResponseKind, items: &ItemSpec) -> bool {
    let _ = kind;
    items.selector.is_some() || items.path.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::engine::EngineDef;

    fn def(json: serde_json::Value) -> EngineDef {
        serde_json::from_value(json).expect("definition")
    }

    fn base() -> url::Url {
        url::Url::parse("https://example.com/search?q=x").unwrap()
    }

    #[test]
    fn extracts_html_items() {
        let d = def(serde_json::json!({
            "schemaVersion": 1,
            "id": "t",
            "request": {"url": "https://example.com/s"},
            "response": {
                "kind": "html",
                "items": {"selector": "li.r"},
                "fields": {
                    "title": {"selector": "a.t", "transform": ["trim", "collapse_ws"]},
                    "url": {"selector": "a.t", "attr": "href", "transform": ["absolutize"]},
                    "snippet": {"selector": "span.s", "transform": ["strip_html", "collapse_ws"]}
                }
            }
        }));
        let body = r#"<html><body><ul>
            <li class="r"><a class="t" href="/a">  A  title </a><span class="s">snip <b>A</b></span></li>
            <li class="r"><a class="t" href="https://other.example/b">B</a></li>
            <li class="r"><span class="s">no title</span></li>
        </ul></body></html>"#;
        let out = extract(&d.response, body, &base(), &[]).unwrap();
        assert_eq!(out.hits.len(), 2);
        assert_eq!(out.hits[0].title, "A title");
        assert_eq!(out.hits[0].url, "https://example.com/a");
        assert_eq!(out.hits[0].snippet.as_deref(), Some("snip A"));
        assert_eq!(out.hits[1].url, "https://other.example/b");
        assert_eq!(out.dropped, 1);
    }

    #[test]
    fn extracts_json_items_and_paging() {
        let d = def(serde_json::json!({
            "schemaVersion": 1,
            "id": "t",
            "request": {"url": "https://example.com/api"},
            "response": {
                "kind": "json",
                "items": {"path": "results"},
                "fields": {
                    "title": {"path": "title", "transform": ["trim"]},
                    "url": {"path": "link", "transform": ["trim"]},
                    "published": {"path": "date", "transform": ["to_date"]}
                },
                "errorWhen": [{"path": "error", "exists": true}]
            }
        }));
        let body = r#"{"results":[
            {"title":" T1 ","link":"https://example.com/1","date":"2026/11/05"},
            {"title":"","link":"https://example.com/2"}
        ]}"#;
        let out = extract(&d.response, body, &base(), &[]).unwrap();
        assert_eq!(out.hits.len(), 1);
        assert_eq!(out.hits[0].title, "T1");
        assert_eq!(out.hits[0].published.as_deref(), Some("2026-11-05"));
        assert_eq!(out.dropped, 1);

        let err = r#"{"error":"rate limited","results":[]}"#;
        let out = extract(&d.response, err, &base(), &[]).unwrap();
        assert!(out.no_results);
    }

    #[test]
    fn strips_tracking_parameters() {
        let d = def(serde_json::json!({
            "schemaVersion": 1,
            "id": "t",
            "request": {"url": "https://example.com/api"},
            "response": {
                "kind": "json",
                "items": {"path": "r"},
                "fields": {
                    "title": {"path": "t"},
                    "url": {"path": "u", "transform": ["strip_tracking"]}
                }
            }
        }));
        let body = r#"{"r":[{"t":"x","u":"https://example.com/p?utm_source=a&id=2"}]}"#;
        let out = extract(&d.response, body, &base(), &[]).unwrap();
        assert_eq!(out.hits[0].url, "https://example.com/p?id=2");
    }

    /// 実データ相当: 結果リンクは `//relay.example/l/?uddg=<実 URL>` のような
    /// プロトコル相対のリダイレクト URL で返ることがある。`unwrap_redirect` が
    /// これを解けないと、取得時にエンジン側のページしか読めない。
    #[test]
    fn unwraps_protocol_relative_redirect() {
        let d = def(serde_json::json!({
            "schemaVersion": 1,
            "id": "t",
            "request": {"url": "https://relay.example/html/"},
            "response": {
                "kind": "html",
                "items": {"selector": "div.result"},
                "fields": {
                    "title": {"selector": "a.hit", "transform": ["collapse_ws"]},
                    "url": {
                        "selector": "a.hit",
                        "attr": "href",
                        "transform": ["trim", "unwrap_redirect", "strip_tracking", "absolutize"]
                    }
                }
            }
        }));
        let body = r#"<html><body>
            <div class="result">
              <a class="hit" href="//relay.example/l/?uddg=https%3A%2F%2Fexample.jp%2Farticle%2F1%3Fx%3D1&amp;rut=abc">Title A</a>
            </div>
            <div class="result">
              <a class="hit" href="https://relay.example/l/?uddg=https%3A%2F%2Fexample.jp%2Farticle%2F2">Title B</a>
            </div>
        </body></html>"#;
        let out = extract(&d.response, body, &base(), &[]).unwrap();
        assert_eq!(out.hits.len(), 2);
        assert_eq!(out.hits[0].url, "https://example.jp/article/1?x=1");
        assert_eq!(out.hits[1].url, "https://example.jp/article/2");
    }

    /// 入れ子のリダイレクト（エンジンが別のリダイレクト URL を包む）も解く。
    #[test]
    fn unwraps_nested_and_double_encoded_redirects() {
        let d = def(serde_json::json!({
            "schemaVersion": 1,
            "id": "t",
            "request": {"url": "https://example.com/search"},
            "response": {
                "kind": "html",
                "items": {"selector": "li"},
                "fields": {
                    "title": {"selector": "a"},
                    "url": {"selector": "a", "attr": "href", "transform": ["unwrap_redirect"]}
                }
            }
        }));
        // 1 段: 素の URL がクエリに入る / 2 段: リダイレクトがリダイレクトを包む
        let body = r#"<html><body><ul>
          <li><a href="https://relay.example/out?url=https%3A%2F%2Fexample.jp%2Fa">A</a></li>
          <li><a href="https://relay.example/out?url=https%3A%2F%2Frelay2.example%2Fgo%3Fu%3Dhttps%253A%252F%252Fexample.jp%252Fb">B</a></li>
          <li><a href="/local/page">C</a></li>
        </ul></body></html>"#;
        let out = extract(&d.response, body, &base(), &[]).unwrap();
        assert_eq!(out.hits[0].url, "https://example.jp/a");
        assert_eq!(out.hits[1].url, "https://example.jp/b");
        // リダイレクトを含まない相対リンクは基準 URL で絶対化するだけ
        assert_eq!(out.hits[2].url, "https://example.com/local/page");
    }

    #[test]
    fn json_path_supports_array_index() {
        let v: serde_json::Value =
            serde_json::from_str(r#"{"a":{"b":[{"c":1},{"c":2}]}}"#).unwrap();
        assert_eq!(json_path(&v, "a.b[1].c").unwrap(), &serde_json::json!(2));
        assert!(json_path(&v, "a.x").is_none());
    }
}
