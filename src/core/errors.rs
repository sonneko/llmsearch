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
}

impl AppError {
    pub fn new(code: ErrorCode) -> Self {
        Self {
            code,
            detail: None,
            hint: None,
        }
    }

    pub fn with_detail(code: ErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: Some(detail.into()),
            hint: None,
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

    /// 利用者向けの 1 文（既定は日本語）。
    pub fn message(&self) -> &'static str {
        message(self.code, Locale::default())
    }

    pub fn exit_code(&self) -> i32 {
        self.code.kind().exit_code()
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {}",
            self.code.as_str(),
            message(self.code, Locale::Ja)
        )?;
        if let Some(d) = &self.detail {
            write!(f, ": {d}")?;
        }
        Ok(())
    }
}

impl std::error::Error for AppError {}

pub type Result<T> = std::result::Result<T, AppError>;

/// メッセージの言語。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Locale {
    #[default]
    Ja,
    En,
}

impl Locale {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "ja" => Some(Locale::Ja),
            "en" => Some(Locale::En),
            _ => None,
        }
    }
}

/// コードごとの定型メッセージ。
pub fn message(code: ErrorCode, locale: Locale) -> &'static str {
    let ja = matches!(locale, Locale::Ja);
    match code {
        ErrorCode::E1001 => {
            if ja {
                "対話モードは端末でのみ利用できる"
            } else {
                "interactive mode requires a terminal"
            }
        }
        ErrorCode::E1002 => {
            if ja {
                "不明な引数またはサブコマンド"
            } else {
                "unknown argument or subcommand"
            }
        }
        ErrorCode::E1003 => {
            if ja {
                "設定・定義ファイルを読み込めない"
            } else {
                "cannot read configuration file"
            }
        }
        ErrorCode::E1004 => {
            if ja {
                "設定・定義の値が不正"
            } else {
                "invalid configuration value"
            }
        }
        ErrorCode::E1005 => {
            if ja {
                "選択の指定を解釈できない"
            } else {
                "cannot parse the selection"
            }
        }
        ErrorCode::E1006 => {
            if ja {
                "対象の識別子が見つからない"
            } else {
                "target identifier not found"
            }
        }
        ErrorCode::E1007 => {
            if ja {
                "検索語が空である"
            } else {
                "query is empty"
            }
        }
        ErrorCode::E1008 => {
            if ja {
                "テンプレート変数が未知"
            } else {
                "unknown template variable"
            }
        }
        ErrorCode::E2001 => {
            if ja {
                "接続に失敗した"
            } else {
                "connection failed"
            }
        }
        ErrorCode::E2002 => {
            if ja {
                "タイムアウトした"
            } else {
                "request timed out"
            }
        }
        ErrorCode::E2003 => {
            if ja {
                "HTTP エラー応答を受け取った"
            } else {
                "HTTP error response"
            }
        }
        ErrorCode::E2004 => {
            if ja {
                "リダイレクトが多すぎる"
            } else {
                "too many redirects"
            }
        }
        ErrorCode::E2005 => {
            if ja {
                "対応していないスキームまたは形式"
            } else {
                "unsupported scheme or content type"
            }
        }
        ErrorCode::E2006 => {
            if ja {
                "robots.txt により拒否された"
            } else {
                "denied by robots.txt"
            }
        }
        ErrorCode::E2007 => {
            if ja {
                "応答サイズが上限を超えた"
            } else {
                "response exceeded size limit"
            }
        }
        ErrorCode::E2008 => {
            if ja {
                "TLS の検証に失敗した"
            } else {
                "TLS verification failed"
            }
        }
        ErrorCode::E3001 => {
            if ja {
                "本文を抽出できない"
            } else {
                "cannot extract the main content"
            }
        }
        ErrorCode::E3002 => {
            if ja {
                "非対応のファイル形式"
            } else {
                "unsupported document type"
            }
        }
        ErrorCode::E3003 => {
            if ja {
                "文字コードを判定できない"
            } else {
                "cannot determine the charset"
            }
        }
        ErrorCode::E3004 => {
            if ja {
                "Markdown への変換に失敗した"
            } else {
                "markdown conversion failed"
            }
        }
        ErrorCode::E4001 => {
            if ja {
                "検索結果が 0 件"
            } else {
                "no search results"
            }
        }
        ErrorCode::E4002 => {
            if ja {
                "検索エンジンを利用できない"
            } else {
                "search engine is unavailable"
            }
        }
        ErrorCode::E4003 => {
            if ja {
                "レート制限または拒否"
            } else {
                "rate limited or blocked"
            }
        }
        ErrorCode::E4004 => {
            if ja {
                "応答を解析できない（抽出ルール不一致）"
            } else {
                "cannot parse response (extraction rules mismatch)"
            }
        }
        ErrorCode::E4005 => {
            if ja {
                "エンジン定義の ID が重複している"
            } else {
                "duplicate engine definition id"
            }
        }
        ErrorCode::E4006 => {
            if ja {
                "抽出結果が不正（相対 URL など）"
            } else {
                "invalid extraction result"
            }
        }
        ErrorCode::E5001 => {
            if ja {
                "セッションが存在しないか期限切れ"
            } else {
                "session not found or expired"
            }
        }
        ErrorCode::E5002 => {
            if ja {
                "状態ディレクトリに書き込めない"
            } else {
                "cannot write the state directory"
            }
        }
        ErrorCode::E5003 => {
            if ja {
                "同時更新の衝突"
            } else {
                "concurrent update conflict"
            }
        }
        ErrorCode::E5004 => {
            if ja {
                "履歴ファイルが破損している"
            } else {
                "history file is corrupted"
            }
        }
        ErrorCode::E5005 => {
            if ja {
                "クォータの上限に達した"
            } else {
                "quota exceeded"
            }
        }
    }
}

