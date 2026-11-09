//! 状態の永続化ポート。

use url::Url;

use crate::core::errors::AppError;
use crate::core::history::HistoryEntry;
use crate::core::model::{DocumentMeta, SearchHit};
use crate::core::session::SessionState;

pub use crate::core::config::quota::Counters;

/// セッションの保存・読み込み。
pub trait SessionStore: Send + Sync {
    fn load(&self, id: &str) -> Result<Option<SessionState>, AppError>;
    fn save(&self, session: &SessionState) -> Result<(), AppError>;
    fn delete(&self, id: &str) -> Result<(), AppError>;
    fn list(&self) -> Result<Vec<String>, AppError>;
}

/// 文書キャッシュ。
pub trait DocumentStore: Send + Sync {
    fn get(&self, url: &Url) -> Result<Option<(DocumentMeta, String)>, AppError>;
    fn put(&self, meta: &DocumentMeta, markdown: &str) -> Result<(), AppError>;
    fn invalidate(&self, url: &Url) -> Result<(), AppError>;
    fn total_bytes(&self) -> Result<u64, AppError>;
}

/// 履歴。
#[derive(Debug, Clone, Default)]
pub struct HistoryQuery {
    pub limit: Option<usize>,
    pub before: Option<String>,
    pub query: Option<String>,
    pub session: Option<String>,
    pub kinds: Vec<String>,
}

/// 履歴の追記・参照。
pub trait HistoryStore: Send + Sync {
    fn append(&self, entry: &HistoryEntry) -> Result<(), AppError>;
    fn query(&self, q: &HistoryQuery) -> Result<Vec<HistoryEntry>, AppError>;
    fn clear(&self, before: Option<&str>) -> Result<usize, AppError>;
    fn skipped_lines(&self) -> usize;
}

/// 検索結果のスロット（セッション復元に使う）。
#[derive(Debug, Clone, Default)]
pub struct ResultSlot {
    pub hits: Vec<SearchHit>,
}

/// カウンタ（クォータ）。
pub trait CounterStore: Send + Sync {
    fn load(&self) -> Result<Counters, AppError>;
    fn save(&self, counters: &Counters) -> Result<(), AppError>;
    fn reset(&self, scope: &str) -> Result<(), AppError>;
}
