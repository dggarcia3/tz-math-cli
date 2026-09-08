//! Parsing, arithmetic and printing for civil timestamps that carry a fixed
//! UTC offset, e.g. `2024-03-10T14:30:00-05:00`. There is no timezone
//! database here (no DST rules, no IANA names) - just the offset math that
//! every higher-level timezone tool eventually has to get right: turning a
//! local wall-clock reading plus an offset into an absolute instant, and
//! back again under a different offset.

use std::fmt;

pub type ParseResult<T> = Result<T, ParseError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(pub String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ParseError {}

fn err<T>(msg: impl Into<String>) -> ParseResult<T> {
    Err(ParseError(msg.into()))
}

/// A fixed UTC offset stored as signed minutes. Real-world offsets run from
/// -12:00 (Baker Island) to +14:00 (Kiribati), so that is what we validate
/// against rather than accepting the full +/-24:00 a naive parser would.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Offset {
    minutes: i32,
}

impl Offset {
    pub const UTC: Offset = Offset { minutes: 0 };

    pub fn from_minutes(minutes: i32) -> ParseResult<Offset> {
        if minutes < -12 * 60 || minutes > 14 * 60 {
            return err(format!(
                "offset of {minutes} minutes is outside the real-world range -12:00..=+14:00"
            ));
        }
        Ok(Offset { minutes })
    }

    pub fn minutes(&self) -> i32 {
        self.minutes
    }

    pub fn parse(s: &str) -> ParseResult<Offset> {
        if s == "Z" || s == "z" {
            return Ok(Offset::UTC);
        }
        if !s.is_ascii() || s.len() != 6 || s.as_bytes()[3] != b':' {
            return err(format!("bad offset '{s}', expected +HH:MM, -HH:MM or Z"));
        }
        let bytes = s.as_bytes();
        let sign: i32 = match bytes[0] {
            b'+' => 1,
            b'-' => -1,
            _ => return err(format!("offset '{s}' must start with + or -")),
        };
        let hh: i32 = s[1..3]
            .parse()
            .map_err(|_| ParseError(format!("bad offset hours in '{s}'")))?;
        let mm: i32 = s[4..6]
            .parse()
            .map_err(|_| ParseError(format!("bad offset minutes in '{s}'")))?;
        if mm > 59 {
            return err(format!("offset minutes {mm} out of range 0..=59"));
        }
        Offset::from_minutes(sign * (hh * 60 + mm))
    }
}

impl fmt::Display for Offset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.minutes == 0 {
            return write!(f, "Z");
        }
        let sign = if self.minutes < 0 { '-' } else { '+' };
        let abs = self.minutes.abs();
        write!(f, "{sign}{:02}:{:02}", abs / 60, abs % 60)
    }
}

/// A civil (wall-clock) date and time paired with the offset it was read
/// under. Two `DateTime`s can represent the same instant while disagreeing
/// on every field except `to_epoch_seconds()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    /// Fraction of the second, in nanoseconds (0..=999_999_999).
    pub nanosecond: u32,
    pub offset: Offset,
}

fn is_leap_year(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(y) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

/// Days since 1970-01-01 for a proleptic Gregorian civil date.
/// Howard Hinnant's `days_from_civil` algorithm: exact for all i32 years.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y / 400 } else { (y - 399) / 400 };
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Inverse of `days_from_civil`.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z / 146097 } else { (z - 146096) / 146097 };
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

