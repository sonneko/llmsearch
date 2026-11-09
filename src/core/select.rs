//! 選択入力の解釈（純粋ロジック）。

use crate::core::errors::{AppError, ErrorCode};

/// 選択の対象種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ref {
    /// 番号（`3`）。
    Index(u32),
    /// 識別子（`R3` / `S3` / `D2`）。
    Id { prefix: char, id: u32 },
}

/// 解釈結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    /// 文書指定（`D1` または `D1:S3` の `1`）。省略時は単一文書前提。
    pub document: Option<u32>,
    pub refs: Vec<Ref>,
}

/// 1 つの選択指定を解釈する。
///
/// - `3` / `1,3` / `1-3` / `R1` / `D1:S3` / `D1:S2-S4` / `D1:S3,D2:S1`
pub fn parse(input: &str) -> Result<Vec<Selection>, AppError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(AppError::with_detail(ErrorCode::E1005, "入力が空"));
    }
    let mut out = Vec::new();
    // 明らかに別のコマンド（`q`, `n`, `h`, `r` 等の単語）はここでは扱わない
    let normalized = trimmed
        .replace('Ｒ', "R")
        .replace('Ｓ', "S")
        .replace('Ｄ', "D");
    // `D` 指定が無い連続した項目は 1 つの選択（同一文書の複数節／複数結果）にまとめる
    let mut loose: Vec<Ref> = Vec::new();
    for chunk in split_chunks(&normalized) {
        let chunk = chunk.trim();
        if chunk.is_empty() {
            continue;
        }
        let parsed = parse_chunk(chunk)?;
        if parsed.document.is_none() {
            loose.extend(parsed.refs);
        } else {
            if !loose.is_empty() {
                out.push(Selection {
                    document: None,
                    refs: std::mem::take(&mut loose),
                });
            }
            out.push(parsed);
        }
    }
    if !loose.is_empty() {
        out.push(Selection {
            document: None,
            refs: loose,
        });
    }
    if out.is_empty() {
        return Err(AppError::with_detail(ErrorCode::E1005, "入力が空"));
    }
    Ok(out)
}

/// 入力を「1 つの選択指定」に分割する。
///
/// `D1:S3,S5` は 1 つの指定として扱い、`D1:S3,D2:S5` は 2 つに分ける。
fn split_chunks(input: &str) -> Vec<String> {
    let pattern = regex::Regex::new(r"[Dd]\s*\d+\s*:").expect("静的パターン");
    let starts: Vec<usize> = pattern.find_iter(input).map(|m| m.start()).collect();
    if starts.is_empty() {
        return input
            .split([',', '、', ' ', '\u{3000}'])
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    let mut out = Vec::new();
    let head = input[..starts[0]].trim();
    if !head.is_empty() {
        out.extend(
            head.split([',', '、', ' ', '\u{3000}'])
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
        );
    }
    for (i, start) in starts.iter().enumerate() {
        let end = starts.get(i + 1).copied().unwrap_or(input.len());
        let chunk = input[*start..end].trim().trim_end_matches(',').trim();
        if !chunk.is_empty() {
            out.push(chunk.to_string());
        }
    }
    out
}

fn parse_chunk(chunk: &str) -> Result<Selection, AppError> {
    let chunk = chunk.to_string();
    let chunk = chunk.as_str();
    let invalid = || AppError::with_detail(ErrorCode::E1005, chunk);

    // D 指定
    if let Some(rest) = chunk.strip_prefix('D').or_else(|| chunk.strip_prefix('d')) {
        let (doc, sections) = match rest.split_once(':') {
            Some((d, s)) => (d, Some(s)),
            None => (rest, None),
        };
        let document: u32 = doc.trim().parse().map_err(|_| invalid())?;
        let refs = match sections {
            None => vec![Ref::Id {
                prefix: 'D',
                id: document,
            }],
            Some(list) => {
                let mut refs = Vec::new();
                for part in list.split([',', '、']) {
                    let part = part.trim();
                    if part.is_empty() {
                        continue;
                    }
                    for r in parse_range(part, 'S').map_err(|_| invalid())? {
                        refs.push(r);
                    }
                }
                if refs.is_empty() {
                    return Err(invalid());
                }
                refs
            }
        };
        return Ok(Selection {
            document: Some(document),
            refs,
        });
    }

    // R / S 指定・数値
    let mut refs = Vec::new();
    for part in chunk.split([',', '、']) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        refs.extend(parse_range(part, 'R')?);
    }
    if refs.is_empty() {
        return Err(invalid());
    }
    Ok(Selection {
        document: None,
        refs,
    })
}

