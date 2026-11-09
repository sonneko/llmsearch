//! 履歴（追記専用 JSON Lines）。

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::adapters::state::StatePaths;
use crate::core::errors::{AppError, ErrorCode};
use crate::core::history::{Filter, HistoryEntry};
use crate::ports::state::{HistoryQuery, HistoryStore};

/// `history.jsonl` を使う履歴。
/// 保持期間の既定（日）。仕様の `history.retentionDays` と一致させる。
pub const HISTORY_RETENTION_DEFAULT: u64 = 90;
/// 上限件数の既定。仕様の `history.maxEntries` と一致させる。
pub const HISTORY_MAX_ENTRIES_DEFAULT: usize = 50_000;

/// 履歴ファイル 1 本。
pub struct JsonlHistory {
    path: PathBuf,
    skipped: std::sync::atomic::AtomicUsize,
    /// 保持期間（日、0 は無期限）。
    retention_days: u64,
    /// 上限件数。
    max_entries: usize,
    clock: std::sync::Arc<dyn crate::ports::Clock>,
}

impl JsonlHistory {
    pub fn new(paths: StatePaths) -> Self {
        Self {
            path: paths.history_file(),
            skipped: std::sync::atomic::AtomicUsize::new(0),
            retention_days: HISTORY_RETENTION_DEFAULT,
            max_entries: HISTORY_MAX_ENTRIES_DEFAULT,
            clock: std::sync::Arc::new(crate::ports::system::SystemClock::new()),
        }
    }

    pub fn with_path(path: PathBuf) -> Self {
        Self {
            path,
            skipped: std::sync::atomic::AtomicUsize::new(0),
            retention_days: HISTORY_RETENTION_DEFAULT,
            max_entries: HISTORY_MAX_ENTRIES_DEFAULT,
            clock: std::sync::Arc::new(crate::ports::system::SystemClock::new()),
        }
    }

    /// 判定に使う時計を差し替える（テストで固定するため）。
    pub fn with_clock(mut self, clock: std::sync::Arc<dyn crate::ports::Clock>) -> Self {
        self.clock = clock;
        self
    }

    /// 保持期間（日）と上限件数を設定する。0 日は無期限。
    pub fn with_limits(mut self, retention_days: u64, max_entries: usize) -> Self {
        self.retention_days = retention_days;
        self.max_entries = max_entries;
        self
    }

    /// 保持期間と上限を適用し、削除した件数を返す。
    ///
    /// 追記のたびに全件を読み直すのは高くつくため、一定件数ごとに間引く。
    fn prune(&self) -> Result<usize, AppError> {
        let all = self.read_all()?;
        let now = self.clock.now_utc();
        let cutoff = if self.retention_days == 0 {
            None
        } else {
            Some(now - chrono::Duration::days(self.retention_days as i64))
        };
        let mut kept: Vec<HistoryEntry> = all
            .into_iter()
            .filter(
                |e| match (cutoff, chrono::DateTime::parse_from_rfc3339(&e.at)) {
                    (Some(cutoff), Ok(at)) => at.with_timezone(&chrono::Utc) >= cutoff,
                    _ => true,
                },
            )
            .collect();
        if kept.len() > self.max_entries {
            let excess = kept.len() - self.max_entries;
            kept.drain(0..excess);
        } else if cutoff.is_none() {
            return Ok(0);
        }
        let removed = self.read_all()?.len().saturating_sub(kept.len());
        if removed == 0 {
            return Ok(0);
        }
        let mut text = String::new();
        for entry in &kept {
            let line = serde_json::to_string(entry)
                .map_err(|e| AppError::with_detail(ErrorCode::E5004, format!("直列化: {e}")))?;
            text.push_str(&line);
            text.push('\n');
        }
        crate::adapters::state::write_atomic(&self.path, text.as_bytes())?;
        Ok(removed)
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
}

impl HistoryStore for JsonlHistory {
    fn next_id(&self) -> Result<String, AppError> {
        let all = self.read_all()?;
        let max = all
            .iter()
            .filter_map(|e| e.id.trim_start_matches('H').parse::<u64>().ok())
            .max()
            .unwrap_or(0);
        Ok(format!("H{}", max + 1))
    }

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
        })?;
        // 保持期間・上限の適用。失敗しても追記自体は成功しているので握りつぶす
        let _ = self.prune();
        Ok(())
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

/// 書き出した JSON Lines を読み戻す（`history` の退避・移行で使う）。
///
/// 引数は読み込み元のパスで、戻り値は読めた件数。壊れた行は数えずに飛ばし、
/// 飛ばした件数を返り値の 2 要素目で知らせる（`W5004` の対象）。
pub fn import_lines(path: &Path, sink: &mut Vec<HistoryEntry>) -> std::io::Result<usize> {
    let text = std::fs::read_to_string(path)?;
    let mut skipped = 0usize;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<HistoryEntry>(line) {
            Ok(entry) => sink.push(entry),
            Err(_) => skipped += 1,
        }
    }
    Ok(skipped)
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

    fn dated(id: &str, at: &str) -> HistoryEntry {
        HistoryEntry::new(
            id,
            chrono::DateTime::parse_from_rfc3339(at)
                .unwrap()
                .with_timezone(&chrono::Utc),
            "s1",
            EventType::Search,
            serde_json::json!({}),
        )
    }

    #[test]
    fn prunes_by_retention_days() {
        struct At(chrono::DateTime<chrono::Utc>);
        impl crate::ports::Clock for At {
            fn now_utc(&self) -> chrono::DateTime<chrono::Utc> {
                self.0
            }
            fn today_local(&self) -> chrono::NaiveDate {
                self.0.date_naive()
            }
            fn monotonic_ns(&self) -> u64 {
                0
            }
            fn sleep(&self, _: std::time::Duration) {}
        }
        let dir = tempfile::tempdir().unwrap();
        let store = JsonlHistory::with_path(dir.path().join("history.jsonl"))
            .with_limits(30, 100)
            .with_clock(std::sync::Arc::new(At(
                chrono::DateTime::parse_from_rfc3339("2026-11-07T00:00:00Z")
                    .unwrap()
                    .with_timezone(&chrono::Utc),
            )));
        store.append(&dated("H1", "2026-01-01T00:00:00Z")).unwrap();
        store.append(&dated("H2", "2026-11-01T00:00:00Z")).unwrap();
        let all = store.read_all().unwrap();
        assert_eq!(all.len(), 1, "保持期間を超えた行が残っている: {all:?}");
        assert_eq!(all[0].id, "H2");
    }

    #[test]
    fn prunes_by_max_entries() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonlHistory::with_path(dir.path().join("history.jsonl"))
            .with_limits(0, 3)
            .with_clock(std::sync::Arc::new(crate::ports::system::SystemClock::new()));
        for i in 1..=4 {
            let at = format!("2026-11-0{}T00:00:00Z", i + 4);
            store.append(&dated(&format!("H{i}"), &at)).unwrap();
        }
        let all = store.read_all().unwrap();
        assert_eq!(all.len(), 3, "上限件数が効いていない: {all:?}");
        assert!(all.iter().all(|e| e.id != "H1"), "古い行が残っている");
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
