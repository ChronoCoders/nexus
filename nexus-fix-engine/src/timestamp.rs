/// Length of a millisecond-precision FIX UTCTimestamp: `YYYYMMDD-HH:MM:SS.sss`.
pub(crate) const UTC_TIMESTAMP_LEN: usize = 21;

/// Formats UTC nanoseconds since the Unix epoch as a FIX UTCTimestamp.
pub(crate) fn format_utc_timestamp(unix_nanos: i128, out: &mut [u8; UTC_TIMESTAMP_LEN]) {
    let secs = unix_nanos.div_euclid(1_000_000_000) as i64;
    let millis = (unix_nanos.rem_euclid(1_000_000_000) / 1_000_000) as u32;
    let days = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400) as u32;
    let (y, m, d) = civil_from_days(days);

    write_digits(&mut out[0..4], y.max(0) as u32);
    write_digits(&mut out[4..6], m);
    write_digits(&mut out[6..8], d);
    out[8] = b'-';
    write_digits(&mut out[9..11], sod / 3600);
    out[11] = b':';
    write_digits(&mut out[12..14], sod / 60 % 60);
    out[14] = b':';
    write_digits(&mut out[15..17], sod % 60);
    out[17] = b'.';
    write_digits(&mut out[18..21], millis);
}

/// Converts days since the Unix epoch to a civil date.
///
/// Howard Hinnant's `civil_from_days` algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (y + i64::from(m <= 2), m, d)
}

fn write_digits(out: &mut [u8], mut v: u32) {
    for slot in out.iter_mut().rev() {
        *slot = b'0' + (v % 10) as u8;
        v /= 10;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(nanos: i128) -> String {
        let mut buf = [0u8; UTC_TIMESTAMP_LEN];
        format_utc_timestamp(nanos, &mut buf);
        String::from_utf8(buf.to_vec()).unwrap()
    }

    #[test]
    fn epoch() {
        assert_eq!(fmt(0), "19700101-00:00:00.000");
    }

    #[test]
    fn known_instant() {
        // 2026-06-03 16:55:33.123 UTC
        let secs = 1_780_505_733i128;
        assert_eq!(fmt(secs * 1_000_000_000 + 123_000_000), "20260603-16:55:33.123");
    }

    #[test]
    fn leap_day() {
        // 2024-02-29 12:00:00 UTC
        assert_eq!(fmt(1_709_208_000i128 * 1_000_000_000), "20240229-12:00:00.000");
    }

    #[test]
    fn year_boundary() {
        // 2025-12-31 23:59:59.999 UTC
        assert_eq!(
            fmt(1_767_225_599i128 * 1_000_000_000 + 999_000_000),
            "20251231-23:59:59.999"
        );
    }
}
