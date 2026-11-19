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
    use super::{stated_period, stated_to_iso};

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
}
