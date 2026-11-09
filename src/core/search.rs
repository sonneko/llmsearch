//! 検索結果の正規化・重複除去・マージ（純粋ロジック）。

use crate::core::model::SearchHit;
use crate::core::url;

/// 抽出直後の生の結果（識別子はまだ無い）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawHit {
    pub title: String,
    pub url: String,
    pub snippet: Option<String>,
    pub site: Option<String>,
    pub published: Option<String>,
}

/// URL で重複を除去する（先勝ち）。除去件数を返す。
pub fn dedup(hits: Vec<RawHit>) -> (Vec<RawHit>, usize) {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(hits.len());
    let mut removed = 0;
    for hit in hits {
        let key = match url::parse(&hit.url) {
            Ok(u) => url::dedup_key(&u),
            Err(_) => hit.url.clone(),
        };
        if seen.insert(key) {
            out.push(hit);
        } else {
            removed += 1;
        }
    }
    (out, removed)
}

/// 結果 URL に残ったエンジン側のリダイレクトを解く。
///
/// エンジン定義側で `unwrap_redirect` を指定していれば抽出時に解かれるが、
/// 定義の書き漏らしや想定外の形式でも「取得したらエンジンのページだった」と
/// いう状態にならないよう、正規化の段階でもう一度解いておく。
///
/// 戻り値は `(変換した件数, 例)`。`例` は変換前の URL（通知用）。
pub fn unwrap_redirects(hits: Vec<RawHit>) -> (Vec<RawHit>, usize, Option<String>) {
    let mut changed = 0usize;
    let mut example: Option<String> = None;
    let out = hits
        .into_iter()
        .map(|mut hit| {
            let Ok(parsed) = url::parse(&hit.url) else {
                return hit;
            };
            let Some(inner) = url::target_of_redirect(&parsed) else {
                return hit;
            };
            changed += 1;
            if example.is_none() {
                example = Some(hit.url.clone());
            }
            hit.url = inner;
            hit
        })
        .collect();
    (out, changed, example)
}

/// 複数エンジンの結果をラウンドロビンで統合する。
///
/// 戻り値は `(統合結果, 由来エンジン, 重複除去件数)`。
pub fn merge_round_robin(
    lists: Vec<Vec<RawHit>>,
    engines: &[String],
) -> (Vec<RawHit>, Vec<String>, usize) {
    let mut iterators: Vec<std::vec::IntoIter<RawHit>> =
        lists.into_iter().map(|l| l.into_iter()).collect();
    let mut names: Vec<String> = engines.to_vec();
    names.resize(iterators.len().max(names.len()), String::from("unknown"));

    let mut out: Vec<RawHit> = Vec::new();
    let mut owners: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut removed = 0;

    loop {
        let mut progressed = false;
        for (i, it) in iterators.iter_mut().enumerate() {
            let Some(hit) = it.next() else { continue };
            progressed = true;
            let key = match url::parse(&hit.url) {
                Ok(u) => url::dedup_key(&u),
                Err(_) => hit.url.clone(),
            };
            if !seen.insert(key) {
                removed += 1;
                continue;
            }
            owners.push(names[i].clone());
            out.push(hit);
        }
        if !progressed {
            break;
        }
    }
    (out, owners, removed)
}

/// 生の結果へ通番を振って `SearchHit` にする。
///
/// `engines` が `hits` より短い場合は先頭の要素で埋める。
pub fn assign_ids(hits: Vec<RawHit>, start_id: u32, engines: &[String]) -> Vec<SearchHit> {
    hits.into_iter()
        .enumerate()
        .map(|(i, h)| SearchHit {
            id: start_id + i as u32,
            rank: start_id + i as u32,
            title: h.title,
            url: h.url,
            site: h.site,
            snippet: h.snippet,
            published: h.published,
            engine: engines
                .get(i)
                .or_else(|| engines.first())
                .cloned()
                .unwrap_or_default(),
            language: None,
        })
        .collect()
}

/// クエリ文字列のハッシュ（キャッシュキー用）。
pub fn query_hash(query: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(query.as_bytes());
    let digest = hasher.finalize();
    digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(title: &str, url: &str) -> RawHit {
        RawHit {
            title: title.into(),
            url: url.into(),
            snippet: None,
            site: None,
            published: None,
        }
    }

    #[test]
    fn dedups_by_url_ignoring_fragment() {
        let hits = vec![
            hit("a", "https://example.com/x#1"),
            hit("b", "https://example.com/x#2"),
            hit("c", "https://example.com/y"),
        ];
        let (kept, removed) = dedup(hits);
        assert_eq!(kept.len(), 2);
        assert_eq!(removed, 1);
    }

    #[test]
    fn round_robin_interleaves_engines() {
        let a = vec![hit("a1", "https://a/1"), hit("a2", "https://a/2")];
        let b = vec![hit("b1", "https://b/1")];
        let (merged, owners, removed) = merge_round_robin(vec![a, b], &["a".into(), "b".into()]);
        assert_eq!(
            merged.iter().map(|h| h.title.as_str()).collect::<Vec<_>>(),
            vec!["a1", "b1", "a2"]
        );
        assert_eq!(
            owners,
            vec!["a".to_string(), "b".to_string(), "a".to_string()]
        );
        assert_eq!(removed, 0);
    }

    #[test]
    fn ids_continue_from_start() {
        let hits = assign_ids(vec![hit("a", "https://a/1")], 8, &["e".to_string()]);
        assert_eq!(hits[0].id, 8);
    }
}
