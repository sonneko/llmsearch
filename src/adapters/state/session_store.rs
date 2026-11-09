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
    now: Box<dyn Fn() -> chrono::DateTime<chrono::Utc> + Send + Sync>,
}

impl FsSessionStore {
    pub fn new(paths: StatePaths) -> Self {
        Self {
            paths,
            now: Box::new(chrono::Utc::now),
        }
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
                let stamp = (self.now)().format("%Y%m%d%H%M%S").to_string();
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
}

impl FsDocumentStore {
    pub fn new(paths: StatePaths) -> Self {
        Self { paths }
    }

    fn key(&self, url: &Url) -> String {
        crate::core::session::cache_key(url.as_str())
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
        write_atomic(&dir.join("body.md"), markdown.as_bytes())
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
}

impl FsCounters {
    pub fn new(paths: StatePaths) -> Self {
        Self { paths }
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
                let stamp = chrono::Utc::now().format("%Y%m%d%H%M%S").to_string();
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
    fn session_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let paths = StatePaths::new(dir.path());
        let store = FsSessionStore::new(paths.clone());
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
        let store = FsSessionStore::new(paths.clone());
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
