//! `outline` / `read` の対象指定の解決。

use crate::core::errors::{AppError, ErrorCode};
use crate::core::session::SessionState;

/// 解決された対象。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Url(url::Url),
    ResultId(u32),
    DocumentId(u32),
}

impl Target {
    pub fn describe(&self) -> String {
        match self {
            Target::Url(u) => u.to_string(),
            Target::ResultId(id) => format!("R{id}"),
            Target::DocumentId(id) => format!("D{id}"),
        }
    }
}

/// 入力を解決する。
pub fn resolve(input: &str, session: Option<&SessionState>) -> Result<Target, AppError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(AppError::with_detail(ErrorCode::E1002, "対象が空"));
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return crate::core::url::parse(trimmed).map(Target::Url);
    }
    let normalized = trimmed
        .replace('Ｒ', "R")
        .replace('Ｄ', "D")
        .replace('ｒ', "r")
        .replace('ｄ', "d");
    let lower = normalized.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix('r') {
        let id = rest
            .trim()
            .parse::<u32>()
            .map_err(|_| AppError::with_detail(ErrorCode::E1002, trimmed.to_string()))?;
        if let Some(session) = session
            && session.find_result(id).is_none()
        {
            return Err(AppError::with_detail(
                ErrorCode::E1006,
                format!(
                    "R{id} は現行セッションに無い（利用可能: {})",
                    available_results(session)
                ),
            ));
        }
        return Ok(Target::ResultId(id));
    }
    if let Some(rest) = lower.strip_prefix('d') {
        let id = rest
            .trim()
            .parse::<u32>()
            .map_err(|_| AppError::with_detail(ErrorCode::E1002, trimmed.to_string()))?;
        if let Some(session) = session
            && session.find_document(id).is_none()
        {
            return Err(AppError::with_detail(
                ErrorCode::E1006,
                format!(
                    "D{id} は現行セッションに無い（利用可能: {})",
                    available_documents(session)
                ),
            ));
        }
        return Ok(Target::DocumentId(id));
    }
    if let Ok(n) = trimmed.parse::<u32>() {
        if let Some(session) = session
            && session.find_result(n).is_none()
        {
            return Err(AppError::with_detail(
                ErrorCode::E1006,
                format!(
                    "{n} は現行セッションに無い（利用可能: {})",
                    available_results(session)
                ),
            ));
        }
        return Ok(Target::ResultId(n));
    }
    Err(AppError::with_detail(
        ErrorCode::E1002,
        format!("{trimmed} を対象として解釈できない"),
    ))
}

fn available_results(session: &SessionState) -> String {
    let ids: Vec<String> = session.results.iter().take(20).map(|r| r.label()).collect();
    if ids.is_empty() {
        "（なし）".to_string()
    } else {
        ids.join(", ")
    }
}

fn available_documents(session: &SessionState) -> String {
    let ids: Vec<String> = session
        .documents
        .iter()
        .take(20)
        .map(|d| format!("D{}", d.id))
        .collect();
    if ids.is_empty() {
        "（なし）".to_string()
    } else {
        ids.join(", ")
    }
}

/// 対象の URL を求める（セッション参照を含む）。
pub fn to_url(target: &Target, session: Option<&SessionState>) -> Result<url::Url, AppError> {
    match target {
        Target::Url(u) => Ok(u.clone()),
        Target::ResultId(id) => {
            let session = session.ok_or_else(|| AppError::new(ErrorCode::E5001))?;
            let hit = session
                .find_result(*id)
                .ok_or_else(|| AppError::with_detail(ErrorCode::E1006, format!("R{id}")))?;
            crate::core::url::parse(&hit.url)
        }
        Target::DocumentId(id) => {
            let session = session.ok_or_else(|| AppError::new(ErrorCode::E5001))?;
            let doc = session
                .find_document(*id)
                .ok_or_else(|| AppError::with_detail(ErrorCode::E1006, format!("D{id}")))?;
            crate::core::url::parse(&doc.url)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::SearchHit;

    fn session() -> SessionState {
        let mut s = SessionState::new("s", chrono::Utc::now());
        s.results.push(SearchHit {
            id: 1,
            rank: 1,
            title: "t".into(),
            url: "https://example.com/a".into(),
            site: None,
            snippet: None,
            published: None,
            engine: "e".into(),
            language: None,
        });
        s
    }

    #[test]
    fn resolves_url_and_ids() {
        let s = session();
        assert!(matches!(
            resolve("https://example.com/x", Some(&s)).unwrap(),
            Target::Url(_)
        ));
        assert_eq!(resolve("R1", Some(&s)).unwrap(), Target::ResultId(1));
        assert_eq!(resolve("1", Some(&s)).unwrap(), Target::ResultId(1));
    }

    #[test]
    fn missing_id_is_e1006() {
        let s = session();
        let err = resolve("R9", Some(&s)).unwrap_err();
        assert_eq!(err.code, ErrorCode::E1006);
    }

    #[test]
    fn to_url_reads_session() {
        let s = session();
        let url = to_url(&Target::ResultId(1), Some(&s)).unwrap();
        assert_eq!(url.as_str(), "https://example.com/a");
    }
}
