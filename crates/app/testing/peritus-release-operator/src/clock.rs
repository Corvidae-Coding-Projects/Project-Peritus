//! UTC timestamp conversion without ambient locale state.

use peritus_release_artifacts::SpdxTimestamp;

use crate::error::OperatorError;

pub fn timestamp(unix_seconds: u64) -> Result<SpdxTimestamp, OperatorError> {
    let seconds = i64::try_from(unix_seconds)
        .map_err(|_| OperatorError::metadata("release timestamp exceeds the supported range"))?;
    let days = seconds.div_euclid(86_400);
    let day_seconds = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    if !(1970..=9999).contains(&year) {
        return Err(OperatorError::metadata("release timestamp year is outside 1970 through 9999"));
    }
    let hour = day_seconds / 3_600;
    let minute = day_seconds % 3_600 / 60;
    let second = day_seconds % 60;
    SpdxTimestamp::new(format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z"))
        .map_err(OperatorError::from)
}

fn civil_from_days(days_since_epoch: i64) -> (i64, i64, i64) {
    let shifted = days_since_epoch + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::timestamp;
    use serde_json::to_string;

    #[test]
    fn epoch_and_leap_day_are_exact() {
        assert_eq!(
            to_string(&timestamp(0).expect("epoch")).expect("JSON"),
            "\"1970-01-01T00:00:00Z\""
        );
        assert_eq!(
            to_string(&timestamp(1_709_164_800).expect("leap day")).expect("JSON"),
            "\"2024-02-29T00:00:00Z\""
        );
    }
}
