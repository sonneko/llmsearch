//! 履歴の参照・エクスポート。

use crate::core::errors::AppError;
use crate::core::history::HistoryEntry;
use crate::ports::state::HistoryQuery;
use crate::usecases::App;

/// 一覧（新しい順）。`limit` が `None` の場合は全件を返す（`history export` が
/// 201 件目以降を黙って落とさないようにするため）。
pub fn list(
    app: &App,
    limit: Option<usize>,
    session: Option<&str>,
) -> Result<Vec<HistoryEntry>, AppError> {
    app.history.query(&HistoryQuery {
        limit,
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
    let _ = app.append_history(&entry);
    Ok(count)
}
