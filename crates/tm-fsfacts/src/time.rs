//! Whole-second RFC 3339 UTC times with a separate nanosecond part (11.1).

use std::cmp::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

/// A point in time as the filesystem reports it: seconds since the Unix
/// epoch and nanoseconds within the second.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Time {
    /// Seconds since 1970-01-01T00:00:00Z (may be negative).
    pub secs: i64,
    /// Nanoseconds within the second, 0 to 999,999,999.
    pub nanos: u32,
}

impl PartialOrd for Time {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Time {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.secs, self.nanos).cmp(&(other.secs, other.nanos))
    }
}

impl Time {
    /// From a `statx` or `stat` timestamp (`tv_sec`, `tv_nsec`).
    #[must_use]
    pub fn new(secs: i64, nanos: u32) -> Time {
        Time { secs, nanos }
    }

    /// Now, from the system clock.
    #[must_use]
    pub fn now() -> Time {
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(d) => Time {
                secs: i64::try_from(d.as_secs()).unwrap_or(i64::MAX),
                nanos: d.subsec_nanos(),
            },
            Err(e) => {
                let d = e.duration();
                let (secs, nanos) = if d.subsec_nanos() == 0 {
                    (-i64::try_from(d.as_secs()).unwrap_or(i64::MAX), 0)
                } else {
                    (
                        -i64::try_from(d.as_secs()).unwrap_or(i64::MAX) - 1,
                        1_000_000_000 - d.subsec_nanos(),
                    )
                };
                Time { secs, nanos }
            }
        }
    }

    /// From a `SystemTime`.
    #[must_use]
    pub fn from_system(t: SystemTime) -> Time {
        match t.duration_since(UNIX_EPOCH) {
            Ok(d) => Time {
                secs: i64::try_from(d.as_secs()).unwrap_or(i64::MAX),
                nanos: d.subsec_nanos(),
            },
            Err(e) => {
                let d = e.duration();
                if d.subsec_nanos() == 0 {
                    Time {
                        secs: -i64::try_from(d.as_secs()).unwrap_or(i64::MAX),
                        nanos: 0,
                    }
                } else {
                    Time {
                        secs: -i64::try_from(d.as_secs()).unwrap_or(i64::MAX) - 1,
                        nanos: 1_000_000_000 - d.subsec_nanos(),
                    }
                }
            }
        }
    }

    /// The whole-second part as `YYYY-MM-DDTHH:MM:SSZ` (11.2's `at` form).
    #[must_use]
    pub fn rfc3339_seconds(&self) -> String {
        let days = self.secs.div_euclid(86_400);
        let rem = self.secs.rem_euclid(86_400);
        let (y, m, d) = civil_from_days(days);
        format!(
            "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
            rem / 3600,
            (rem % 3600) / 60,
            rem % 60
        )
    }

    /// Parse the `at` form back to whole seconds (nanoseconds 0).
    #[must_use]
    pub fn parse_rfc3339_seconds(text: &str) -> Option<Time> {
        let bytes = text.as_bytes();
        let shaped = bytes.len() == 20
            && bytes[4] == b'-'
            && bytes[7] == b'-'
            && bytes[10] == b'T'
            && bytes[13] == b':'
            && bytes[16] == b':'
            && bytes[19] == b'Z';
        if !shaped {
            return None;
        }
        let num = |from: usize, to: usize| text[from..to].parse::<i64>().ok();
        let (year, month, day) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
        let (hour, minute, second) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
        if !(1..=12).contains(&month)
            || !(1..=31).contains(&day)
            || hour > 23
            || minute > 59
            || second > 59
        {
            return None;
        }
        let days = days_from_civil(year, month, day);
        if civil_from_days(days) != (year, month, day) {
            return None;
        }
        Some(Time {
            secs: days * 86_400 + hour * 3600 + minute * 60 + second,
            nanos: 0,
        })
    }
}

/// Howard Hinnant's `civil_from_days`: days since 1970-01-01 to (y, m, d).
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// The inverse of [`civil_from_days`].
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_and_parses() {
        let t = Time::new(1_762_696_931, 5);
        assert_eq!(t.rfc3339_seconds(), "2025-11-09T14:02:11Z");
        assert_eq!(
            Time::parse_rfc3339_seconds("2025-11-09T14:02:11Z"),
            Some(Time::new(1_762_696_931, 0))
        );
        assert_eq!(Time::new(0, 0).rfc3339_seconds(), "1970-01-01T00:00:00Z");
        assert_eq!(Time::new(-1, 0).rfc3339_seconds(), "1969-12-31T23:59:59Z");
        assert_eq!(
            Time::new(951_782_400, 0).rfc3339_seconds(),
            "2000-02-29T00:00:00Z"
        );
        assert_eq!(Time::parse_rfc3339_seconds("2001-02-29T00:00:00Z"), None);
        assert_eq!(Time::parse_rfc3339_seconds("2025-11-09T14:02:11"), None);
        for secs in [-86_401, -1, 0, 86_399, 86_400, 1_700_000_000, 4_102_444_800] {
            let s = Time::new(secs, 0).rfc3339_seconds();
            assert_eq!(
                Time::parse_rfc3339_seconds(&s),
                Some(Time::new(secs, 0)),
                "{s}"
            );
        }
        assert!(Time::new(1, 0) < Time::new(1, 1));
        assert!(Time::now().secs > 1_700_000_000);
    }
}
