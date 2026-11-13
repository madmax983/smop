//! The retry policy and the `Retry-After` parser.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The first wait if you do not set one. It doubles after each retry.
const DEFAULT_DELAY: Duration = Duration::from_secs(1);

/// The longest wait if you do not set one.
pub(super) const DEFAULT_MAX_DELAY: Duration = Duration::from_secs(60);

/// Methods that do the same thing when sent two times (RFC 9110, 9.2.2).
const IDEMPOTENT: [&str; 6] = ["GET", "HEAD", "PUT", "DELETE", "OPTIONS", "TRACE"];

/// When and how often to send a request again.
///
/// A retry occurs after a timeout, or after status 408, 429, 500, 502, 503
/// or 504. These are the cases of `curl --retry`. Other errors and statuses
/// do not cause a retry.
///
/// The first wait is [`delay`](Self::delay) (1 s). The wait doubles after
/// each retry. A `Retry-After` header (seconds or HTTP-date) replaces the
/// wait. [`max_delay`](Self::max_delay) (60 s) is the longest wait.
///
/// Only GET, HEAD, PUT, DELETE, OPTIONS and TRACE retry.
/// [`any_method`](Self::any_method) adds POST, PATCH and all other methods.
///
/// The client timeout applies to each attempt. Each retry writes one
/// `warning:` line to stderr.
///
/// # Examples
///
/// ```no_run
/// use smop::http;
///
/// // Like `curl -fsSL --retry 3 -o tool.tar.gz URL`.
/// http::Client::new()
///     .retry(http::Retry::new(3))
///     .download("https://example.com/tool.tar.gz", "tool.tar.gz")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct Retry {
    retries: u32,
    delay: Duration,
    max_delay: Duration,
    any_method: bool,
}

impl Retry {
    /// Sends a request at most `retries` more times after the first attempt.
    /// `Retry::new(0)` does not retry.
    pub const fn new(retries: u32) -> Self {
        Self {
            retries,
            delay: DEFAULT_DELAY,
            max_delay: DEFAULT_MAX_DELAY,
            any_method: false,
        }
    }

    /// Sets the first wait. The wait doubles after each retry.
    pub const fn delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    /// Sets the longest wait. It also limits the `Retry-After` wait.
    pub const fn max_delay(mut self, max_delay: Duration) -> Self {
        self.max_delay = max_delay;
        self
    }

    /// Also retries POST, PATCH and other methods that are not idempotent.
    /// Use this only if the server can get the same request two times.
    pub const fn any_method(mut self) -> Self {
        self.any_method = true;
        self
    }

    /// The number of retries for `method`.
    pub(super) fn retries_for(&self, method: &str) -> u32 {
        if self.allows(method) { self.retries } else { 0 }
    }

    /// Whether `method` (in uppercase) can retry.
    pub(super) fn allows(&self, method: &str) -> bool {
        self.any_method || IDEMPOTENT.contains(&method)
    }

    /// The wait before retry `n` (from 1). `retry_after` replaces the backoff.
    pub(super) fn wait(&self, n: u32, retry_after: Option<Duration>) -> Duration {
        let factor = 2u32.checked_pow(n.saturating_sub(1)).unwrap_or(u32::MAX);
        retry_after
            .unwrap_or_else(|| self.delay.saturating_mul(factor))
            .min(self.max_delay)
    }
}

/// Whether a response with `status` can retry.
pub(super) const fn retryable_status(status: u16) -> bool {
    matches!(status, 408 | 429 | 500 | 502 | 503 | 504)
}

/// Parses a `Retry-After` value into the wait from `now`.
///
/// The value is seconds or an HTTP-date. A date before `now` gives zero.
/// A value that is not valid gives `None`.
pub(super) fn retry_after(value: &str, now: SystemTime) -> Option<Duration> {
    let value = value.trim();
    if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) {
        // Only digits, so a parse error is an overflow.
        return Some(Duration::from_secs(value.parse().unwrap_or(u64::MAX)));
    }
    let secs = http_date(value)?;
    let Ok(secs) = u64::try_from(secs) else {
        return Some(Duration::ZERO);
    };
    let date = UNIX_EPOCH.checked_add(Duration::from_secs(secs))?;
    Some(date.duration_since(now).unwrap_or(Duration::ZERO))
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const LONG_DAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