/// コードごとの回復手順。
pub fn hint(code: ErrorCode, locale: Locale) -> &'static str {
    let ja = matches!(locale, Locale::Ja);
    match code {
        ErrorCode::E1001 => {
            if ja {
                "`llmsearch --help` でサブコマンドを確認する"
            } else {
                "see `llmsearch --help`"
            }
        }
        ErrorCode::E1002 => {
            if ja {
                "`llmsearch --help` を見る"
            } else {
                "run `llmsearch --help`"
            }
        }
        ErrorCode::E1003 => {
            if ja {
                "`config path` で探索先を確認し、`config init` で雛形を作る"
            } else {
                "check `config path`, or run `config init`"
            }
        }
        ErrorCode::E1004 => {
            if ja {
                "`config validate` / `engines validate` で詳細を確認する"
            } else {
                "run `config validate` / `engines validate`"
            }
        }
        ErrorCode::E1005 => {
            if ja {
                "`1`、`1,3`、`1-3`、`R1` の形式で指定する"
            } else {
                "use forms like `1`, `1,3`, `1-3`, `R1`"
            }
        }
        ErrorCode::E1006 => {
            if ja {
                "`search` を実行し、有効な識別子を確認する"
            } else {
                "run `search` and use a valid identifier"
            }
        }
        ErrorCode::E1007 => {
            if ja {
                "検索語を指定する"
            } else {
                "provide a query"
            }
        }
        ErrorCode::E1008 => {
            if ja {
                "`engines validate` で使用可能な変数を確認する"
            } else {
                "run `engines validate` to see allowed variables"
            }
        }
        ErrorCode::E2001 => {
            if ja {
                "別の結果を選ぶか、時間を置いて再実行する"
            } else {
                "choose another result or retry later"
            }
        }
        ErrorCode::E2002 => {
            if ja {
                "ネットワークプロファイルの timeout を延ばす"
            } else {
                "increase the network profile timeout"
            }
        }
        ErrorCode::E2003 => {
            if ja {
                "4xx は別 URL、5xx は時間を置いた再実行を試す"
            } else {
                "for 4xx pick another URL; for 5xx retry later"
            }
        }
        ErrorCode::E2004 => {
            if ja {
                "元 URL を確認する"
            } else {
                "check the original URL"
            }
        }
        ErrorCode::E2005 => {
            if ja {
                "HTML のページを選ぶ"
            } else {
                "choose an HTML page"
            }
        }
        ErrorCode::E2006 => {
            if ja {
                "別の結果を選ぶ"
            } else {
                "choose another result"
            }
        }
        ErrorCode::E2007 => {
            if ja {
                "`network.maxBytes` を大きくする"
            } else {
                "increase `network.maxBytes`"
            }
        }
        ErrorCode::E2008 => {
            if ja {
                "`tls.caFile` を設定する"
            } else {
                "configure `tls.caFile`"
            }
        }
        ErrorCode::E3001 => {
            if ja {
                "`--refresh` で再取得する"
            } else {
                "retry with `--refresh`"
            }
        }
        ErrorCode::E3002 => {
            if ja {
                "別の結果を選ぶ"
            } else {
                "choose another result"
            }
        }
        ErrorCode::E3003 => {
            if ja {
                "`network.charset` を指定する"
            } else {
                "set `network.charset`"
            }
        }
        ErrorCode::E3004 => {
            if ja {
                "抽出方式を変更する"
            } else {
                "change the extraction engine"
            }
        }
        ErrorCode::E4001 => {
            if ja {
                "検索語を変えるか、別のエンジンを指定する"
            } else {
                "change the query or choose another engine"
            }
        }
        ErrorCode::E4002 => {
            if ja {
                "`engines list` で利用可能なエンジンを確認する"
            } else {
                "run `engines list`"
            }
        }
        ErrorCode::E4003 => {
            if ja {
                "待機してから再実行する"
            } else {
                "wait and retry"
            }
        }
        ErrorCode::E4004 => {
            if ja {
                "`engines test` でセレクタを確認する"
            } else {
                "use `engines test` to inspect selectors"
            }
        }
        ErrorCode::E4005 => {
            if ja {
                "重複した定義を整理する"
            } else {
                "remove the duplicate definition"
            }
        }
        ErrorCode::E4006 => {
            if ja {
                "抽出ルールに `absolutize` を追加する"
            } else {
                "add `absolutize` to the extraction rules"
            }
        }
        ErrorCode::E5001 => {
            if ja {
                "`search` で新しいセッションを開始する"
            } else {
                "run `search` to start a new session"
            }
        }
        ErrorCode::E5002 => {
            if ja {
                "権限と空き容量を確認する"
            } else {
                "check permissions and free space"
            }
        }
        ErrorCode::E5003 => {
            if ja {
                "同じ操作を再実行する"
            } else {
                "retry the same operation"
            }
        }
        ErrorCode::E5004 => {
            if ja {
                "`history export` で退避してから作り直す"
            } else {
                "export then recreate the history file"
            }
        }
        ErrorCode::E5005 => {
            if ja {
                "`quota show` で残量を確認し、待機するか設定を変更する"
            } else {
                "check `quota show`, then wait or change the policy"
            }
        }
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
