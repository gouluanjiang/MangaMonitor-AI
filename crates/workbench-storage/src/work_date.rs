//! Source-provided dates, separate from observation, file and task timestamps.
use chrono::{DateTime, Datelike, NaiveDate, SecondsFormat, Timelike, Utc};

/// Retain date-only precision; normalize offset timestamps to UTC. Missing,
/// malformed and known epoch placeholders never become a usable work date.
pub fn normalize_work_date(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 40 {
        return None;
    }
    if value.len() == 10 && value.as_bytes()[4] == b'-' && value.as_bytes()[7] == b'-' {
        let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()?;
        return valid_date(date).then(|| date.format("%Y-%m-%d").to_string());
    }
    let instant = if value.bytes().all(|byte| byte.is_ascii_digit()) {
        let number: i64 = value.parse().ok()?;
        if number <= 0 {
            return None;
        }
        if value.len() <= 10 {
            DateTime::<Utc>::from_timestamp(number, 0)?
        } else if value.len() == 13 {
            DateTime::<Utc>::from_timestamp_millis(number)?
        } else {
            return None;
        }
    } else {
        DateTime::parse_from_rfc3339(value)
            .ok()?
            .with_timezone(&Utc)
    };
    // JavaScript dates cannot represent a leap second. Keep optional metadata
    // unknown rather than emit a value that would invalidate the IPC record.
    (instant.nanosecond() < 1_000_000_000 && valid_date(instant.date_naive()))
        .then(|| instant.to_rfc3339_opts(SecondsFormat::Millis, true))
}

pub fn work_date_is_valid(value: &str) -> bool {
    normalize_work_date(value).is_some_and(|normalized| normalized == value)
}

fn valid_date(date: NaiveDate) -> bool {
    (1900..=9999).contains(&date.year())
        && !(date.year() == 1970 && date.month() == 1 && date.day() == 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_date_precision_and_normalizes_offsets_and_epoch_units() {
        assert_eq!(
            normalize_work_date("2026-09-15").as_deref(),
            Some("2026-09-15")
        );
        assert_eq!(
            normalize_work_date("2026-09-15T09:30:00+08:00").as_deref(),
            Some("2026-09-15T01:30:00.000Z")
        );
        assert_eq!(
            normalize_work_date("1609459200"),
            normalize_work_date("1609459200000")
        );
        assert_eq!(
            normalize_work_date("1609459200").as_deref(),
            Some("2021-01-01T00:00:00.000Z")
        );
        assert!(work_date_is_valid("2026-09-15"));
        assert!(work_date_is_valid("2026-09-15T01:30:00.000Z"));
        assert!(!work_date_is_valid("1609459200"));
    }

    #[test]
    fn rejects_placeholders_invalid_dates_and_times_without_known_timezone() {
        for value in [
            "",
            "0",
            "1970-01-01",
            "1970-01-01T00:00:00Z",
            "0001-01-01",
            "2026-02-30",
            "2026-09-15 12:00:00",
            "yesterday",
            "999999999999999999999",
            "2026-09-15T99:00:00Z",
            "2016-12-31T23:59:60Z",
        ] {
            assert_eq!(normalize_work_date(value), None, "{value}");
        }
        assert!(!work_date_is_valid(" 2026-09-15 "));
    }
}
