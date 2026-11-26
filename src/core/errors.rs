//! エラーコード・診断・エラー型。

use std::fmt;

/// エラー分類（終了コードに対応）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Input,
    Network,
    Convert,
    Provider,
    State,
}

impl ErrorKind {
    pub fn exit_code(self) -> i32 {
        match self {
            ErrorKind::Input => 1,
            ErrorKind::Network => 2,
            ErrorKind::Convert => 3,
            ErrorKind::Provider => 4,
            ErrorKind::State => 5,
        }
    }
}

/// エラーコード（`E` + 4 桁）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    E1001,
    E1002,
    E1003,
    E1004,
    E1005,
    E1006,
    E1007,
    E1008,
    E2001,
    E2002,
    E2003,
    E2004,
    E2005,
    E2006,
    E2007,
    E2008,
    E3001,
    E3002,
    E3003,
    E3004,
    E4001,
    E4002,
    E4003,
    E4004,
    E4005,
    E4006,
    E5001,
    E5002,
    E5003,
    E5004,
    E5005,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::E1001 => "E1001",
            ErrorCode::E1002 => "E1002",
            ErrorCode::E1003 => "E1003",
            ErrorCode::E1004 => "E1004",
            ErrorCode::E1005 => "E1005",
            ErrorCode::E1006 => "E1006",
            ErrorCode::E1007 => "E1007",
            ErrorCode::E1008 => "E1008",
            ErrorCode::E2001 => "E2001",
            ErrorCode::E2002 => "E2002",
            ErrorCode::E2003 => "E2003",
            ErrorCode::E2004 => "E2004",
            ErrorCode::E2005 => "E2005",
            ErrorCode::E2006 => "E2006",
            ErrorCode::E2007 => "E2007",
            ErrorCode::E2008 => "E2008",
            ErrorCode::E3001 => "E3001",
            ErrorCode::E3002 => "E3002",
            ErrorCode::E3003 => "E3003",
            ErrorCode::E3004 => "E3004",
            ErrorCode::E4001 => "E4001",
            ErrorCode::E4002 => "E4002",
            ErrorCode::E4003 => "E4003",
            ErrorCode::E4004 => "E4004",
            ErrorCode::E4005 => "E4005",
            ErrorCode::E4006 => "E4006",
            ErrorCode::E5001 => "E5001",
            ErrorCode::E5002 => "E5002",
            ErrorCode::E5003 => "E5003",
            ErrorCode::E5004 => "E5004",
            ErrorCode::E5005 => "E5005",
        }
    }

    pub fn kind(self) -> ErrorKind {
        match self {
            ErrorCode::E1001
            | ErrorCode::E1002
            | ErrorCode::E1003
            | ErrorCode::E1004
            | ErrorCode::E1005
            | ErrorCode::E1006
            | ErrorCode::E1007
            | ErrorCode::E1008 => ErrorKind::Input,
            ErrorCode::E2001
            | ErrorCode::E2002
            | ErrorCode::E2003
            | ErrorCode::E2004
            | ErrorCode::E2005
            | ErrorCode::E2006
            | ErrorCode::E2007
            | ErrorCode::E2008 => ErrorKind::Network,
            ErrorCode::E3001 | ErrorCode::E3002 | ErrorCode::E3003 | ErrorCode::E3004 => {
                ErrorKind::Convert
            }
            ErrorCode::E4001
            | ErrorCode::E4002
            | ErrorCode::E4003
            | ErrorCode::E4004
            | ErrorCode::E4005
            | ErrorCode::E4006 => ErrorKind::Provider,
            ErrorCode::E5001
            | ErrorCode::E5002
            | ErrorCode::E5003
            | ErrorCode::E5004
            | ErrorCode::E5005 => ErrorKind::State,
        }
    }
}

