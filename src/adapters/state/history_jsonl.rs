//! 履歴（追記専用 JSON Lines）。

use std::io::Write;
use std::path::PathBuf;

use crate::adapters::state::StatePaths;
use crate::core::errors::{AppError, ErrorCode};
use crate::core::history::{Filter, HistoryEntry};
use crate::ports::state::{HistoryQuery, HistoryStore};

/// `history.jsonl` を使う履歴。
pub struct JsonlHistory {
    path: PathBuf,
    skipped: std::sync::atomic::AtomicUsize,
}

impl JsonlHistory {
    pub fn new(paths: StatePaths) -> Self {
        Self {
            path: paths.history_file(),
            skipped: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    pub fn with_path(path: PathBuf) -> Self {
        Self {
            path,
            skipped: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// 全レコード（壊れた行は数えてスキップ）。
    pub fn read_all(&self) -> Result<Vec<HistoryEntry>, AppError> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => {
                return Err(AppError::with_detail(
                    ErrorCode::E5004,
                    format!("{}: {e}", self.path.display()),
                ));
            }
        };
        let mut out = Vec::new();
        let mut skipped = 0usize;
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<HistoryEntry>(line) {
                Ok(entry) => out.push(entry),
                Err(_) => skipped += 1,
            }
        }
        self.skipped
            .store(skipped, std::sync::atomic::Ordering::Relaxed);
        Ok(out)
    }

    /// 次の識別子（`H<n>`）。
    pub fn next_id(&self) -> Result<String, AppError> {
        let all = self.read_all()?;
        let max = all
            .iter()
            .filter_map(|e| e.id.trim_start_matches('H').parse::<u64>().ok())
            .max()
            .unwrap_or(0);
        Ok(format!("H{}", max + 1))
    }
}

impl HistoryStore for JsonlHistory {
    fn append(&self, entry: &HistoryEntry) -> Result<(), AppError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                AppError::with_detail(ErrorCode::E5002, format!("{}: {e}", parent.display()))
            })?;
        }
        let line = serde_json::to_string(entry)
            .map_err(|e| AppError::with_detail(ErrorCode::E5004, format!("直列化: {e}")))?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|e| {
                AppError::with_detail(ErrorCode::E5002, format!("{}: {e}", self.path.display()))
            })?;
        writeln!(file, "{line}").map_err(|e| {
            AppError::with_detail(ErrorCode::E5002, format!("{}: {e}", self.path.display()))
        })
    }

    fn query(&self, q: &HistoryQuery) -> Result<Vec<HistoryEntry>, AppError> {
        let all = self.read_all()?;
        let kind = q
            .kinds
            .iter()
            .find_map(|k| crate::core::history::EventType::parse(k));
        let filter = Filter {
            query: q.query.as_deref(),
            kind,
            session: q.session.as_deref(),
            engine: None,
            before: q.before.as_deref(),
            after: None,
        };
        let mut out: Vec<HistoryEntry> = all.into_iter().filter(|e| filter.matches(e)).collect();
        // 最新を先頭へ
        out.reverse();
        if let Some(limit) = q.limit {
            out.truncate(limit.min(200));
        }
        Ok(out)
    }

    fn clear(&self, before: Option<&str>) -> Result<usize, AppError> {
        let all = self.read_all()?;
        let kept: Vec<HistoryEntry> = match before {
            None => Vec::new(),
            Some(stamp) => all.into_iter().filter(|e| e.at.as_str() >= stamp).collect(),
        };
        let text: String = kept
            .iter()
            .filter_map(|e| serde_json::to_string(e).ok())
            .map(|l| format!("{l}\n"))
            .collect();
        crate::adapters::state::write_atomic(&self.path, text.as_bytes())?;
        Ok(0)
    }

    fn skipped_lines(&self) -> usize {
        self.skipped.load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::history::EventType;

    fn entry(id: &str, kind: EventType, payload: serde_json::Value) -> HistoryEntry {
        HistoryEntry::new(
            id,
            chrono::DateTime::parse_from_rfc3339("2026-11-07T05:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            "s1",
            kind,
            payload,
        )
    }

    #[test]
    fn appends_and_reads_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonlHistory::with_path(dir.path().join("history.jsonl"));
        store
            .append(&entry(
                "H1",
                EventType::Search,
                serde_json::json!({"query":"a"}),
            ))
            .unwrap();
        store
            .append(&entry(
                "H2",
                EventType::Read,
                serde_json::json!({"section":"D1:S1"}),
            ))
            .unwrap();
        let all = store.query(&HistoryQuery::default()).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].id, "H2");
        assert_eq!(store.next_id().unwrap(), "H3");
    }

    #[test]
    fn skips_corrupt_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.jsonl");
        std::fs::write(&path, "{\"broken\":true}\n").unwrap();
        let store = JsonlHistory::with_path(path);
        assert!(store.read_all().unwrap().is_empty());
        assert_eq!(store.skipped_lines(), 1);
    }

    #[test]
    fn filters_by_kind() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonlHistory::with_path(dir.path().join("h.jsonl"));
        store
            .append(&entry(
                "H1",
                EventType::Search,
                serde_json::json!({"query":"a"}),
            ))
            .unwrap();
        store
            .append(&entry(
                "H2",
                EventType::Read,
                serde_json::json!({"section":"D1:S1"}),
            ))
            .unwrap();
        let out = store
            .query(&HistoryQuery {
                kinds: vec!["read".into()],
                ..Default::default()
            })
            .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, EventType::Read);
    }
}
