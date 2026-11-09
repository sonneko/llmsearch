//! ファイルシステム上の状態管理。

pub mod history_jsonl;
pub mod session_store;

pub use history_jsonl::JsonlHistory;
pub use session_store::{FsCounters, FsDocumentStore, FsSessionStore};

use std::path::{Path, PathBuf};

use crate::core::errors::{AppError, ErrorCode};
use crate::core::session::cache_key;
use url::Url;

/// 状態ディレクトリのパス群。
#[derive(Debug, Clone)]
pub struct StatePaths {
    pub root: PathBuf,
}

impl StatePaths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn sessions(&self) -> PathBuf {
        self.root.join("sessions")
    }

    pub fn documents(&self) -> PathBuf {
        self.root.join("documents")
    }

    pub fn history_file(&self) -> PathBuf {
        self.root.join("history/history.jsonl")
    }

    pub fn counters_file(&self) -> PathBuf {
        self.root.join("counters.json")
    }

    pub fn robots_file(&self) -> PathBuf {
        self.root.join("cache/robots.json")
    }

    pub fn document_dir(&self, url: &str) -> PathBuf {
        self.documents().join(cache_key(url))
    }

    /// 必要なディレクトリを作る（権限 0700）。
    pub fn ensure(&self) -> Result<(), AppError> {
        for dir in [
            self.root.clone(),
            self.sessions(),
            self.documents(),
            self.root.join("history"),
            self.root.join("cache"),
        ] {
            std::fs::create_dir_all(&dir).map_err(|e| {
                AppError::with_detail(ErrorCode::E5002, format!("{}: {e}", dir.display()))
            })?;
            restrict(&dir)?;
        }
        Ok(())
    }

    /// 合計サイズ（バイト）。
    pub fn total_bytes(&self) -> u64 {
        fn walk(dir: &Path, acc: &mut u64) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                match entry.metadata() {
                    Ok(meta) if meta.is_dir() => walk(&path, acc),
                    Ok(meta) => *acc += meta.len(),
                    Err(_) => {}
                }
            }
        }
        let mut total = 0;
        walk(&self.root, &mut total);
        total
    }
}

#[cfg(unix)]
fn restrict(dir: &Path) -> Result<(), AppError> {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    Ok(())
}

#[cfg(not(unix))]
fn restrict(_dir: &Path) -> Result<(), AppError> {
    Ok(())
}

/// 原子的に書き込む（一時ファイル + rename）。
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            AppError::with_detail(ErrorCode::E5002, format!("{}: {e}", parent.display()))
        })?;
    }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&tmp, bytes)
        .map_err(|e| AppError::with_detail(ErrorCode::E5002, format!("{}: {e}", tmp.display())))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        AppError::with_detail(ErrorCode::E5002, format!("{}: {e}", path.display()))
    })
}

/// 破損ファイルを退避する。
pub fn quarantine(path: &Path, stamp: &str) -> Option<PathBuf> {
    let target = path.with_extension(format!("corrupt-{stamp}"));
    std::fs::rename(path, &target).ok().map(|_| target)
}

/// URL をファイル名に使えるキーへ。
pub fn url_key(url: &Url) -> String {
    cache_key(url.as_str())
}
