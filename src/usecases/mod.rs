//! ユースケース（操作の単位）。UI 層から呼ばれる。

pub mod config;
pub mod doctor;
pub mod document;
pub mod engines;
pub mod guard;
pub mod history;
pub mod search;
pub mod target;

use std::path::PathBuf;
use std::sync::Arc;

use crate::core::config::filter::CompiledFilters;
use crate::core::config::{NetworkProfile, QuotaPolicy, Settings};
use crate::core::errors::AppError;
use crate::core::secret::Secrets;
use crate::ports::state::{CounterStore, DocumentStore, HistoryStore, SessionStore};
use crate::ports::{Clock, Env, HttpClient, IdGen, Reporter};
use crate::usecases::guard::Guard;

/// 実行環境の一式（依存の注入点）。
pub struct App {
    pub settings: Settings,
    pub network: NetworkProfile,
    pub filters: CompiledFilters,
    pub quota: QuotaPolicy,
    pub secrets: Secrets,
    pub paths: crate::adapters::state::StatePaths,
    pub bundled: PathBuf,
    pub clock: Arc<dyn Clock>,
    pub env: Arc<dyn Env>,
    pub ids: Arc<dyn IdGen>,
    pub reporter: Arc<dyn Reporter>,
    pub http: Arc<dyn HttpClient>,
    pub sessions: Arc<dyn SessionStore>,
    pub documents: Arc<dyn DocumentStore>,
    pub history: Arc<dyn HistoryStore>,
    pub counters: Arc<dyn CounterStore>,
    pub engines: crate::core::engine::EngineRegistry,
    /// フィルタを無効化するか。
    pub no_filter: bool,
    /// キャッシュを読み書きしない。
    pub no_cache: bool,
    /// キャッシュを無視して再取得する。
    pub refresh: bool,
    pub json: bool,
    pub quiet: bool,
    pub verbose: bool,
    /// 追跡パラメータ（`strip_tracking` 用）。
    pub tracking: Vec<String>,
    /// 選択されたエンジン（`search.engine` の解決結果）。
    pub default_engine: String,
    /// 現行セッションの識別子（遅延生成）。
    pub session_id: std::sync::Mutex<String>,
}

impl App {
    /// ガード（レート制限 + クォータ）を作る。
    pub fn guard(&self) -> Result<Guard, AppError> {
        Guard::new(
            self.quota.clone(),
            Arc::clone(&self.counters),
            Arc::clone(&self.clock),
            Arc::clone(&self.reporter),
        )
    }

    /// 追跡パラメータ。
    pub fn tracking(&self) -> &[String] {
        &self.tracking
    }

    /// 日本語か。
    pub fn locale(&self) -> crate::core::errors::Locale {
        self.settings.locale()
    }

    /// 現行セッションの識別子（未作成なら空）。
    pub fn current_session_id(&self) -> String {
        self.session_id
            .lock()
            .map(|s| s.clone())
            .unwrap_or_default()
    }

    /// セッションを用意して識別子を返す。
    pub fn ensure_session(&self) -> Result<String, AppError> {
        let mut guard = match self.session_id.lock() {
            Ok(g) => g,
            Err(poison) => poison.into_inner(),
        };
        if guard.is_empty() {
            *guard = self.ids.session_id(self.clock.now_utc());
        }
        Ok(guard.clone())
    }

    /// セッション識別子を設定する（`--session`）。
    pub fn set_session(&self, id: impl Into<String>) {
        if let Ok(mut guard) = self.session_id.lock() {
            *guard = id.into();
        }
    }

    /// 同梱リソースの基準ディレクトリ。
    pub fn resource_root(&self) -> PathBuf {
        self.bundled.clone()
    }
}
