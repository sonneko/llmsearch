//! エンジン定義の評価（リクエスト組み立て → 送信 → 抽出）。

use crate::core::config::NetworkProfile;
use crate::core::engine::EngineDef;
use crate::core::errors::{AppError, Diagnostic, ErrorCode, WarningCode};
use crate::core::extract::{self, ExtractOutcome};
use crate::core::secret::Secrets;
use crate::core::template::{self, TmplCtx};
use crate::ports::HttpClient;
use crate::ports::http::{HttpRequest, HttpResponse};

/// 1 回の評価結果。
#[derive(Debug, Clone)]
pub struct EngineCall {
    pub outcome: ExtractOutcome,
    pub request_url: String,
    pub status: u16,
    pub truncated: bool,
    pub elapsed_ms: u128,
}

/// エンジンを 1 回呼び出し、結果を抽出する。
pub fn evaluate(
    client: &dyn HttpClient,
    profile: &NetworkProfile,
    def: &EngineDef,
    ctx: &TmplCtx,
    secrets: &Secrets,
    tracking: &[String],
) -> Result<EngineCall, AppError> {
    let prepared = template::prepare(def, ctx, secrets)?;
    let url = crate::core::url::parse(&prepared.url)?;

    let mut request = HttpRequest::get(url.clone());
    request.method = prepared.method.clone();
    request.headers = prepared
        .headers
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    request.body = prepared.body.as_ref().map(|b| b.clone().into_bytes());
    request.timeout_ms = prepared.timeout_ms;
    request.secrets = secrets
        .names()
        .filter_map(|n| secrets.value_of(n))
        .map(|s| s.to_string())
        .collect();

    let started = std::time::Instant::now();
    let response: HttpResponse = client.send_blocking(profile, &request)?;
    let elapsed_ms = started.elapsed().as_millis();

    let (body, _had_errors) = decode(&response, def);
    let mut outcome = extract::extract(&def.response, &body, &response.final_url, tracking)?;
    if response.truncated {
        outcome.warnings.push(Diagnostic::new(
            WarningCode::W3003,
            "応答サイズの上限で打ち切った",
        ));
    }
    if outcome.hits.is_empty() && outcome.items_seen == 0 && !body.trim().is_empty() {
        // 抽出規則が合っていない可能性（先頭 2048 文字のみ示す。本文は出さない）
        let head: String = body.chars().take(2048).collect();
        outcome.warnings.push(Diagnostic::new(
            WarningCode::W4004,
            format!("応答が想定形式と異なる可能性: {}", head.replace('\n', " ")),
        ));
        return Err(AppError::with_detail(
            ErrorCode::E4004,
            "抽出規則に一致する要素が 1 件も無かった".to_string(),
        ));
    }

    Ok(EngineCall {
        outcome,
        request_url: prepared.url,
        status: response.status,
        truncated: response.truncated,
        elapsed_ms,
    })
}

fn decode(response: &HttpResponse, def: &EngineDef) -> (String, bool) {
    let charset = match &def.response.charset {
        Some(explicit) => explicit.clone(),
        None => {
            crate::ports::fetch::detect_charset(response.content_type(), &response.body, None).0
        }
    };
    crate::ports::fetch::decode_body(&response.body, &charset)
}

/// リクエストの組み立てのみ（`--dry-run`）。
pub fn dry_run_url(
    def: &EngineDef,
    ctx: &TmplCtx,
    secrets: &Secrets,
) -> Result<template::PreparedRequest, AppError> {
    template::prepare(def, ctx, secrets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::engine::EngineRegistry;

    struct FixedClient {
        body: Vec<u8>,
        status: u16,
    }

    impl HttpClient for FixedClient {
        fn send_blocking(
            &self,
            _profile: &NetworkProfile,
            request: &HttpRequest,
        ) -> Result<HttpResponse, AppError> {
            Ok(HttpResponse {
                status: self.status,
                final_url: request.url.clone(),
                headers: vec![("content-type".into(), "text/html; charset=utf-8".into())],
                body: self.body.clone(),
                truncated: false,
                redirects: 0,
            })
        }

        fn describe(&self, profile: &NetworkProfile) -> String {
            profile.id.clone()
        }
    }

    fn def() -> EngineDef {
        let mut reg = EngineRegistry::new();
        let value = serde_json::json!({
            "schemaVersion": 1,
            "id": "t",
            "request": {"url": "https://example.com/s", "query": {"q": "{{query}}", "n": "{{limit}}"}},
            "response": {
                "kind": "html",
                "items": {"selector": "li"},
                "fields": {
                    "title": {"selector": "a"},
                    "url": {"selector": "a", "attr": "href", "transform": ["absolutize"]}
                }
            }
        });
        reg.insert(serde_json::from_value(value).unwrap());
        reg.get("t").unwrap().clone()
    }

    #[test]
    fn builds_request_and_extracts() {
        let client = FixedClient {
            body: br#"<ul><li><a href="/x">X</a></li></ul>"#.to_vec(),
            status: 200,
        };
        let ctx = TmplCtx {
            query: "q".into(),
            limit: 7,
            ..Default::default()
        };
        let call = evaluate(
            &client,
            &NetworkProfile::default(),
            &def(),
            &ctx,
            &Secrets::new(),
            &[],
        )
        .unwrap();
        assert_eq!(call.outcome.hits.len(), 1);
        assert!(call.request_url.contains("q=q"));
        assert!(call.request_url.contains("n=7"));
    }

    #[test]
    fn unextractable_response_is_an_error() {
        let client = FixedClient {
            body: b"<p>nothing here</p>".to_vec(),
            status: 200,
        };
        let err = evaluate(
            &client,
            &NetworkProfile::default(),
            &def(),
            &TmplCtx::default(),
            &Secrets::new(),
            &[],
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::E4004);
    }
}
