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
    /// accepted) followed by `Z` or a `+HH:MM` / `-HH:MM` offset. Every
    /// field is range-checked, including day-of-month against the actual
    /// length of that month in that year.
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
        let offset = Offset::parse(&s[19..])?;

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

        Ok(DateTime { year, month, day, hour, minute, second, offset })
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
    /// the given instant.
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
            offset,
        }
    }

    /// Re-expresses the same instant under a different offset.
    pub fn with_offset(&self, offset: Offset) -> DateTime {
        DateTime::from_epoch_seconds(self.to_epoch_seconds(), offset)
    }

    /// Adds a (possibly negative) number of seconds, keeping the offset.
    pub fn add_seconds(&self, delta: i64) -> DateTime {
        DateTime::from_epoch_seconds(self.to_epoch_seconds() + delta, self.offset)
    }
}

impl fmt::Display for DateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{}",
            self.year, self.month, self.day, self.hour, self.minute, self.second, self.offset
        )
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
