//! How the views write figures and dates: shared by every tab.

/// Bytes in the largest binary unit under 1024 of them, to a tenth.
pub(super) fn file_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1_024 {
        return format!("{bytes} bytes");
    }
    let mut value = bytes as f64 / 1_024.0;
    let mut unit = 0;
    while value >= 1_024.0 && unit + 1 < UNITS.len() {
        value /= 1_024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// A length of time as people say it, to the minute past one: "2 hr 13
/// min", "48 min", "35 sec".
pub(super) fn total_duration(milliseconds: u64) -> String {
    let seconds = milliseconds / 1_000;
    let (hours, minutes) = (seconds / 3_600, seconds / 60 % 60);
    match (hours, minutes) {
        (0, 0) => format!("{seconds} sec"),
        (0, minutes) => format!("{minutes} min"),
        (hours, 0) => format!("{hours} hr"),
        (hours, minutes) => format!("{hours} hr {minutes} min"),
    }
}

/// A time in nanoseconds since 1970, in UTC: "2023-11-14 22:13 UTC".
/// None for one at or before 1970, which no file was written at.
pub(super) fn timestamp(nanoseconds: i64) -> Option<String> {
    if nanoseconds <= 0 {
        return None;
    }
    let seconds = nanoseconds.div_euclid(1_000_000_000);
    let (days, of_day) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));
    let (year, month, day) = civil_from_days(days);
    Some(format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        of_day / 3_600,
        of_day / 60 % 60
    ))
}

/// The proleptic Gregorian date `days` after 1970-01-01 (Howard
/// Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let of_era = days.rem_euclid(146_097);
    let year_of_era = (of_era - of_era / 1_460 + of_era / 36_524 - of_era / 146_096) / 365;
    let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * of_year + 2) / 153;
    let day = (of_year - (153 * shifted_month + 2) / 5 + 1) as u32;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    } as u32;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_dates_read_as_people_write_them() {
        assert_eq!(file_size(512), "512 bytes");
        assert_eq!(file_size(8_812_345), "8.4 MB");
        assert_eq!(timestamp(0), None);
        assert_eq!(timestamp(1), Some("1970-01-01 00:00 UTC".to_owned()));
        assert_eq!(
            timestamp(1_700_000_000 * 1_000_000_000),
            Some("2023-11-14 22:13 UTC".to_owned())
        );
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(total_duration(35_999), "35 sec");
        assert_eq!(total_duration(48 * 60_000 + 59_000), "48 min");
        assert_eq!(total_duration(7_200_000), "2 hr");
        assert_eq!(total_duration(7_980_000), "2 hr 13 min");
    }
}
