//! Strict UTC review-timestamp validation.

use crate::trust::manifest_date::CalendarDate;

pub(super) fn is_full_utc(value: &str, expected_date: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() < 20
        || bytes.get(10) != Some(&b'T')
        || bytes.last() != Some(&b'Z')
        || bytes.get(..10) != Some(expected_date.as_bytes())
        || CalendarDate::parse(expected_date).is_none()
        || bytes.get(13) != Some(&b':')
        || bytes.get(16) != Some(&b':')
    {
        return false;
    }
    let Some(hour) = decimal_pair(bytes, 11) else { return false };
    let Some(minute) = decimal_pair(bytes, 14) else { return false };
    let Some(second) = decimal_pair(bytes, 17) else { return false };
    if hour > 23 || minute > 59 || second > 59 {
        return false;
    }
    match &bytes[19..bytes.len() - 1] {
        [] => true,
        [b'.', fraction @ ..] => {
            !fraction.is_empty() && fraction.len() <= 9 && fraction.iter().all(u8::is_ascii_digit)
        }
        _ => false,
    }
}

fn decimal_pair(bytes: &[u8], offset: usize) -> Option<u8> {
    let high = bytes.get(offset)?.checked_sub(b'0')?;
    let low = bytes.get(offset + 1)?.checked_sub(b'0')?;
    (high <= 9 && low <= 9).then_some(high * 10 + low)
}

#[cfg(test)]
mod tests {
    use super::is_full_utc;

    #[test]
    fn accepts_exact_utc_and_rejects_offsets_dates_and_invalid_time() {
        assert!(is_full_utc("2026-08-22T23:59:59Z", "2026-08-22"));
        assert!(is_full_utc("2026-08-22T00:00:00.123456789Z", "2026-08-22"));
        for invalid in [
            "2026-08-22",
            "2026-08-22T24:00:00Z",
            "2026-08-22T00:60:00Z",
            "2026-08-22T00:00:60Z",
            "2026-08-22T00:00:00+00:00",
            "2026-08-22T00:00:00.Z",
            "2026-08-23T00:00:00Z",
        ] {
            assert!(!is_full_utc(invalid, "2026-08-22"), "accepted `{invalid}`");
        }
    }
}
