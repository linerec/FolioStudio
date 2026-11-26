//! Dates for the report without a time-zone database: UTC from the system clock, shown in
//! the offset the caller gives (the window sends its own).

/// (year, month, day) of a day count since 1970-01-01 (H. Hinnant's civil-from-days).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

pub struct Stamp {
    /// ISO 8601 with offset, e.g. "2026-10-18T23:41:05+09:00".
    pub iso: String,
    /// For people, e.g. "2026-10-18 23:41 (UTC+09:00)".
    pub text: String,
}

pub fn stamp(unix_secs: i64, offset_minutes: Option<i32>) -> Stamp {
    let off = offset_minutes.unwrap_or(0).clamp(-14 * 60, 14 * 60);
    let t = unix_secs + i64::from(off) * 60;
    let (y, mo, d) = civil_from_days(t.div_euclid(86_400));
    let s = t.rem_euclid(86_400);
    let (h, mi, se) = (s / 3600, s % 3600 / 60, s % 60);
    let sign = if off < 0 { '-' } else { '+' };
    let (oh, om) = (off.abs() / 60, off.abs() % 60);
    Stamp {
        iso: format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{se:02}{sign}{oh:02}:{om:02}"),
        text: format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02} (UTC{sign}{oh:02}:{om:02})"),
    }
}

pub fn now(offset_minutes: Option<i32>) -> Stamp {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    stamp(secs, offset_minutes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(stamp(0, None).iso, "1970-01-01T00:00:00+00:00");
        // 2026-10-18 14:41:05 UTC = 23:41:05 in Korea
        let s = stamp(1_792_334_465, Some(540));
        assert_eq!(s.iso, "2026-10-18T23:41:05+09:00");
        assert_eq!(s.text, "2026-10-18 23:41 (UTC+09:00)");
        assert_eq!(stamp(951_782_400, None).iso, "2000-02-29T00:00:00+00:00");
        assert_eq!(stamp(-1, Some(-330)).iso, "1969-12-31T18:29:59-05:30");
    }
}