impl DateTime {
    /// Parses `YYYY-MM-DDTHH:MM:SS` (a space instead of `T` is also
    /// accepted), an optional `.` followed by 1-9 fractional-second digits,
    /// then `Z` or a `+HH:MM` / `-HH:MM` offset. Every field is
    /// range-checked, including day-of-month against the actual length of
    /// that month in that year. Fractional digits beyond nanosecond
    /// precision are truncated rather than rejected.
    pub fn parse(s: &str) -> ParseResult<DateTime> {
        let s = s.trim();
        if !s.is_ascii() {
            return err(format!("'{s}' contains non-ASCII bytes"));
        }
        if s.len() < 20 {
            return err(format!("'{s}' is too short to be a timestamp"));
        }
        let bytes = s.as_bytes();
        if bytes[10] != b'T' && bytes[10] != b't' && bytes[10] != b' ' {
            return err(format!("expected 'T' or a space at position 10 in '{s}'"));
        }
        if bytes[4] != b'-' || bytes[7] != b'-' {
            return err(format!("expected '-' separators in the date part of '{s}'"));
        }
        if bytes[13] != b':' || bytes[16] != b':' {
            return err(format!("expected ':' separators in the time part of '{s}'"));
        }

        let year: i32 = s[0..4]
            .parse()
            .map_err(|_| ParseError(format!("bad year in '{s}'")))?;
        let month: u32 = s[5..7]
            .parse()
            .map_err(|_| ParseError(format!("bad month in '{s}'")))?;
        let day: u32 = s[8..10]
            .parse()
            .map_err(|_| ParseError(format!("bad day in '{s}'")))?;
        let hour: u32 = s[11..13]
            .parse()
            .map_err(|_| ParseError(format!("bad hour in '{s}'")))?;
        let minute: u32 = s[14..16]
            .parse()
            .map_err(|_| ParseError(format!("bad minute in '{s}'")))?;
        let second: u32 = s[17..19]
            .parse()
            .map_err(|_| ParseError(format!("bad second in '{s}'")))?;
        let mut idx = 19;
        let mut nanosecond: u32 = 0;
        if bytes.get(idx) == Some(&b'.') {
            idx += 1;
            let start = idx;
            while bytes.get(idx).is_some_and(|b| b.is_ascii_digit()) {
                idx += 1;
            }
            if idx == start {
                return err(format!("expected digits after '.' in '{s}'"));
            }
            let frac = &s[start..idx.min(start + 9)];
            let mut nanos: u32 = frac
                .parse()
                .map_err(|_| ParseError(format!("bad fractional seconds in '{s}'")))?;
            for _ in frac.len()..9 {
                nanos *= 10;
            }
            nanosecond = nanos;
        }
        let offset = Offset::parse(&s[idx..])?;

        if !(1..=12).contains(&month) {
            return err(format!("month {month} out of range 1..=12"));
        }
        let max_day = days_in_month(year, month);
        if day < 1 || day > max_day {
            return err(format!("day {day} out of range for {year}-{month:02}"));
        }
        if hour > 23 {
            return err(format!("hour {hour} out of range 0..=23"));
        }
        if minute > 59 {
            return err(format!("minute {minute} out of range 0..=59"));
        }
        if second > 59 {
            return err(format!("second {second} out of range 0..=59"));
        }

        Ok(DateTime { year, month, day, hour, minute, second, nanosecond, offset })
    }

    /// Seconds since the Unix epoch, i.e. the offset-independent instant.
    pub fn to_epoch_seconds(&self) -> i64 {
        let days = days_from_civil(self.year as i64, self.month as i64, self.day as i64);
        let local = days * 86400
            + self.hour as i64 * 3600
            + self.minute as i64 * 60
            + self.second as i64;
        local - self.offset.minutes() as i64 * 60
    }

    /// Builds the wall-clock reading a clock set to `offset` would show at
    /// the given instant. The nanosecond field is always zero; instants
    /// only carry whole seconds.
    pub fn from_epoch_seconds(epoch: i64, offset: Offset) -> DateTime {
        let local = epoch + offset.minutes() as i64 * 60;
        let days = local.div_euclid(86400);
        let secs_of_day = local.rem_euclid(86400);
        let (y, m, d) = civil_from_days(days);
        DateTime {
            year: y as i32,
            month: m as u32,
            day: d as u32,
            hour: (secs_of_day / 3600) as u32,
            minute: (secs_of_day / 60 % 60) as u32,
            second: (secs_of_day % 60) as u32,
            nanosecond: 0,
            offset,
        }
    }

    /// Re-expresses the same instant under a different offset.
    pub fn with_offset(&self, offset: Offset) -> DateTime {
        DateTime { nanosecond: self.nanosecond, ..DateTime::from_epoch_seconds(self.to_epoch_seconds(), offset) }
    }

    /// Adds a (possibly negative) number of seconds, keeping the offset and
    /// the fractional second unchanged.
    pub fn add_seconds(&self, delta: i64) -> DateTime {
        DateTime {
            nanosecond: self.nanosecond,
            ..DateTime::from_epoch_seconds(self.to_epoch_seconds() + delta, self.offset)
        }
    }
}

impl fmt::Display for DateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )?;
        if self.nanosecond != 0 {
            let mut frac = format!("{:09}", self.nanosecond);
            while frac.ends_with('0') {
                frac.pop();
            }
            write!(f, ".{frac}")?;
        }
        write!(f, "{}", self.offset)
    }
}

