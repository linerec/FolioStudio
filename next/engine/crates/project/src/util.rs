//! Small shared helpers: random ids, UTC timestamps, hex.

/// A random (version 4) UUID, lower-case with hyphens.
pub fn uuid_v4() -> String {
    let mut b = [0u8; 16];
    if getrandom::fill(&mut b).is_err() {
        // No OS randomness: time and process id still make a collision unlikely.
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        b[..16].copy_from_slice(&(t ^ ((std::process::id() as u128) << 64)).to_le_bytes());
    }
    b[6] = (b[6] & 0x0F) | 0x40;
    b[8] = (b[8] & 0x3F) | 0x80;
    let h = hex(&b);
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

/// A short random id (16 hex digits).
pub fn short_id() -> String {
    let mut b = [0u8; 8];
    let _ = getrandom::fill(&mut b);
    hex(&b)
}

pub fn hex(bytes: &[u8]) -> String {
    const D: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(D[(b >> 4) as usize] as char);
        s.push(D[(b & 15) as usize] as char);
    }
    s
}

/// Seconds since the Unix epoch, now.
pub fn now_secs() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// RFC 3339 UTC timestamp, e.g. "2026-10-19T03:12:45Z".
pub fn rfc3339(unix_secs: i64) -> String {
    let iso = folio_package::time::stamp(unix_secs, Some(0)).iso;
    format!("{}Z", iso.trim_end_matches("+00:00"))
}

pub fn now_rfc3339() -> String {
    rfc3339(now_secs())
}

/// Seconds since the epoch of an RFC 3339 UTC timestamp written by [`rfc3339`].
pub fn parse_rfc3339(s: &str) -> Option<i64> {
    let s = s.strip_suffix('Z')?;
    let (date, time) = s.split_once('T')?;
    let d: Vec<i64> = date.split('-').map(|x| x.parse().ok()).collect::<Option<_>>()?;
    let t: Vec<i64> = time.split(':').map(|x| x.parse().ok()).collect::<Option<_>>()?;
    if d.len() != 3 || t.len() != 3 {
        return None;
    }
    // days from civil (H. Hinnant)
    let (y, m, day) = (d[0] - i64::from(d[1] <= 2), d[1], d[2]);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + t[0] * 3600 + t[1] * 60 + t[2])
}

/// Modification time of file metadata in nanoseconds since the epoch (cache validation only;
/// never used to decide whether two files are the same).
pub fn mtime_ns(m: &std::fs::Metadata) -> Option<u128> {
    m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_nanos())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_times() {
        let u = uuid_v4();
        assert_eq!(u.len(), 36);
        assert_eq!(&u[14..15], "4");
        assert!(matches!(&u[19..20], "8" | "9" | "a" | "b"));
        assert_ne!(uuid_v4(), u);
        assert_eq!(short_id().len(), 16);
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(1_792_334_465), "2026-10-18T14:41:05Z");
        for t in [0, 951_782_400, 1_792_334_465, 4_102_444_799] {
            assert_eq!(parse_rfc3339(&rfc3339(t)), Some(t));
        }
    }
}
