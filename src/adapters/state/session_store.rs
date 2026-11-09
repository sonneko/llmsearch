//! セッション・文書・カウンタのファイル実装。

use std::path::PathBuf;
use url::Url;

use crate::adapters::state::{StatePaths, quarantine, write_atomic};
use crate::core::config::quota::Counters;
use crate::core::errors::{AppError, ErrorCode};
use crate::core::model::DocumentMeta;
use crate::core::session::SessionState;
use crate::ports::state::{CounterStore, DocumentStore, SessionStore};

/// セッション JSON の保存・読み込み。
pub struct FsSessionStore {
    paths: StatePaths,
    now: std::sync::Arc<dyn crate::ports::Clock>,
}

impl FsSessionStore {
    /// 退避ファイル名に使う時刻は `Clock` から取る（テストで固定できるようにするため）。
    pub fn new(paths: StatePaths, clock: std::sync::Arc<dyn crate::ports::Clock>) -> Self {
        Self { paths, now: clock }
    }

    fn file(&self, id: &str) -> PathBuf {
        self.paths.sessions().join(format!("{id}.json"))
    }
}

impl SessionStore for FsSessionStore {
    fn load(&self, id: &str) -> Result<Option<SessionState>, AppError> {
        let path = self.file(id);
        if !path.is_file() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(&path).map_err(|e| {
            AppError::with_detail(ErrorCode::E5002, format!("{}: {e}", path.display()))
        })?;
        match serde_json::from_str::<SessionState>(&text) {
            Ok(session) => Ok(Some(session)),
            Err(e) => {
                let stamp = self.now.now_utc().format("%Y%m%d%H%M%S").to_string();
                let moved = quarantine(&path, &stamp);
                Err(AppError::with_detail(
                    ErrorCode::E5001,
                    format!(
                        "{} を読めない（{e}）。退避先: {}",
                        path.display(),
                        moved
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| "（退避できず）".to_string())
                    ),
                ))
            }
        }
    }

    fn save(&self, session: &SessionState) -> Result<(), AppError> {
        self.paths.ensure()?;
        let path = self.file(&session.session_id);
        let text = serde_json::to_vec_pretty(session)
            .map_err(|e| AppError::with_detail(ErrorCode::E5002, format!("直列化: {e}")))?;
        write_atomic(&path, &text)
    }

    fn delete(&self, id: &str) -> Result<(), AppError> {
        let path = self.file(id);
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(AppError::with_detail(
                ErrorCode::E5002,
                format!("{}: {e}", path.display()),
            )),
        }
    }

    fn list(&self) -> Result<Vec<String>, AppError> {
        let dir = self.paths.sessions();
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => {
                return Err(AppError::with_detail(
                    ErrorCode::E5002,
                    format!("{}: {e}", dir.display()),
                ));
            }
        };
        let mut out: Vec<String> = entries
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let path = e.path();
                if path.extension().map(|x| x == "json").unwrap_or(false) {
                    path.file_stem().map(|s| s.to_string_lossy().to_string())
                } else {
                    None
                }
            })
            .collect();
        out.sort();
        Ok(out)
    }
}

/// 文書キャッシュのファイル実装。
pub struct FsDocumentStore {
    paths: StatePaths,
    /// 合計サイズの上限（バイト、0 は無制限）。
    max_bytes: u64,
}

impl FsDocumentStore {
    pub fn new(paths: StatePaths) -> Self {
        Self {
            paths,
            max_bytes: 0,
        }
    }

    /// 合計サイズの上限を設定する（バイト、0 は無制限）。
    pub fn with_max_bytes(mut self, max_bytes: u64) -> Self {
        self.max_bytes = max_bytes;
        self
    }

    fn key(&self, url: &Url) -> String {
        crate::core::session::cache_key(url.as_str())
    }

