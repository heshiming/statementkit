//! Statement date forms: IB prints `September 8, 2026` and ranges of it.

const MONTHS: [(&str, u32); 12] = [
    ("january", 1),
    ("february", 2),
    ("march", 3),
    ("april", 4),
    ("may", 5),
    ("june", 6),
    ("july", 7),
    ("august", 8),
    ("september", 9),
    ("october", 10),
    ("november", 11),
    ("december", 12),
];

/// `September 8, 2026` -> `Some("2026-09-08")`.
pub fn stated_to_iso(text: &str) -> Option<String> {
    let text = text.trim();
    let (month_word, rest) = text.split_once(' ')?;
    let (day, year) = rest.rsplit_once(' ')?;
    let day: u32 = day.strip_suffix(',')?.parse().ok()?;
    let year: u32 = year.parse().ok()?;
    let month = MONTHS
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(month_word))?
        .1;
    if !(1..=31).contains(&day) || !(1900..=2999).contains(&year) {
        return None;
    }
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

/// Days since 1970-01-01 for an ISO `YYYY-MM-DD` date (civil algorithm).
pub fn iso_to_days(iso: &str) -> Option<i64> {
    let y: i64 = iso.get(0..4)?.parse().ok()?;
    let m: i64 = iso.get(5..7)?.parse().ok()?;
    let d: i64 = iso.get(8..10)?.parse().ok()?;
    if iso.len() != 10 || iso.as_bytes()[4] != b'-' || iso.as_bytes()[7] != b'-' {
        return None;
    }
    let y = y - if m <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146097 + doe - 719468)
}

/// ISO `YYYY-MM-DD` for days since 1970-01-01 (inverse of
/// [`iso_to_days`]).
pub fn days_to_iso(days: i64) -> String {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// A `Period` value: single date or `from - to` range.
/// Returns (from, to, is_range); `None` if neither side parses.
pub fn stated_period(text: &str) -> Option<(Option<String>, Option<String>, bool)> {
    if let Some((a, b)) = text.split_once(" - ") {
        return Some((stated_to_iso(a), stated_to_iso(b), true));
    }
    let d = stated_to_iso(text);
    d.as_ref()?;
    Some((d.clone(), d, false))
}

#[cfg(test)]
mod tests {
    use super::{days_to_iso, iso_to_days, stated_period, stated_to_iso};

    #[test]
    fn parses_stated_dates() {
        assert_eq!(
            stated_to_iso("September 8, 2026").as_deref(),
            Some("2026-09-08")
        );
        assert_eq!(
            stated_to_iso(" may 5, 2026 ").as_deref(),
            Some("2026-05-05")
        );
        assert_eq!(stated_to_iso("May 32, 2026"), None);
        assert_eq!(stated_to_iso("Sept 5, 2026"), None);
        assert_eq!(stated_to_iso("May 5 2026"), None);
    }

    #[test]
    fn parses_periods() {
        assert_eq!(
            stated_period("January 1, 2024 - December 31, 2024"),
            Some((Some("2024-01-01".into()), Some("2024-12-31".into()), true))
        );
        assert_eq!(
            stated_period("May 5, 2026"),
            Some((Some("2026-05-05".into()), Some("2026-05-05".into()), false))
        );
        assert_eq!(stated_period("whenever"), None);
    }

    #[test]
    fn iso_dates_to_days() {
        assert_eq!(iso_to_days("1970-01-01"), Some(0));
        assert_eq!(iso_to_days("2000-02-29"), Some(11016));
        assert_eq!(iso_to_days("2026-02-28"), Some(20512));
        assert_eq!(iso_to_days("2026-01-05"), Some(20458));
        assert_eq!(iso_to_days("2026-01-08"), Some(20461));
        assert_eq!(iso_to_days("2026-1-08"), None);
        assert_eq!(iso_to_days("2026-01-08T00:00"), None);
        assert_eq!(days_to_iso(0), "1970-01-01");
        assert_eq!(days_to_iso(20458), "2026-01-05");
        assert_eq!(days_to_iso(-1), "1969-12-31");
        assert_eq!(days_to_iso(20457), "2026-01-04");
    }
}
