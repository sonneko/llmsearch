//! 再試行の待機時間（純粋ロジック）。
//!
//! HTTP の `Retry-After` ヘッダの解釈をここに閉じる。ネットワーク層は
//! 解釈結果（ミリ秒）だけを扱い、文字列の解析を持ち込まない。

/// `Retry-After` を解釈して待機ミリ秒を返す。
///
/// 次の 2 形式を受理する（RFC 9110 §10.2.3）。
///
/// - 遅延秒数（`Retry-After: 120`）
/// - HTTP 日付（`Retry-After: Wed, 21 Oct 2015 07:28:00 GMT`）
///
/// 過去の日付は「待機なし」（`Some(0)`）として扱う。解釈できない値は
/// `None` を返し、呼び出し側で指数バックオフへ戻す。負の値も `None` とする。
pub fn parse_retry_after_ms(value: &str, now: chrono::DateTime<chrono::Utc>) -> Option<u64> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(seconds.saturating_mul(1000));
    }
    if let Ok(seconds) = value.parse::<f64>()
        && seconds.is_finite()
        && seconds >= 0.0
    {
        return Some((seconds * 1000.0) as u64);
    }
    let when = chrono::DateTime::parse_from_rfc2822(value).ok()?;
    let delta = when.with_timezone(&chrono::Utc) - now;
    let millis = delta.num_milliseconds();
    Some(if millis < 0 { 0 } else { millis as u64 })
}

/// 待機時間が上限を超えるか（仕様 network.md §3）。
///
/// `Retry-After` が無い（`None`）ときは超えないものとして扱う。
pub fn exceeds(retry_after_ms: Option<u64>, max_backoff_ms: u64) -> bool {
    retry_after_ms.is_some_and(|ms| ms > max_backoff_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> chrono::DateTime<chrono::Utc> {
        chrono::Utc.with_ymd_and_hms(2026, 11, 14, 0, 0, 0).unwrap()
    }

    #[test]
    fn parses_seconds() {
        assert_eq!(parse_retry_after_ms("120", now()), Some(120_000));
        assert_eq!(parse_retry_after_ms(" 0 ", now()), Some(0));
    }

    #[test]
    fn parses_http_date() {
        let value = "Sat, 14 Nov 2026 00:00:30 GMT";
        assert_eq!(parse_retry_after_ms(value, now()), Some(30_000));
    }

    #[test]
    fn past_date_is_no_wait() {
        let value = "Fri, 13 Nov 2026 00:00:00 GMT";
        assert_eq!(parse_retry_after_ms(value, now()), Some(0));
    }

    #[test]
    fn unknown_values_are_ignored() {
        assert_eq!(parse_retry_after_ms("", now()), None);
        assert_eq!(parse_retry_after_ms("soon", now()), None);
        assert_eq!(parse_retry_after_ms("-1", now()), None);
    }

    #[test]
    fn exceed_check_considers_only_present_values() {
        assert!(exceeds(Some(9000), 8000));
        assert!(!exceeds(Some(8000), 8000));
        assert!(!exceeds(None, 8000));
    }
}