    /// 上限を超えていれば、文書を古い順（`fetched_at` 昇順）に削除する。
    ///
    /// セッションと履歴は削除対象にしない（仕様 §4.4）。削除した件数を返す。
    pub fn evict_to_limit(&self) -> Result<usize, AppError> {
        if self.max_bytes == 0 || self.paths.total_bytes() <= self.max_bytes {
            return Ok(0);
        }
        let dir = self.paths.documents();
        let mut docs: Vec<(String, std::path::PathBuf)> = Vec::new();
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Ok(0);
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let fetched_at = std::fs::read_to_string(path.join("meta.json"))
                .ok()
                .and_then(|t| serde_json::from_str::<DocumentMeta>(&t).ok())
                .map(|m| m.fetched_at)
                .unwrap_or_default();
            docs.push((fetched_at, path));
        }
        docs.sort();
        let mut removed = 0usize;
        for (_, path) in docs {
            if self.paths.total_bytes() <= self.max_bytes {
                break;
            }
            if std::fs::remove_dir_all(&path).is_ok() {
                removed += 1;
            }
        }
        Ok(removed)
    }
}

impl DocumentStore for FsDocumentStore {
    fn get(&self, url: &Url) -> Result<Option<(DocumentMeta, String)>, AppError> {
        let dir = self.paths.documents().join(self.key(url));
        let meta_path = dir.join("meta.json");
        let body_path = dir.join("body.md");
        if !meta_path.is_file() || !body_path.is_file() {
            return Ok(None);
        }
        let meta_text = std::fs::read_to_string(&meta_path).map_err(|e| {
            AppError::with_detail(ErrorCode::E5002, format!("{}: {e}", meta_path.display()))
        })?;
        let meta: DocumentMeta = match serde_json::from_str(&meta_text) {
            Ok(m) => m,
            Err(_) => return Ok(None),
        };
        let body = std::fs::read_to_string(&body_path).map_err(|e| {
            AppError::with_detail(ErrorCode::E5002, format!("{}: {e}", body_path.display()))
        })?;
        Ok(Some((meta, body)))
    }

    fn put(&self, meta: &DocumentMeta, markdown: &str) -> Result<(), AppError> {
        self.paths.ensure()?;
        let dir = self.paths.documents().join(
            self.key(
                &Url::parse(&meta.url)
                    .map_err(|e| AppError::with_detail(ErrorCode::E5002, format!("URL: {e}")))?,
            ),
        );
        std::fs::create_dir_all(&dir).map_err(|e| {
            AppError::with_detail(ErrorCode::E5002, format!("{}: {e}", dir.display()))
        })?;
        let meta_text = serde_json::to_vec_pretty(meta)
            .map_err(|e| AppError::with_detail(ErrorCode::E5002, format!("直列化: {e}")))?;
        write_atomic(&dir.join("meta.json"), &meta_text)?;
        write_atomic(&dir.join("body.md"), markdown.as_bytes())?;
        // 上限を超えた分は古い文書から落とす
        let _ = self.evict_to_limit();
        Ok(())
    }

    fn invalidate(&self, url: &Url) -> Result<(), AppError> {
        let dir = self.paths.documents().join(self.key(url));
        if dir.is_dir() {
            let _ = std::fs::remove_dir_all(&dir);
        }
        Ok(())
    }

    fn total_bytes(&self) -> Result<u64, AppError> {
        Ok(self.paths.total_bytes())
    }
}

/// クォータカウンタのファイル実装。
pub struct FsCounters {
    paths: StatePaths,
    now: std::sync::Arc<dyn crate::ports::Clock>,
}

impl FsCounters {
    pub fn new(paths: StatePaths, clock: std::sync::Arc<dyn crate::ports::Clock>) -> Self {
        Self { paths, now: clock }
    }
}

impl CounterStore for FsCounters {
    fn load(&self) -> Result<Counters, AppError> {
        let path = self.paths.counters_file();
        if !path.is_file() {
            return Ok(Counters::default());
        }
        let text = std::fs::read_to_string(&path).map_err(|e| {
            AppError::with_detail(ErrorCode::E5002, format!("{}: {e}", path.display()))
        })?;
        match serde_json::from_str(&text) {
            Ok(c) => Ok(c),
            Err(_) => {
                let stamp = self.now.now_utc().format("%Y%m%d%H%M%S").to_string();
                let _ = quarantine(&path, &stamp);
                Ok(Counters::default())
            }
        }
    }

