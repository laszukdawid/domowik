//! RFC 3339 UTC timestamps, to the precision §10.7 actually needs.
//!
//! Independently derived elementary arithmetic rather than a date library: the
//! only questions asked here are "is this string a well-formed UTC instant"
//! and "is instant A more than M hours after instant B", and a calendar
//! dependency would bring a timezone database to answer them. Leap seconds are
//! not represented; a `:60` second field parses as 60 and compares as 60,
//! which is the conservative direction for an expiry test.

/// Why a timestamp was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{value:?} is not an RFC 3339 UTC timestamp: {reason}")]
pub struct TimestampError {
    pub value: String,
    pub reason: &'static str,
}

fn err(value: &str, reason: &'static str) -> TimestampError {
    TimestampError {
        value: value.to_owned(),
        reason,
    }
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: u32) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Seconds since 1970-01-01T00:00:00Z, for an RFC 3339 timestamp in UTC.
///
/// UTC only. §10.2 and §10.7 both say "RFC 3339 UTC", so an offset other than
/// zero is a rejection rather than a conversion — accepting `+02:00` here
/// would mean two descriptors could disagree about what "the same instant"
/// serialises to, and set semantics over hints would stop working.
pub fn parse_rfc3339_utc(value: &str) -> Result<i64, TimestampError> {
    let bytes = value.as_bytes();
    if !value.is_ascii() || bytes.len() < 20 {
        return Err(err(value, "too short to be a full date-time"));
    }
    let num = |from: usize, to: usize, what: &'static str| -> Result<i64, TimestampError> {
        let slice = value.get(from..to).ok_or_else(|| err(value, what))?;
        if !slice.bytes().all(|b| b.is_ascii_digit()) {
            return Err(err(value, what));
        }
        slice.parse::<i64>().map_err(|_| err(value, what))
    };
    if bytes[4] != b'-' || bytes[7] != b'-' {
        return Err(err(value, "date separators must be '-'"));
    }
    // RFC 3339 permits a lowercase 't'; the specification's own examples use
    // the uppercase form, and both are accepted rather than making a reader
    // guess which one a writer chose.
    if !(bytes[10] == b'T' || bytes[10] == b't') {
        return Err(err(value, "date and time must be separated by 'T'"));
    }
    if bytes[13] != b':' || bytes[16] != b':' {
        return Err(err(value, "time separators must be ':'"));
    }
    let year = num(0, 4, "year")?;
    let month = num(5, 7, "month")? as u32;
    let day = num(8, 10, "day")?;
    let hour = num(11, 13, "hour")?;
    let minute = num(14, 16, "minute")?;
    let second = num(17, 19, "second")?;

    let mut tail = &value[19..];
    if let Some(rest) = tail.strip_prefix('.') {
        let frac_len = rest.bytes().take_while(|b| b.is_ascii_digit()).count();
        if frac_len == 0 {
            return Err(err(value, "a '.' must be followed by fractional digits"));
        }
        // Sub-second precision is parsed and DISCARDED. Every comparison this
        // module supports is at hour or day scale, and carrying milliseconds
        // would imply a resolution the §10.7 rules do not use.
        tail = &rest[frac_len..];
    }
    if !matches!(tail, "Z" | "z" | "+00:00" | "-00:00") {
        return Err(err(value, "offset must be UTC ('Z' or '+00:00')"));
    }

    if !(1..=12).contains(&month) {
        return Err(err(value, "month must be 01..12"));
    }
    if day < 1 || day > days_in_month(year, month) {
        return Err(err(value, "day is out of range for that month"));
    }
    if hour > 23 || minute > 59 || second > 60 {
        return Err(err(value, "time field out of range"));
    }

    let mut days: i64 = 0;
    if year >= 1970 {
        for y in 1970..year {
            days += if is_leap(y) { 366 } else { 365 };
        }
    } else {
        for y in year..1970 {
            days -= if is_leap(y) { 366 } else { 365 };
        }
    }
    for m in 1..month {
        days += days_in_month(year, m);
    }
    days += day - 1;

    Ok(days * 86_400 + hour * 3_600 + minute * 60 + second)
}

/// Whether `value` is a well-formed RFC 3339 UTC timestamp.
pub fn is_rfc3339_utc(value: &str) -> bool {
    parse_rfc3339_utc(value).is_ok()
}