/// エラー本体。
#[derive(Debug, Clone)]
pub struct AppError {
    pub code: ErrorCode,
    /// 動的な補足（パス・値・件数など）。秘匿情報を含めてはならない。
    pub detail: Option<String>,
    pub hint: Option<String>,
    /// 失敗に至る過程で出た警告（仕様 errors.md §1.2）。
    pub warnings: Vec<Diagnostic>,
    /// `Retry-After` の解釈結果（ミリ秒）。再試行の待機時間に使う。
    pub retry_after_ms: Option<u64>,
    /// HTTP 応答の状態番号（あれば）。再試行判定で詳細文を解析しないために持つ。
    pub status: Option<u16>,
}

impl AppError {
    pub fn new(code: ErrorCode) -> Self {
        Self {
            code,
            detail: None,
            hint: None,
            warnings: Vec::new(),
            retry_after_ms: None,
            status: None,
        }
    }

    pub fn with_detail(code: ErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: Some(detail.into()),
            hint: None,
            warnings: Vec::new(),
            retry_after_ms: None,
            status: None,
        }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// 失敗に至る過程で出た警告を添える（表示・JSON の両方で出す）。
    pub fn warnings(mut self, warnings: Vec<Diagnostic>) -> Self {
        self.warnings = warnings;
        self
    }

    /// `Retry-After` の解釈結果を添える（再試行の待機時間）。
    pub fn retry_after_ms(mut self, ms: Option<u64>) -> Self {
        self.retry_after_ms = ms;
        self
    }

    /// HTTP 応答の状態番号を添える（再試行判定で使う）。
    pub fn status(mut self, status: u16) -> Self {
        self.status = Some(status);
        self
    }

    /// 利用者向けの 1 文。
    pub fn message(&self) -> &'static str {
        message(self.code)
    }

    pub fn exit_code(&self) -> i32 {
        self.code.kind().exit_code()
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.code.as_str(), message(self.code))?;
        if let Some(d) = &self.detail {
            write!(f, ": {d}")?;
        }
        Ok(())
    }
}

impl std::error::Error for AppError {}

pub type Result<T> = std::result::Result<T, AppError>;

/// コードごとの定型メッセージ。
pub fn message(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::E1001 => "interactive mode requires a terminal",
        ErrorCode::E1002 => "unknown argument or subcommand",
        ErrorCode::E1003 => "cannot load or resolve configuration resource",
        ErrorCode::E1004 => "invalid configuration value",
        ErrorCode::E1005 => "cannot parse the selection",
        ErrorCode::E1006 => "target identifier not found",
        ErrorCode::E1007 => "query is empty",
        ErrorCode::E1008 => "unknown template variable",
        ErrorCode::E2001 => "connection failed",
        ErrorCode::E2002 => "request timed out",
        ErrorCode::E2003 => "HTTP error response",
        ErrorCode::E2004 => "too many redirects",
        ErrorCode::E2005 => "unsupported scheme or content type",
        ErrorCode::E2006 => "denied by robots.txt",
        ErrorCode::E2007 => "response exceeded size limit",
        ErrorCode::E2008 => "TLS verification failed",
        ErrorCode::E3001 => "cannot extract the main content",
        ErrorCode::E3002 => "unsupported document type",
        ErrorCode::E3003 => "cannot determine the charset",
        ErrorCode::E3004 => "markdown conversion failed",
        ErrorCode::E4001 => "no search results",
        ErrorCode::E4002 => "search engine is unavailable",
        ErrorCode::E4003 => "rate limited or blocked",
        ErrorCode::E4004 => "cannot parse response (extraction rules mismatch)",
        ErrorCode::E4005 => "duplicate engine definition id",
        ErrorCode::E4006 => "invalid extraction result",
        ErrorCode::E5001 => "session not found or expired",
        ErrorCode::E5002 => "cannot write the state directory",
        ErrorCode::E5003 => "concurrent update conflict",
        ErrorCode::E5004 => "history file is corrupted",
        ErrorCode::E5005 => "quota exceeded",
    }
}

