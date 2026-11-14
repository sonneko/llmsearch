//! ポート（外部依存の抽象）。実装は `adapters` に置く。

use std::path::PathBuf;

use crate::core::errors::{AppError, Diagnostic};
use crate::core::model::{DocumentMeta, SearchHit};

pub mod fetch;
pub mod http;
pub mod state;
pub mod system;

pub use fetch::{FetchOptions, FetchedDocument, RawDocument};
pub use http::{HttpClient, HttpRequest, HttpResponse};
pub use state::{CounterStore, DocumentStore, HistoryQuery, HistoryStore, SessionStore};
pub use system::{SystemClock, SystemEnv, SystemIdGen};

/// 時刻。
pub trait Clock: Send + Sync {
    fn now_utc(&self) -> chrono::DateTime<chrono::Utc>;
    /// 端末のローカル日付（締め時刻の判定に使う）。
    fn today_local(&self) -> chrono::NaiveDate;
    /// 単調増加のナノ秒（レート制限の差分計算用）。
    fn monotonic_ns(&self) -> u64;
    fn sleep(&self, duration: std::time::Duration);
}

/// 識別子の生成。
pub trait IdGen: Send + Sync {
    /// セッション識別子（`YYYYMMDD-HHmmss-xxxx`）。
    fn session_id(&self, now: chrono::DateTime<chrono::Utc>) -> String;
}

/// 環境変数の参照。
pub trait Env: Send + Sync {
    fn var(&self, name: &str) -> Option<String>;
    fn var_names(&self) -> Vec<String>;
    fn is_stdin_tty(&self) -> bool;
    fn is_stdout_tty(&self) -> bool;
    fn terminal_width(&self) -> usize;
    fn home_dir(&self) -> Option<PathBuf>;
    fn state_home(&self) -> Option<PathBuf>;
    fn config_home(&self) -> Option<PathBuf>;
    /// 実行ファイルの配置先（配布物に同梱したリソースの解決用）。
    fn executable_dir(&self) -> Option<PathBuf> {
        None
    }
}

/// 進行・警告の通知。
pub trait Reporter: Send + Sync {
    fn info(&self, message: &str);
    fn warn(&self, diagnostic: &Diagnostic);
    fn verbose(&self, message: &str);
}

/// 検索エンジンの 1 回の呼び出し結果。
#[derive(Debug, Clone, Default)]
pub struct SearchResponse {
    pub hits: Vec<SearchHit>,
    pub dropped: usize,
    pub no_results: bool,
    pub engine: String,
    pub warnings: Vec<Diagnostic>,
    pub cached: bool,
}

/// 取得した文書。
#[derive(Debug, Clone)]
pub struct StoredDocument {
    pub meta: DocumentMeta,
    pub markdown: String,
    pub cached: bool,
}

/// エンジンの 1 回分の要求結果（未加工）。
#[derive(Debug, Clone)]
pub struct EngineCall {
    pub body: String,
    pub status: u16,
    pub url: String,
    pub truncated: bool,
}

/// 失敗の分類（フォールバック可否の判断に使う）。
pub fn classify(err: &AppError) -> FallbackClass {
    use crate::core::errors::ErrorCode::*;
    match err.code {
        E4001 | E4003 => FallbackClass::RetryOtherEngine,
        E1001 | E1002 | E1003 | E1004 | E1005 | E1006 | E1007 | E1008 => FallbackClass::Fatal,
        _ => FallbackClass::Fatal,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackClass {
    RetryOtherEngine,
    Fatal,
}
