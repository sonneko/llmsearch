//! セッション状態。

use serde::{Deserialize, Serialize};

use crate::core::model::SearchHit;

/// 実行した検索（結果一覧のスロット）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
#[derive(Default)]
pub struct QueryRecord {
    pub query: String,
    pub engine: String,
    pub at: String,
    pub results: usize,
    pub filtered: usize,
}

/// ページングの状態。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct PageState {
    /// 現在表示済みの最大ページ（1 始まり）。
    pub page: u32,
    /// 次に取得するページ。
    pub next_page: u32,
    /// エンジン側の上限に達したか。
    pub exhausted: bool,
}

impl Default for PageState {
    fn default() -> Self {
        Self {
            page: 0,
            next_page: 1,
            exhausted: false,
        }
    }
}

/// 取得済み文書の参照。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
#[derive(Default)]
pub struct DocumentRef {
    pub id: u32,
    pub url: String,
    pub final_url: String,
    pub title: String,
    pub fetched_at: String,
    pub chars: usize,
    pub headings: usize,
    /// 取得時の警告（表示用の文字列。コードは履歴側で保持する）。
    pub warnings: Vec<String>,
    /// 文書キャッシュのキー（URL のハッシュ）。
    pub cache_key: String,
}

/// 選択済みの節。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    pub document: u32,
    pub section: u32,
    pub order: u32,
}

/// セッション状態。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SessionState {
    pub schema_version: u32,
    pub session_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub queries: Vec<QueryRecord>,
    pub active_query: usize,
    pub results: Vec<SearchHit>,
    pub page: PageState,
    pub documents: Vec<DocumentRef>,
    pub outline_depth: u8,
    pub selections: Vec<Selection>,
    pub pending_selections: Vec<String>,
    /// 直近のクエリ（`refine` の初期値）。
    pub last_query: String,
    /// 直近のエンジン選択。
    pub last_engine: String,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            schema_version: 2,
            session_id: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
            queries: Vec::new(),
            active_query: 0,
            results: Vec::new(),
            page: PageState::default(),
            documents: Vec::new(),
            outline_depth: 3,
            selections: Vec::new(),
            pending_selections: Vec::new(),
            last_query: String::new(),
            last_engine: String::new(),
        }
    }
}

impl SessionState {
    pub fn new(id: impl Into<String>, now: chrono::DateTime<chrono::Utc>) -> Self {
        let stamp = now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        Self {
            session_id: id.into(),
            created_at: stamp.clone(),
            updated_at: stamp,
            ..Default::default()
        }
    }

    pub fn touch(&mut self, now: chrono::DateTime<chrono::Utc>) {
        self.updated_at = now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    }

    /// 次の結果識別子。
    pub fn next_result_id(&self) -> u32 {
        self.results.iter().map(|r| r.id).max().unwrap_or(0) + 1
    }

    /// 次の文書識別子。
    pub fn next_document_id(&self) -> u32 {
        self.documents.iter().map(|d| d.id).max().unwrap_or(0) + 1
    }

    pub fn find_result(&self, id: u32) -> Option<&SearchHit> {
        self.results.iter().find(|r| r.id == id)
    }

    pub fn find_document(&self, id: u32) -> Option<&DocumentRef> {
        self.documents.iter().find(|d| d.id == id)
    }

    /// 文書を登録する（同一 URL は既存 ID を再利用）。
    pub fn register_document(&mut self, mut doc: DocumentRef) -> u32 {
        if let Some(existing) = self
            .documents
            .iter()
            .find(|d| d.url == doc.url || d.final_url == doc.final_url)
        {
            let id = existing.id;
            doc.id = id;
            if let Some(slot) = self.documents.iter_mut().find(|d| d.id == id) {
                *slot = doc;
            }
            return id;
        }
        let id = self.next_document_id();
        doc.id = id;
        self.documents.push(doc);
        id
    }

    /// 現在のクエリ。
    pub fn current_query(&self) -> Option<&QueryRecord> {
        self.queries.get(self.active_query)
    }

    /// 品質の要約（1 行）。
    pub fn summary_line(&self) -> String {
        format!(
            "セッション {} / 検索 {} 回 / 結果 {} 件 / 文書 {} 件",
            self.session_id,
            self.queries.len(),
            self.results.len(),
            self.documents.len()
        )
    }
}

/// 文書キャッシュのキー（URL の SHA-256 先頭 16 文字）。
pub fn cache_key(url: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    let digest = hasher.finalize();
    digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_ids_are_stable_per_url() {
        let mut s = SessionState::new("s", chrono::Utc::now());
        let mk = |url: &str| DocumentRef {
            url: url.into(),
            final_url: url.into(),
            ..Default::default()
        };
        let id1 = s.register_document(mk("https://a/1"));
        let id2 = s.register_document(mk("https://a/1"));
        let id3 = s.register_document(mk("https://a/2"));
        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
        assert_eq!(s.documents.len(), 2);
    }

    #[test]
    fn result_ids_continue() {
        let mut s = SessionState::new("s", chrono::Utc::now());
        assert_eq!(s.next_result_id(), 1);
        s.results.push(SearchHit {
            id: 7,
            rank: 7,
            title: "t".into(),
            url: "https://a/".into(),
            site: None,
            snippet: None,
            published: None,
            engine: "e".into(),
            language: None,
        });
        assert_eq!(s.next_result_id(), 8);
    }

    #[test]
    fn cache_key_is_stable() {
        assert_eq!(cache_key("https://a/"), cache_key("https://a/"));
        assert_eq!(cache_key("https://a/").len(), 16);
    }
}