/// Parses an HTTP-date into seconds from the Unix epoch.
///
/// RFC 9110, 5.6.7, tells recipients to accept three forms:
/// `Sun, 06 Nov 1994 08:49:37 GMT`, `Sunday, 06-Nov-94 08:49:37 GMT` and
/// `Sun Nov  6 08:49:37 1994`.
fn http_date(value: &str) -> Option<i64> {
    let parts: Vec<&str> = value.split_ascii_whitespace().collect();
    let (year, month, day, time) = match parts.as_slice() {
        [name, day, month, year, time, "GMT"] => {
            DAYS.contains(&name.strip_suffix(',')?).then_some(())?;
            (
                digits(year, 4)?,
                month_number(month)?,
                digits(day, 2)?,
                *time,
            )
        }
        [name, date, time, "GMT"] => {
            LONG_DAYS.contains(&name.strip_suffix(',')?).then_some(())?;
            let mut date = date.split('-');
            let (day, month, year) = (date.next()?, date.next()?, date.next()?);
            date.next().is_none().then_some(())?;
            let year = digits(year, 2)?;
            let year = if year < 70 { 2000 + year } else { 1900 + year };
            (year, month_number(month)?, digits(day, 2)?, *time)
        }
        [name, month, day, time, year] => {
            DAYS.contains(name).then_some(())?;
            let day = digits(day, 1).or_else(|| digits(day, 2))?;
            (digits(year, 4)?, month_number(month)?, day, *time)
        }
        _ => return None,
    };
    let (hour, minute, second) = clock(time)?;
    if day == 0 || day > days_in_month(year, month) {
        return None;
    }
    let days = days_from_civil(i64::from(year), month, day);
    Some(days * 86_400 + i64::from(hour * 3600 + minute * 60 + second))
}

/// Parses exactly `len` ASCII digits.
fn digits(s: &str, len: usize) -> Option<u32> {
    (s.len() == len && s.bytes().all(|b| b.is_ascii_digit()))
        .then(|| s.parse().ok())
        .flatten()
}

/// `Jan` is 1.
fn month_number(s: &str) -> Option<u32> {
    let i = MONTHS.iter().position(|m| *m == s)?;
    u32::try_from(i + 1).ok()
}

/// Parses `HH:MM:SS`. Second 60 is a leap second.
fn clock(s: &str) -> Option<(u32, u32, u32)> {
    let mut parts = s.split(':');
    let hour = digits(parts.next()?, 2)?;
    let minute = digits(parts.next()?, 2)?;
    let second = digits(parts.next()?, 2)?;
    (parts.next().is_none() && hour < 24 && minute < 60 && second <= 60)
        .then_some((hour, minute, second))
}

const fn is_leap(year: u32) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

const fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days from 1970-01-01 to a date in the proleptic Gregorian calendar
/// (Howard Hinnant's `days_from_civil`).
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let shifted = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * shifted + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    /// RFC 9110, section 5.6.7: the same time in the three forms.
    const RFC_TIME: u64 = 784_111_777;

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    #[test]
    fn retryable_statuses_are_the_curl_set() {
        for code in [408, 429, 500, 502, 503, 504] {
            assert!(retryable_status(code), "{code}");
        }
        for code in [200, 301, 400, 401, 403, 404, 409, 422, 501, 505, 599] {
            assert!(!retryable_status(code), "{code}");
        }
    }

    #[test]
    fn only_idempotent_methods_retry_by_default() {
        let retry = Retry::new(3);
        for method in ["GET", "HEAD", "PUT", "DELETE", "OPTIONS", "TRACE"] {
            assert!(retry.allows(method), "{method}");
        }
        for method in ["POST", "PATCH", "CONNECT", "PURGE"] {
            assert!(!retry.allows(method), "{method}");
        }
        for method in ["POST", "PATCH", "CONNECT", "PURGE"] {
            assert!(retry.any_method().allows(method), "{method}");
        }
    }

    #[test]
    fn backoff_doubles_from_delay_and_stops_at_max_delay() {
        let retry = Retry::new(10)
            .delay(Duration::from_secs(1))
            .max_delay(Duration::from_secs(5));
        let waits: Vec<u64> = (1..=5).map(|n| retry.wait(n, None).as_secs()).collect();
        assert_eq!(waits, [1, 2, 4, 5, 5]);
        assert_eq!(retry.wait(u32::MAX, None), Duration::from_secs(5));
    }

    #[test]
    fn retry_after_replaces_backoff_and_is_capped() {
        let retry = Retry::new(3)
            .delay(Duration::from_secs(1))
            .max_delay(Duration::from_secs(60));
        let after = |s| Some(Duration::from_secs(s));
        assert_eq!(retry.wait(3, after(0)), Duration::ZERO);
        assert_eq!(retry.wait(1, after(30)), Duration::from_secs(30));
        assert_eq!(retry.wait(1, after(3600)), Duration::from_secs(60));
    }

    #[test]
    fn defaults_match_curl() {
        let retry = Retry::new(3);
        assert_eq!(retry.wait(1, None), Duration::from_secs(1));
        assert_eq!(retry.wait(2, None), Duration::from_secs(2));
        assert_eq!(retry.wait(1, Some(Duration::MAX)), DEFAULT_MAX_DELAY);
    }

    #[test]
    fn retry_after_seconds() {
        let now = at(RFC_TIME);
        assert_eq!(retry_after("0", now), Some(Duration::ZERO));
        assert_eq!(retry_after("120", now), Some(Duration::from_secs(120)));
        assert_eq!(retry_after(" 7 ", now), Some(Duration::from_secs(7)));
        assert_eq!(
            retry_after("99999999999999999999999", now),
            Some(Duration::from_secs(u64::MAX))
        );
        for bad in ["", "-1", "+1", "1.5", "1e3", "abc", "1 2"] {
            assert_eq!(retry_after(bad, now), None, "{bad:?}");
        }
    }

    #[test]
    fn retry_after_http_date_in_all_three_forms() {
        let now = at(RFC_TIME - 10);
        for form in [
            "Sun, 06 Nov 1994 08:49:37 GMT",
            "Sunday, 06-Nov-94 08:49:37 GMT",
            "Sun Nov  6 08:49:37 1994",
        ] {
            assert_eq!(
                retry_after(form, now),
                Some(Duration::from_secs(10)),
                "{form}"
            );
        }
    }

    #[test]
    fn retry_after_date_in_the_past_is_zero() {
        let now = at(RFC_TIME + 100);
        assert_eq!(
            retry_after("Sun, 06 Nov 1994 08:49:37 GMT", now),
            Some(Duration::ZERO)
        );
    }

    #[test]
    fn retry_after_dates_after_2000_and_leap_days() {
        // 2024-02-29T00:00:00Z and 2000-03-01T12:30:05Z.
        let now = UNIX_EPOCH;
        assert_eq!(
            retry_after("Thu, 29 Feb 2024 00:00:00 GMT", now),
            Some(Duration::from_secs(1_709_164_800))
        );
        assert_eq!(
            retry_after("Wed, 01 Mar 2000 12:30:05 GMT", now),
            Some(Duration::from_secs(951_913_805))
        );
        // RFC 850 years below 70 are 20xx.
        assert_eq!(
            retry_after("Thursday, 29-Feb-24 00:00:00 GMT", now),
            Some(Duration::from_secs(1_709_164_800))
        );
    }

    #[test]
    fn retry_after_rejects_bad_dates() {
        let now = UNIX_EPOCH;
        for bad in [
            "Sun, 06 Nov 1994 08:49:37 UTC",
            "Sun, 06 Nov 1994 08:49:37",
            "Sun, 32 Nov 1994 08:49:37 GMT",
            "Sun, 31 Nov 1994 08:49:37 GMT",
            "Mon, 29 Feb 2023 00:00:00 GMT",
            "Sun, 06 Foo 1994 08:49:37 GMT",
            "Sun, 06 Nov 1994 24:00:00 GMT",
            "Sun, 06 Nov 1994 08:60:00 GMT",
            "Sun, 06 Nov 1994 08:49:61 GMT",
            "Sun, 6 Nov 1994 8:49:37 GMT",
            "Sun Nov 6 08:49:37 1994 extra",
            "Sunday, 06-Nov-1994 08:49:37 GMT",
            "garbage",
        ] {
            assert_eq!(retry_after(bad, now), None, "{bad:?}");
        }
    }

    #[test]
    fn retry_after_date_before_1970_is_zero() {
        let now = at(1);
        assert_eq!(
            retry_after("Thu, 01 Jan 1960 00:00:00 GMT", now),
            Some(Duration::ZERO)
        );
    }

    #[test]
    fn retry_after_leap_second_is_accepted() {
        // RFC 9110 allows second 60.
        assert_eq!(
            retry_after("Sat, 31 Dec 2016 23:59:60 GMT", UNIX_EPOCH),
            Some(Duration::from_secs(1_483_228_800))
        );
    }
}