fn strip_prefix(s: &str, default_prefix: char) -> Result<(char, String), AppError> {
    let trimmed = s.trim();
    let mut chars = trimmed.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {
            let prefix = c.to_ascii_uppercase();
            if !matches!(prefix, 'R' | 'S' | 'D') {
                return Err(AppError::with_detail(
                    ErrorCode::E1005,
                    format!("未知の接頭辞 {prefix}"),
                ));
            }
            Ok((prefix, trimmed[c.len_utf8()..].trim().to_string()))
        }
        _ => Ok((default_prefix, trimmed.to_string())),
    }
}

fn parse_range(part: &str, default_prefix: char) -> Result<Vec<Ref>, AppError> {
    let (prefix, body) = strip_prefix(part, default_prefix)?;
    if body.is_empty() {
        return Err(AppError::with_detail(ErrorCode::E1005, part.to_string()));
    }
    let parse_num = |s: &str| -> Result<u32, AppError> {
        s.trim()
            .parse::<u32>()
            .map_err(|_| AppError::with_detail(ErrorCode::E1005, format!("数値 {part}")))
    };
    if let Some((a, b)) = body.split_once('-') {
        let start = parse_num(&strip_prefix(a, prefix)?.1)?;
        let end = parse_num(&strip_prefix(b, prefix)?.1)?;
        if end < start {
            return Err(AppError::with_detail(
                ErrorCode::E1005,
                format!("範囲 {part} が逆順"),
            ));
        }
        if end - start > 200 {
            return Err(AppError::with_detail(
                ErrorCode::E1005,
                format!("範囲 {part} が広すぎる（最大 200 件）"),
            ));
        }
        return Ok((start..=end).map(|id| Ref::Id { prefix, id }).collect());
    }
    let id = parse_num(&body)?;
    Ok(vec![Ref::Id { prefix, id }])
}

/// 文書の選択指定を人が読める形にする。
pub fn describe(selection: &Selection) -> String {
    let refs: Vec<String> = selection
        .refs
        .iter()
        .map(|r| match r {
            Ref::Index(i) => i.to_string(),
            Ref::Id { prefix, id } => format!("{prefix}{id}"),
        })
        .collect();
    match selection.document {
        Some(d) => format!("D{d}:{}", refs.join(",")),
        None => refs.join(","),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_and_ranges() {
        let s = parse("1,3-5").unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].refs.len(), 4);
        assert_eq!(describe(&s[0]), "R1,R3,R4,R5");
    }

    #[test]
    fn parses_document_sections() {
        let s = parse("D1:S3").unwrap();
        assert_eq!(s[0].document, Some(1));
        assert_eq!(s[0].refs, vec![Ref::Id { prefix: 'S', id: 3 }]);

        let s = parse("D2:S1-S3,D2:S5").unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s[1].document, Some(2));
        assert_eq!(describe(&s[0]), "D2:S1,S2,S3");
    }

    #[test]
    fn rejects_unknown_prefix() {
        assert!(parse("X1").is_err());
        assert!(parse("").is_err());
        assert!(parse("5-1").is_err());
    }

    #[test]
    fn accepts_full_width_commas_and_space() {
        let s = parse("1、2").unwrap();
        assert_eq!(s[0].refs.len(), 2);
        let s = parse("1 2").unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].refs.len(), 2);
    }
}