/// Parses a signed duration made of `d`/`h`/`m`/`s` components, e.g.
/// `3h30m`, `-90m`, `1d`. Returns the total in seconds.
pub fn parse_duration(s: &str) -> ParseResult<i64> {
    let s = s.trim();
    if s.is_empty() {
        return err("duration is empty");
    }
    let (sign, rest): (i64, &str) = match s.as_bytes()[0] {
        b'-' => (-1, &s[1..]),
        b'+' => (1, &s[1..]),
        _ => (1, s),
    };
    if rest.is_empty() {
        return err(format!("duration '{s}' has a sign but no digits"));
    }

    let mut total: i64 = 0;
    let mut num = String::new();
    for c in rest.chars() {
        if c.is_ascii_digit() {
            num.push(c);
            continue;
        }
        if num.is_empty() {
            return err(format!("duration '{s}' is missing a number before '{c}'"));
        }
        let n: i64 = num
            .parse()
            .map_err(|_| ParseError(format!("bad number in duration '{s}'")))?;
        let unit_seconds = match c {
            'd' => 86400,
            'h' => 3600,
            'm' => 60,
            's' => 1,
            _ => return err(format!("unknown duration unit '{c}' in '{s}'")),
        };
        total += n * unit_seconds;
        num.clear();
    }
    if !num.is_empty() {
        return err(format!("duration '{s}' has trailing digits with no unit"));
    }
    Ok(sign * total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_timestamp() {
        let dt = DateTime::parse("2024-03-10T14:30:00-05:00").unwrap();
        assert_eq!(dt.year, 2024);
        assert_eq!(dt.month, 3);
        assert_eq!(dt.day, 10);
        assert_eq!(dt.hour, 14);
        assert_eq!(dt.minute, 30);
        assert_eq!(dt.second, 0);
        assert_eq!(dt.offset.minutes(), -5 * 60);
    }

    #[test]
    fn accepts_space_in_place_of_t() {
        let dt = DateTime::parse("2024-03-10 14:30:00Z").unwrap();
        assert_eq!(dt.hour, 14);
    }

    #[test]
    fn accepts_lowercase_t_and_z() {
        let dt = DateTime::parse("2024-03-10t14:30:00z").unwrap();
        assert_eq!(dt.offset, Offset::UTC);
    }

    #[test]
    fn trims_surrounding_whitespace() {
        let dt = DateTime::parse("  2024-03-10T14:30:00Z  ").unwrap();
        assert_eq!(dt.day, 10);
    }

    #[test]
    fn rejects_non_ascii() {
        assert!(DateTime::parse("2024-03-10T14:30:00\u{2103}").is_err());
    }

    #[test]
    fn rejects_too_short_input() {
        assert!(DateTime::parse("2024-03-10T14:30").is_err());
    }

    #[test]
    fn rejects_bad_date_separators() {
        assert!(DateTime::parse("2024/03/10T14:30:00Z").is_err());
    }

    #[test]
    fn rejects_bad_time_separators() {
        assert!(DateTime::parse("2024-03-10T14-30-00Z").is_err());
    }

    #[test]
    fn rejects_missing_t_or_space() {
        assert!(DateTime::parse("2024-03-10X14:30:00Z").is_err());
    }

    #[test]
    fn rejects_month_out_of_range() {
        assert!(DateTime::parse("2024-00-10T14:30:00Z").is_err());
        assert!(DateTime::parse("2024-13-10T14:30:00Z").is_err());
    }

    #[test]
    fn rejects_hour_minute_second_out_of_range() {
        assert!(DateTime::parse("2024-03-10T24:00:00Z").is_err());
        assert!(DateTime::parse("2024-03-10T00:60:00Z").is_err());
        assert!(DateTime::parse("2024-03-10T00:00:60Z").is_err());
    }

    #[test]
    fn leap_year_feb_29_is_valid_on_divisible_by_4() {
        assert!(DateTime::parse("2024-02-29T00:00:00Z").is_ok());
    }

    #[test]
    fn feb_29_is_invalid_on_non_leap_year() {
        assert!(DateTime::parse("2023-02-29T00:00:00Z").is_err());
    }

    #[test]
    fn century_year_divisible_by_400_is_leap() {
        assert!(DateTime::parse("2000-02-29T00:00:00Z").is_ok());
    }

    #[test]
    fn century_year_not_divisible_by_400_is_not_leap() {
        assert!(DateTime::parse("1900-02-29T00:00:00Z").is_err());
    }

    #[test]
    fn rejects_day_zero_and_day_31_in_short_month() {
        assert!(DateTime::parse("2024-04-00T00:00:00Z").is_err());
        assert!(DateTime::parse("2024-04-31T00:00:00Z").is_err());
    }

    #[test]
    fn parses_z_offset() {
        assert_eq!(Offset::parse("Z").unwrap(), Offset::UTC);
        assert_eq!(Offset::parse("z").unwrap(), Offset::UTC);
    }

    #[test]
    fn parses_offset_extremes() {
        assert_eq!(Offset::parse("-12:00").unwrap().minutes(), -12 * 60);
        assert_eq!(Offset::parse("+14:00").unwrap().minutes(), 14 * 60);
    }

    #[test]
    fn rejects_offset_beyond_real_world_range() {
        assert!(Offset::parse("-12:01").is_err());
        assert!(Offset::parse("+14:01").is_err());
    }

    #[test]
    fn rejects_offset_minutes_out_of_range() {
        assert!(Offset::parse("+05:60").is_err());
    }

    #[test]
    fn rejects_malformed_offset() {
        assert!(Offset::parse("+5:00").is_err());
        assert!(Offset::parse("0500").is_err());
        assert!(Offset::parse("+05:00:00").is_err());
    }

    #[test]
    fn offset_display_round_trips() {
        assert_eq!(Offset::parse("+09:30").unwrap().to_string(), "+09:30");
        assert_eq!(Offset::parse("-05:00").unwrap().to_string(), "-05:00");
        assert_eq!(Offset::UTC.to_string(), "Z");
    }

    #[test]
    fn with_offset_preserves_instant() {
        let dt = DateTime::parse("2024-03-10T14:30:00-05:00").unwrap();
        let converted = dt.with_offset(Offset::parse("+09:00").unwrap());
        assert_eq!(dt.to_epoch_seconds(), converted.to_epoch_seconds());
        assert_eq!(converted.to_string(), "2024-03-11T04:30:00+09:00");
    }

    #[test]
    fn add_seconds_rolls_over_month_and_year() {
        let dt = DateTime::parse("2023-12-31T23:30:00Z").unwrap();
        let later = dt.add_seconds(3600);
        assert_eq!(later.to_string(), "2024-01-01T00:30:00Z");
    }

    #[test]
    fn add_seconds_rolls_over_leap_day() {
        let dt = DateTime::parse("2024-02-28T23:00:00Z").unwrap();
        let later = dt.add_seconds(3600 * 2);
        assert_eq!(later.to_string(), "2024-02-29T01:00:00Z");
    }

    #[test]
    fn epoch_round_trip_is_stable() {
        let dt = DateTime::parse("1969-12-31T23:59:59Z").unwrap();
        assert_eq!(dt.to_epoch_seconds(), -1);
        let back = DateTime::from_epoch_seconds(-1, Offset::UTC);
        assert_eq!(back, dt);
    }

    #[test]
    fn parses_fractional_seconds() {
        let dt = DateTime::parse("2024-03-10T14:30:00.5Z").unwrap();
        assert_eq!(dt.nanosecond, 500_000_000);
    }

    #[test]
    fn fractional_seconds_pad_and_truncate_to_nanoseconds() {
        let dt = DateTime::parse("2024-03-10T14:30:00.123456789123Z").unwrap();
        assert_eq!(dt.nanosecond, 123_456_789);
    }

    #[test]
    fn fractional_seconds_round_trip_through_display() {
        assert_eq!(
            DateTime::parse("2024-03-10T14:30:00.250Z").unwrap().to_string(),
            "2024-03-10T14:30:00.25Z"
        );
        assert_eq!(
            DateTime::parse("2024-03-10T14:30:00Z").unwrap().to_string(),
            "2024-03-10T14:30:00Z"
        );
    }

    #[test]
    fn rejects_dot_with_no_digits() {
        assert!(DateTime::parse("2024-03-10T14:30:00.Z").is_err());
    }

    #[test]
    fn fractional_seconds_survive_offset_conversion_and_addition() {
        let dt = DateTime::parse("2024-03-10T14:30:00.750-05:00").unwrap();
        let converted = dt.with_offset(Offset::parse("+09:00").unwrap());
        assert_eq!(converted.nanosecond, 750_000_000);
        let later = dt.add_seconds(60);
        assert_eq!(later.nanosecond, 750_000_000);
        assert_eq!(later.to_string(), "2024-03-10T14:31:00.75-05:00");
    }

    #[test]
    fn parse_duration_handles_mixed_units() {
        assert_eq!(parse_duration("1d2h3m4s").unwrap(), 86400 + 7200 + 180 + 4);
    }

    #[test]
    fn parse_duration_handles_sign() {
        assert_eq!(parse_duration("-90m").unwrap(), -5400);
        assert_eq!(parse_duration("+90m").unwrap(), 5400);
    }

    #[test]
    fn parse_duration_rejects_empty_and_malformed() {
        assert!(parse_duration("").is_err());
        assert!(parse_duration("-").is_err());
        assert!(parse_duration("3x").is_err());
        assert!(parse_duration("h3").is_err());
        assert!(parse_duration("3h4").is_err());
    }
}
