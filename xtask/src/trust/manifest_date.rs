use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct CalendarDate {
    year: i64,
    month: u8,
    day: u8,
}

impl CalendarDate {
    pub(super) fn parse(value: &str) -> Option<Self> {
        let bytes = value.as_bytes();
        if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
            return None;
        }
        let year = digits(&bytes[0..4])?;
        let month = u8::try_from(digits(&bytes[5..7])?).ok()?;
        let day = u8::try_from(digits(&bytes[8..10])?).ok()?;
        let valid_day = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if is_leap_year(year) => 29,
            2 => 28,
            _ => return None,
        };
        (year >= 1970 && day > 0 && day <= valid_day).then_some(Self { year, month, day })
    }

    pub(super) fn today_utc() -> Self {
        let elapsed = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
        let days = i64::try_from(elapsed.as_secs() / 86_400).unwrap_or(i64::MAX);
        from_unix_days(days)
    }
}

fn digits(bytes: &[u8]) -> Option<i64> {
    bytes.iter().try_fold(0_i64, |value, byte| {
        byte.is_ascii_digit().then(|| value * 10 + i64::from(byte - b'0'))
    })
}

const fn is_leap_year(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn from_unix_days(days: i64) -> CalendarDate {
    let shifted = days + 719_468;
    let era = if shifted >= 0 { shifted } else { shifted - 146_096 } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    CalendarDate {
        year,
        month: u8::try_from(month).expect("civil month is bounded"),
        day: u8::try_from(day).expect("civil day is bounded"),
    }
}

#[cfg(test)]
mod tests {
    use super::{CalendarDate, from_unix_days};

    #[test]
    fn validates_calendar_dates() {
        assert!(CalendarDate::parse("2028-02-29").is_some());
        for invalid in ["2026-2-01", "1969-12-31", "2026-02-29", "2026-13-01", "text"] {
            assert!(CalendarDate::parse(invalid).is_none(), "accepted `{invalid}`");
        }
    }

    #[test]
    fn converts_known_unix_days() {
        assert_eq!(from_unix_days(0), CalendarDate::parse("1970-01-01").unwrap());
        assert_eq!(from_unix_days(20_454), CalendarDate::parse("2026-01-01").unwrap());
    }
}
