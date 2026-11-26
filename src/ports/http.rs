//! HTTP クライアントのポート。

use url::Url;

use crate::core::config::NetworkProfile;
use crate::core::errors::AppError;

/// 1 回のリクエスト。
#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: String,
    pub url: Url,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    pub timeout_ms: Option<u64>,
    /// 認証情報を含むヘッダを伏字化するための一覧。
    pub secrets: Vec<String>,
}

impl HttpRequest {
    pub fn get(url: Url) -> Self {
        Self {
            method: "GET".into(),
            url,
            headers: Vec::new(),
            body: None,
            timeout_ms: None,
            secrets: Vec::new(),
        }
    }
}

/// 生の応答。
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub final_url: Url,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub truncated: bool,
    pub redirects: usize,
}

impl HttpResponse {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn content_type(&self) -> Option<&str> {
        self.header("content-type")
    }
}

/// HTTP クライアント。
pub trait HttpClient: Send + Sync {
    fn send_blocking(
        &self,
        profile: &NetworkProfile,
        request: &HttpRequest,
    ) -> Result<HttpResponse, AppError>;

    /// 送信の直前に呼び出し側の判定を行ってから HTTP 要求を送る。
    ///
    /// 標準実装は 1 回の送信を想定する。内部で再試行する実装は、各実送信の直前に
    /// `preflight` を呼ぶよう override する。
    fn send_blocking_with_preflight(
        &self,
        profile: &NetworkProfile,
        request: &HttpRequest,
        preflight: &mut dyn FnMut() -> Result<(), AppError>,
    ) -> Result<HttpResponse, AppError> {
        preflight()?;
        self.send_blocking(profile, request)
    }

    /// プロファイルの内容を表示用に要約する。
    fn describe(&self, profile: &NetworkProfile) -> String;
}
