//! Time and date utilities.
//!
//! Convenient functions for working with dates, times, and durations.
//! This module is only available with the `time` feature.

use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};

// Re-export commonly used chrono types
pub use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, NaiveTime, Utc};

/// Returns the current UTC time.
///
/// # Examples
///
/// ```
/// use smop::time;
///
/// let now = time::now();
/// println!("Current UTC time: {}", now);
/// ```
#[must_use]
pub fn now() -> DateTime<Utc> {
    Utc::now()
}

/// Returns the current local time.
///
/// # Examples
///
/// ```
/// use smop::time;
///
/// let now = time::now_local();
/// println!("Current local time: {}", now);
/// ```
#[must_use]
pub fn now_local() -> DateTime<Local> {
    Local::now()
}

/// Parses a datetime string using the specified format.
///
/// Format string uses the strftime specification:
/// - `%Y` = 4-digit year
/// - `%m` = month (01-12)
/// - `%d` = day (01-31)
/// - `%H` = hour (00-23)
/// - `%M` = minute (00-59)
/// - `%S` = second (00-59)
///
/// # Errors
///
/// Returns an error if the string doesn't match the format.
///
/// # Examples
///
/// ```
/// use smop::time;
///
/// let dt = time::parse("2024-01-15 14:30:00", "%Y-%m-%d %H:%M:%S")?;
/// println!("Parsed: {}", dt);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn parse<S: AsRef<str>>(s: S, fmt: &str) -> Result<NaiveDateTime> {
    let s = s.as_ref();
    NaiveDateTime::parse_from_str(s, fmt)
        .with_context(|| format!("Failed to parse datetime '{s}' with format '{fmt}'"))
}

/// Formats a datetime using the specified format string.
///
/// # Examples
///
/// ```
/// use smop::time;
///
/// let now = time::now();
/// let formatted = time::format(&now, "%Y-%m-%d");
/// println!("Date: {}", formatted);
/// ```
#[must_use]
pub fn format<T: chrono::TimeZone>(dt: &DateTime<T>, fmt: &str) -> String
where
    T::Offset: std::fmt::Display,
{
    dt.format(fmt).to_string()
}

/// Sleeps for the specified number of seconds.
///
/// # Examples
///
/// ```no_run
/// use smop::time;
///
/// time::sleep_secs(2);  // Sleep for 2 seconds
/// ```
pub fn sleep_secs(secs: u64) {
    thread::sleep(Duration::from_secs(secs));
}

/// Sleeps for the specified number of milliseconds.
///
/// # Examples
///
/// ```no_run
/// use smop::time;
///
/// time::sleep_millis(500);  // Sleep for 500ms
/// ```
pub fn sleep_millis(millis: u64) {
    thread::sleep(Duration::from_millis(millis));
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, Timelike};

    #[test]
    fn now_returns_current_time() {
        let time = now();
        // Just verify it's a reasonable time (after 2020)
        assert!(time.year() >= 2020);
    }

    #[test]
    fn now_local_returns_local_time() {
        let time = now_local();
        // Just verify it's a reasonable time (after 2020)
        assert!(time.year() >= 2020);
    }

    #[test]
    fn parse_parses_datetime() {
        let dt = parse("2024-01-15 14:30:00", "%Y-%m-%d %H:%M:%S").unwrap();
        assert_eq!(dt.year(), 2024);
        assert_eq!(dt.month(), 1);
        assert_eq!(dt.day(), 15);
        assert_eq!(dt.hour(), 14);
        assert_eq!(dt.minute(), 30);
        assert_eq!(dt.second(), 0);
    }

    #[test]
    fn parse_fails_on_invalid_format() {
        let result = parse("2024-01-15", "%Y-%m-%d %H:%M:%S");
        assert!(result.is_err());
    }

    #[test]
    fn format_formats_datetime() {
        let dt = parse("2024-01-15 14:30:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let dt_utc = DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc);
        let formatted = format(&dt_utc, "%Y/%m/%d");
        assert_eq!(formatted, "2024/01/15");
    }

    #[test]
    fn format_supports_various_patterns() {
        let dt = parse("2024-01-15 14:30:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let dt_utc = DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc);

        assert_eq!(format(&dt_utc, "%Y"), "2024");
        assert_eq!(format(&dt_utc, "%m"), "01");
        assert_eq!(format(&dt_utc, "%d"), "15");
        assert_eq!(format(&dt_utc, "%H:%M"), "14:30");
    }

    #[test]
    fn sleep_secs_sleeps() {
        use std::time::Instant;

        let start = Instant::now();
        sleep_secs(0); // Sleep for 0 seconds (should return immediately)
        let elapsed = start.elapsed();

        // Should take less than 100ms
        assert!(elapsed.as_millis() < 100);
    }

    #[test]
    fn sleep_millis_sleeps() {
        use std::time::Instant;

        let start = Instant::now();
        sleep_millis(10);
        let elapsed = start.elapsed();

        // Should take at least 10ms, but less than 100ms
        assert!(elapsed.as_millis() >= 10);
        assert!(elapsed.as_millis() < 100);
    }
}
