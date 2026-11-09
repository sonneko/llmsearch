//! ドメインモデル（検索結果・文書・見出し・節）。

use serde::{Deserialize, Serialize};

/// 検索結果 1 件。本文は持たない（段階的開示のため）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchHit {
    /// セッション内の識別子（`R1`, `R2`, … の番号部分）。
    pub id: u32,
    /// 表示順（1 始まり）。
    pub rank: u32,
    pub title: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published: Option<String>,
    /// 由来エンジン（`id` または `id@variant`）。
    pub engine: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

impl SearchHit {
    pub fn label(&self) -> String {
        format!("R{}", self.id)
    }
}

/// 文書のメタデータ。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct DocumentMeta {
    pub url: String,
    pub final_url: String,
    pub title: String,
    #[serde(default)]
    pub site: Option<String>,
    pub fetched_at: String,
    pub expires_at: String,
    pub chars: usize,
    pub headings: usize,
    /// 変換方式（`density` など）。
    pub extractor: String,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub cacheable: bool,
}

/// 見出し。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Heading {
    /// 文書内の識別子番号（`S1`, `S2`, … の番号部分）。
    pub id: u32,
    pub level: u8,
    pub text: String,
    /// 1 始まりの行番号。
    pub start_line: usize,
    pub end_line: usize,
    /// 見出し行を除く本文の文字数。
    pub chars: usize,
    pub has_children: bool,
    /// フィルタ・深度で表示から隠されたか。
    #[serde(default)]
    pub hidden: bool,
}

impl Heading {
    pub fn label(&self) -> String {
        format!("S{}", self.id)
    }
}

/// 節の本文。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionBody {
    pub heading_id: u32,
    pub level: u8,
    pub heading: String,
    pub text: String,
    pub chars: usize,
    pub truncated: bool,
    pub next_heading_id: Option<u32>,
    /// 子見出しを省略した件数。
    pub omitted_children: usize,
}

/// 文書（本文とメタデータ）。
#[derive(Debug, Clone)]
pub struct Document {
    pub meta: DocumentMeta,
    pub markdown: String,
}