/// コードごとの回復手順。
pub fn hint(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::E1001 => "see `llmsearch --help`",
        ErrorCode::E1002 => "run `llmsearch --help`",
        ErrorCode::E1003 => "check `config path`, or run `config init`",
        ErrorCode::E1004 => "run `config validate` / `engines validate`",
        ErrorCode::E1005 => "use forms like `1`, `1,3`, `1-3`, `R1`",
        ErrorCode::E1006 => "run `search` and use a valid identifier",
        ErrorCode::E1007 => "provide a query",
        ErrorCode::E1008 => "run `engines validate` to see allowed variables",
        ErrorCode::E2001 => "choose another result or retry later",
        ErrorCode::E2002 => "increase the network profile timeout",
        ErrorCode::E2003 => "for 4xx pick another URL; for 5xx retry later",
        ErrorCode::E2004 => "check the original URL",
        ErrorCode::E2005 => "choose an HTML page",
        ErrorCode::E2006 => "choose another result",
        ErrorCode::E2007 => "increase `network.maxBytes`",
        ErrorCode::E2008 => "configure `tls.caFile`",
        ErrorCode::E3001 => "retry with `--refresh`",
        ErrorCode::E3002 => "choose another result",
        ErrorCode::E3003 => "set `network.charset`",
        ErrorCode::E3004 => "change the extraction engine",
        ErrorCode::E4001 => "change the query or choose another engine",
        ErrorCode::E4002 => "run `engines list`",
        ErrorCode::E4003 => "wait and retry",
        ErrorCode::E4004 => "use `engines test` to inspect selectors",
        ErrorCode::E4005 => "remove the duplicate definition",
        ErrorCode::E4006 => "add `absolutize` to the extraction rules",
        ErrorCode::E5001 => "run `search` to start a new session",
        ErrorCode::E5002 => "check permissions and free space",
        ErrorCode::E5003 => "retry the same operation",
        ErrorCode::E5004 => "export then recreate the history file",
        ErrorCode::E5005 => "check `quota show`, then wait or change the policy",
    }
}

/// 警告コード。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WarningCode {
    W1001,
    W1002,
    W1003,
    W1004,
    W1005,
    W2001,
    W3001,
    W3002,
    W3003,
    W3004,
    W4001,
    W4002,
    W4003,
    W4004,
    W4005,
    W4006,
    W5001,
    W5002,
    W5004,
    W5005,
}

impl WarningCode {
    pub fn as_str(self) -> &'static str {
        match self {
            WarningCode::W1001 => "W1001",
            WarningCode::W1002 => "W1002",
            WarningCode::W1003 => "W1003",
            WarningCode::W1004 => "W1004",
            WarningCode::W1005 => "W1005",
            WarningCode::W2001 => "W2001",
            WarningCode::W3001 => "W3001",
            WarningCode::W3002 => "W3002",
            WarningCode::W3003 => "W3003",
            WarningCode::W3004 => "W3004",
            WarningCode::W4001 => "W4001",
            WarningCode::W4002 => "W4002",
            WarningCode::W4003 => "W4003",
            WarningCode::W4004 => "W4004",
            WarningCode::W4005 => "W4005",
            WarningCode::W4006 => "W4006",
            WarningCode::W5001 => "W5001",
            WarningCode::W5002 => "W5002",
            WarningCode::W5004 => "W5004",
            WarningCode::W5005 => "W5005",
        }
    }
}

/// 警告 1 件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: WarningCode,
    pub message: String,
}

impl Diagnostic {
    pub fn new(code: WarningCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// 表示用の 1 行（`W4002 メッセージ` 形式ではない点に注意）。
    pub fn text(&self) -> String {
        self.message.clone()
    }
}

/// 検証結果（`[ok]` / `[warn]` / `[fail]`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Check {
    Ok(String),
    Warn(String),
    Fail(String),
}

impl Check {
    pub fn is_fail(&self) -> bool {
        matches!(self, Check::Fail(_))
    }

    pub fn label(&self) -> &'static str {
        match self {
            Check::Ok(_) => "ok",
            Check::Warn(_) => "warn",
            Check::Fail(_) => "fail",
        }
    }

    pub fn text(&self) -> &str {
        match self {
            Check::Ok(t) | Check::Warn(t) | Check::Fail(t) => t,
        }
    }
}
