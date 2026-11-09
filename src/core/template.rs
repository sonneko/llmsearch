//! リクエストのテンプレート展開と組み立て。

use std::collections::BTreeMap;

use url::Url;

use crate::core::engine::{EngineDef, PagingSpec, PagingStyle, TmplValue};
use crate::core::errors::{AppError, ErrorCode};

/// テンプレート変数の値。
#[derive(Debug, Clone, Default)]
pub struct TmplCtx {
    pub query: String,
    pub query_raw: String,
    pub offset: u32,
    pub page: u32,
    pub limit: u32,
    pub locale: String,
    pub region: String,
    pub api_key: Option<String>,
    pub date_from: String,
    pub date_to: String,
}

impl TmplCtx {
    fn lookup(&self, name: &str) -> Option<String> {
        Some(match name {
            "query" => self.query.clone(),
            "queryRaw" => self.query_raw.clone(),
            "offset" => self.offset.to_string(),
            "page" => self.page.to_string(),
            "limit" => self.limit.to_string(),
            "locale" => self.locale.clone(),
            "region" => self.region.clone(),
            "apiKey" => self.api_key.clone().unwrap_or_default(),
            "dateFrom" => self.date_from.clone(),
            "dateTo" => self.date_to.clone(),
            _ => return None,
        })
    }
}

/// `{{name}}` を 1 パスで置換する（値は URL エンコードしない）。
pub fn expand(tmpl: &str, ctx: &TmplCtx) -> Result<String, AppError> {
    let mut out = String::with_capacity(tmpl.len());
    let mut rest = tmpl;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find("}}") {
            Some(end) => {
                let name = after[..end].trim();
                match ctx.lookup(name) {
                    Some(v) => out.push_str(&v),
                    None => {
                        return Err(AppError::with_detail(
                            ErrorCode::E1008,
                            format!("{{{{{name}}}}}"),
                        ));
                    }
                }
                rest = &after[end + 2..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    Ok(out)
}

/// 指定した文字符号化でクエリ文字列をパーセントエンコードする。
pub fn encode_query(s: &str, encoding: &str) -> String {
    let encoding = encoding.to_ascii_lowercase();
    if encoding == "utf-8" || encoding == "utf8" {
        return url::form_urlencoded::byte_serialize(s.as_bytes()).collect();
    }
    let enc = match encoding.as_str() {
        "euc-jp" | "eucjp" => encoding_rs::EUC_JP,
        "shift_jis" | "shift-jis" | "sjis" => encoding_rs::SHIFT_JIS,
        "gbk" => encoding_rs::GBK,
        "big5" => encoding_rs::BIG5,
        "euc-kr" | "euckr" => encoding_rs::EUC_KR,
        _ => encoding_rs::UTF_8,
    };
    let (bytes, _, _) = enc.encode(s);
    bytes
        .iter()
        .map(|b| match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b' ' => {
                if *b == b' ' {
                    '+'.to_string()
                } else {
                    (*b as char).to_string()
                }
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// 組み立てたリクエスト。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedRequest {
    pub method: String,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub body: Option<String>,
    pub timeout_ms: Option<u64>,
}

/// リクエストを組み立てる。
pub fn prepare(
    def: &EngineDef,
    ctx: &TmplCtx,
    secrets: &crate::core::secret::Secrets,
) -> Result<PreparedRequest, AppError> {
    let req = &def.request;
    let base = resolve_tmpl(&TmplValue::Value(req.url.clone()), ctx, secrets)?;
    let mut url = Url::parse(&base).map_err(|e| {
        AppError::with_detail(ErrorCode::E1004, format!("request.url を解釈できない: {e}"))
    })?;
    crate::core::url::validate_scheme(&url)?;

    // クエリ（BTreeMap のため順序は固定）
    {
        let mut pairs = url.query_pairs_mut();
        for (k, v) in &req.query {
            let raw = resolve_tmpl(v, ctx, secrets)?;
            let encoded = encode_query(&raw, &req.encoding);
            // `query_pairs_mut` はさらにエンコードするため、生の値を渡す
            let _ = encoded;
            pairs.append_pair(k, &raw);
        }
    }

    let mut headers = BTreeMap::new();
    for (k, v) in &req.headers {
        headers.insert(k.clone(), resolve_tmpl(v, ctx, secrets)?);
    }

    let body = match &req.body {
        Some(b) => Some(expand(b, ctx)?),
        None => None,
    };
    if let Some(ct) = &req.content_type {
        headers.insert("Content-Type".into(), ct.clone());
    }

    Ok(PreparedRequest {
        method: req.method.to_ascii_uppercase(),
        url: url.to_string(),
        headers,
        body,
        timeout_ms: req.timeout_ms,
    })
}

fn resolve_tmpl(
    v: &TmplValue,
    ctx: &TmplCtx,
    secrets: &crate::core::secret::Secrets,
) -> Result<String, AppError> {
    match v {
        TmplValue::Value(s) => expand(s, ctx),
        TmplValue::Explicit { value } => Ok(value.clone()),
        TmplValue::Env { value_env } => match secrets.get(value_env) {
            Some(secret) => Ok(secret.expose().to_string()),
            None => Err(AppError::with_detail(
                ErrorCode::E4002,
                format!("環境変数 {value_env} が未設定"),
            )),
        },
    }
}

/// ページング位置からリクエスト用の値を計算する。
pub fn paging_values(paging: Option<&PagingSpec>, page: u32, limit: u32) -> (u32, u32) {
    // 戻り値: (offset, page_number)
    match paging {
        None => (0, 1),
        Some(p) => match p.style {
            PagingStyle::Offset => {
                let step = p
                    .offset_step
                    .unwrap_or_else(|| p.page_size.unwrap_or(limit));
                let offset = (page.saturating_sub(1)) * step;
                (offset, page)
            }
            PagingStyle::Page => (0, p.page_base + page.saturating_sub(1)),
        },
    }
}

/// 次ページ以降が存在しうるか。
pub fn has_more(paging: Option<&PagingSpec>, page: u32, limit: u32, max_results: usize) -> bool {
    let consumed = (page as usize) * (limit as usize);
    if consumed >= max_results {
        return false;
    }
    match paging {
        None => false,
        Some(p) => {
            let max_page = p.page_size.unwrap_or(limit).max(1);
            consumed + (max_page as usize) <= p.max_results as usize
                || consumed < p.max_results as usize
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_variables() {
        let ctx = TmplCtx {
            query: "a b".into(),
            limit: 7,
            ..Default::default()
        };
        assert_eq!(
            expand("q={{query}}&n={{limit}}", &ctx).unwrap(),
            "q=a b&n=7"
        );
    }

    #[test]
    fn unknown_variable_is_error() {
        let ctx = TmplCtx::default();
        let err = expand("{{nope}}", &ctx).unwrap_err();
        assert_eq!(err.code, ErrorCode::E1008);
    }

    #[test]
    fn encodes_euc_jp() {
        let out = encode_query("日本", "euc-jp");
        assert!(out.starts_with("%C6%FC"), "{out}");
    }

    #[test]
    fn paging_offset_and_page() {
        let p = PagingSpec {
            style: PagingStyle::Offset,
            offset_step: Some(10),
            ..Default::default()
        };
        assert_eq!(paging_values(Some(&p), 2, 7), (10, 2));
        let p2 = PagingSpec {
            style: PagingStyle::Page,
            page_base: 1,
            ..Default::default()
        };
        assert_eq!(paging_values(Some(&p2), 3, 7), (0, 3));
    }
}
