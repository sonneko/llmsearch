//! 履歴の参照・エクスポート。

use crate::core::errors::AppError;
use crate::core::history::{HistoryEntry, export_markdown};
use crate::ports::state::HistoryQuery;
use crate::usecases::App;

/// 一覧（新しい順）。
pub fn list(
    app: &App,
    limit: Option<usize>,
    session: Option<&str>,
) -> Result<Vec<HistoryEntry>, AppError> {
    app.history.query(&HistoryQuery {
        limit: Some(limit.unwrap_or(20)),
        session: session.map(|s| s.to_string()),
        ..Default::default()
    })
}

/// 絞り込み検索。
pub fn search(
    app: &App,
    query: Option<&str>,
    kinds: &[String],
    session: Option<&str>,
    before: Option<&str>,
    limit: Option<usize>,
) -> Result<Vec<HistoryEntry>, AppError> {
    app.history.query(&HistoryQuery {
        limit: Some(limit.unwrap_or(50)),
        query: query.map(|s| s.to_string()),
        kinds: kinds.to_vec(),
        session: session.map(|s| s.to_string()),
        before: before.map(|s| s.to_string()),
    })
}

/// 1 件を識別子で取得する。
pub fn get(app: &App, id: &str) -> Result<Option<HistoryEntry>, AppError> {
    let all = app.history.query(&HistoryQuery {
        limit: None,
        ..Default::default()
    })?;
    Ok(all.into_iter().find(|e| e.id == id))
}

/// Markdown 形式で出力する。
pub fn export(app: &App, limit: Option<usize>) -> Result<String, AppError> {
    let mut entries = list(app, limit.or(Some(200)), None)?;
    entries.reverse();
    Ok(export_markdown(&entries))
}

/// 全削除。
pub fn clear(app: &App, before: Option<&str>) -> Result<usize, AppError> {
    let count = app.history.clear(before)?;
    let entry = HistoryEntry::new(
        app.history.next_id().unwrap_or_else(|_| "H?".to_string()),
        app.clock.now_utc(),
        app.current_session_id(),
        crate::core::history::EventType::Delete,
        serde_json::json!({"target": "history", "count": count, "before": before}),
    );
    let _ = app.history.append(&entry);
    Ok(count)
}
