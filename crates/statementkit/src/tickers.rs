//! Ticker selection semantics.
//!
//! A typed root selects rows instrument-wise, per product shape:
//!
//! - stocks: the symbol is its own root (exact match);
//! - options: legs roll up — the first space-separated token is
//!   the root, so one typed root takes every strike and expiry;
//! - futures: the root strips the month code — `MNQH6`, `NGZ25`
//!   (one- and two-digit years) and the explicit-date shape
//!   (`FDAX 20260320 M`, which is the option-style shape) all
//!   reduce to `MNQ`, `NG`, `FDAX`;
//! - forex: pairs (`EUR.USD`) select exactly; the base alone
//!   selects nothing.
//!
//! Roots are a namespace, not a type: a root can be a stock and
//! a futures root at once (same letters, different products), so
//! a root selection is a set of roots and keeps every product it
//! hits. Option roots collide with their underlyings by design:
//! typing the underlying takes the option legs too.
//!
//! A symbol ending in month-letter + digits is treated as a dated
//! contract; a stock ticker with that literal shape would be
//! stripped by rule. No such ticker appears in the layouts these
//! rules were built against.

/// Month codes used by futures expiries.
fn is_month(b: u8) -> bool {
    matches!(
        b,
        b'F' | b'G' | b'H' | b'J' | b'K' | b'M' | b'N' | b'Q' | b'U' | b'V' | b'Z'
    )
}

/// The selection root of a symbol, uppercased.
pub fn root_up(sym: &str) -> String {
    let s = sym.trim();
    if let Some(i) = s.find(' ') {
        // option leg or explicit-date future: first token is the root
        return s[..i].trim().to_uppercase();
    }
    if s.contains('.') {
        // forex pair: exact
        return s.to_uppercase();
    }
    let b = s.as_bytes();
    let n = b.len();
    let mut i = n;
    while i > 0 && b[i - 1].is_ascii_digit() {
        i -= 1;
    }
    let digits = n - i;
    if (1..=2).contains(&digits) && i >= 2 && is_month(b[i - 1]) {
        return s[..i - 1].to_uppercase();
    }
    s.to_uppercase()
}

/// The ticker carried by a `TICKER(USCUSIP…) Description prefix.
pub fn ticker_prefix(desc: &str) -> Option<String> {
    let paren = desc.find('(')?;
    let token = desc[..paren].trim();
    if token.is_empty()
        || token.len() > 8
        || !token
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'.')
    {
        return None;
    }
    Some(token.to_uppercase())
}

#[cfg(test)]
mod tests {
    use super::root_up;

    #[test]
    fn stocks_exact_options_rollup() {
        assert_eq!(root_up("SPY"), "SPY");
        assert_eq!(root_up("SPXW"), "SPXW");
        assert_eq!(root_up("SPY 18DEC26 26 C"), "SPY");
        assert_eq!(root_up("VIX   20Mar26 30 P"), "VIX");
    }

    #[test]
    fn futures_month_codes_strip() {
        assert_eq!(root_up("MNQH6"), "MNQ");
        assert_eq!(root_up("NGZ25"), "NG");
        assert_eq!(root_up("CLU6"), "CL");
        assert_eq!(root_up("FDAX 20260320 M"), "FDAX");
    }

    #[test]
    fn roots_namespace_not_type() {
        assert_eq!(root_up("ZS"), "ZS"); // stock and futures root, same letters
        assert_eq!(root_up("EUR.USD"), "EUR.USD"); // pair exact
        assert_eq!(root_up("eur.usd"), "EUR.USD");
    }

    #[test]
    fn month_strip_is_greedy_on_letter_digit_shapes() {
        assert_eq!(root_up("F6"), "F6"); // one letter + digits: not stripped
        assert_eq!(root_up("GE9"), "GE9"); // trailing letter not a month code
        // disclosed rule shape: M+13 reads as a month code, so a stock
        // literally spelled IBM13 would strip to IB. No such ticker in
        // the layouts these rules were built against.
        assert_eq!(root_up("IBM13"), "IB");
    }
}
