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
    /// 既知の秘匿値を伏せてから履歴へ追記する。
    pub fn append_history(
        &self,
        entry: &crate::core::history::HistoryEntry,
    ) -> Result<(), AppError> {
        let redactions =
            crate::core::secret::redaction_list_for_engines(&self.secrets, &self.engines);
        let mut safe = entry.clone();
        if let Some(engine) = &mut safe.engine {
            *engine = crate::core::secret::redact_display_text(engine, &redactions);
        }
        crate::core::secret::redact_json_output(&mut safe.payload, &redactions);
        self.history.append(&safe)
    }

    /// クォータ判定用のガードを作る（レート待機は HTTP アダプタが担当）。
    pub fn guard(&self) -> Result<Guard, AppError> {
        let session = self.current_session_id();
        Guard::new(
            self.quota.clone(),
            Arc::clone(&self.counters),
            Arc::clone(&self.clock),
            Arc::clone(&self.reporter),
            &session,
        )
    }

    /// HTTP 実装を差し替える（テスト・診断用）。
    pub fn set_http(&mut self, http: Arc<dyn HttpClient>) {
        self.http = http;
    }

    /// 時計を差し替える。
    pub fn set_clock(&mut self, clock: Arc<dyn Clock>) {
        self.clock = clock;
    }

    /// 追跡パラメータ。
    pub fn tracking(&self) -> &[String] {
        &self.tracking
    }

    /// 検索エンジンへ送る言語（設定 `search.language`）。
    pub fn language(&self) -> crate::core::lang::Language {
        crate::core::lang::parse(&self.settings.search.language).unwrap_or_default()
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

    /// 明示されたセッションを選ぶ。未知の ID は暗黙に作成しない。
    pub fn select_session(&self, id: &str) -> Result<crate::core::session::SessionState, AppError> {
        let session = self.sessions.load(id)?.ok_or_else(|| {
            AppError::new(crate::core::errors::ErrorCode::E5001)
                .detail(format!("session '{id}' does not exist"))
                .hint("choose an existing session ID, or run `search` without `--session`")
        })?;
        self.set_session(id);
        Ok(session)
    }

    /// `counters.json` が指す既存セッションを再開する（存在しなければ `None`）。
    pub fn resume_latest_session(
        &self,
    ) -> Result<Option<crate::core::session::SessionState>, AppError> {
        let current = self.current_session_id();
        if !current.is_empty() {
            return self.sessions.load(&current);
        }
        let counters = self.counters.load()?;
        if counters.session_key.is_empty() {
            return Ok(None);
        }
        let Some(session) = self.sessions.load(&counters.session_key)? else {
            return Ok(None);
        };
        self.set_session(&counters.session_key);
        Ok(Some(session))
    }

    /// 既存の現行または直近セッションを必須として取得する。
    pub fn require_existing_session(&self) -> Result<crate::core::session::SessionState, AppError> {
        let requested = self.current_session_id();
        self.resume_latest_session()?.ok_or_else(|| {
            let detail = if requested.is_empty() {
                "no existing session is available".to_string()
            } else {
                format!("session '{requested}' does not exist")
            };
            AppError::new(crate::core::errors::ErrorCode::E5001)
                .detail(detail)
                .hint("run `search` to start a session, then retry")
        })
    }

    /// セッション識別子を設定する（新規セッション生成後など）。
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