    fn save(&self, counters: &Counters) -> Result<(), AppError> {
        self.paths.ensure()?;
        let text = serde_json::to_vec_pretty(counters)
            .map_err(|e| AppError::with_detail(ErrorCode::E5002, format!("直列化: {e}")))?;
        write_atomic(&self.paths.counters_file(), &text)
    }

    fn reset(&self, scope: &str) -> Result<(), AppError> {
        let mut counters = self.load()?;
        match scope {
            "session" => {}
            "day" => {
                counters.day = Default::default();
                counters.day_key.clear();
            }
            "window" => counters.window.clear(),
            "total" => counters.total = Default::default(),
            _ => {
                counters = Counters::default();
            }
        }
        self.save(&counters)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evicts_oldest_documents_when_over_limit() {
        let dir = tempfile::tempdir().unwrap();
        let paths = crate::adapters::state::StatePaths::new(dir.path().join("state"));
        paths.ensure().unwrap();
        // 上限 0 で書き込み、その後 1 バイト上限として間引きを起こす
        let store = FsDocumentStore::new(paths.clone());
        for (i, at) in ["2026-01-01T00:00:00Z", "2026-06-01T00:00:00Z"]
            .iter()
            .enumerate()
        {
            let meta = DocumentMeta {
                url: format!("https://example.jp/{i}"),
                final_url: format!("https://example.jp/{i}"),
                title: format!("doc {i}"),
                site: None,
                fetched_at: (*at).to_string(),
                expires_at: "2027-01-01T00:00:00Z".to_string(),
                chars: 10,
                headings: 0,
                extractor: "density".to_string(),
                warnings: Vec::new(),
                cacheable: true,
            };
            store.put(&meta, "x".repeat(64).as_str()).unwrap();
        }
        assert_eq!(store.evict_to_limit().unwrap(), 0, "上限 0 は無制限");

        let bounded = FsDocumentStore::new(paths.clone()).with_max_bytes(1);
        let removed = bounded.evict_to_limit().unwrap();
        assert_eq!(removed, 2, "上限超過時は全て削除される");
        assert_eq!(bounded.total_bytes().unwrap(), 0);
    }

    #[test]
    fn session_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let paths = StatePaths::new(dir.path());
        let store = FsSessionStore::new(
            paths.clone(),
            std::sync::Arc::new(crate::ports::system::SystemClock::new()),
        );
        let session = SessionState::new("20261107-000000-abcd", chrono::Utc::now());
        store.save(&session).unwrap();
        let loaded = store.load(&session.session_id).unwrap().unwrap();
        assert_eq!(loaded.session_id, session.session_id);
        assert_eq!(store.list().unwrap().len(), 1);
        store.delete(&session.session_id).unwrap();
        assert!(store.load(&session.session_id).unwrap().is_none());
    }

    #[test]
    fn corrupt_session_is_quarantined() {
        let dir = tempfile::tempdir().unwrap();
        let paths = StatePaths::new(dir.path());
        paths.ensure().unwrap();
        let file = paths.sessions().join("bad.json");
        std::fs::write(&file, "{not json").unwrap();
        let store = FsSessionStore::new(
            paths.clone(),
            std::sync::Arc::new(crate::ports::system::SystemClock::new()),
        );
        assert!(store.load("bad").is_err());
        let leftovers: Vec<_> = std::fs::read_dir(paths.sessions())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(leftovers.len(), 1);
        assert!(
            leftovers[0]
                .file_name()
                .to_string_lossy()
                .contains("corrupt")
        );
    }

    #[test]
    fn documents_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsDocumentStore::new(StatePaths::new(dir.path()));
        let url = Url::parse("https://example.com/a").unwrap();
        let meta = DocumentMeta {
            url: url.to_string(),
            final_url: url.to_string(),
            title: "t".into(),
            fetched_at: chrono::Utc::now().to_rfc3339(),
            expires_at: (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339(),
            chars: 3,
            headings: 1,
            extractor: "density".into(),
            cacheable: true,
            ..Default::default()
        };
        store.put(&meta, "# t\n").unwrap();
        assert_eq!(store.get(&url).unwrap().unwrap().1, "# t\n");
        store.invalidate(&url).unwrap();
        assert!(store.get(&url).unwrap().is_none());
    }
}
