//! statementkit core: parsing brokerage statement content into numbers.
//!
//! Pure functions only — no I/O, no FFI types. All parsing and derivation
//! logic lives in this crate; bindings are thin shells over it. Streaming
//! entry points take caller-fed byte chunks; the crate opens no files.
//!
//! Independent tool. Not affiliated with or endorsed by Interactive
//! Brokers LLC.

pub mod csv;
pub mod dates;
pub mod dec;
pub mod json;
pub mod session;
pub mod target;
mod targets;
pub mod tickers;

pub use dec::{Dec, DecError};
pub use json::Value;
pub use session::{FileSummary, SectionSummary, Session, SessionError};
pub use target::{RowRef, Target, target_names};
pub use tickers::{root_up, ticker_prefix};

/// Failure reasons for numeric parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Input was empty after trimming.
    Empty,
    /// Input was not a finite bare number.
    Invalid,
}

/// Parse a bare decimal float literal into f64.
///
/// Trims surrounding whitespace; rejects empty input, non-numeric text,
/// and non-finite values (`NaN` and infinities appear verbatim as cell
/// strings in real statement exports and must not enter arithmetic).
/// Separators, parentheses-negatives and percent forms are later
/// milestones; until then they parse as Invalid.
///
/// This is a skeleton demo returning binary float. Money values will use
/// an exact decimal type; do not route money through this function.
pub fn parse_float(input: &str) -> Result<f64, Error> {
    let text = input.trim();
    if text.is_empty() {
        return Err(Error::Empty);
    }
    let value = text.parse::<f64>().map_err(|_| Error::Invalid)?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(Error::Invalid)
    }
}

#[cfg(test)]
mod tests {
    use super::{Error, parse_float};

    #[test]
    fn parses_plain_literals() {
        for (text, want) in [
            ("0", 0.0),
            ("8.5", 8.5),
            ("-0.5", -0.5),
            ("+2.5", 2.5),
            ("1e3", 1000.0),
            ("-2.5E-3", -0.0025),
            ("7.", 7.0),
            (".5", 0.5),
            ("  42.0  ", 42.0),
        ] {
            assert_eq!(parse_float(text), Ok(want), "input: {text:?}");
        }
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(parse_float(""), Err(Error::Empty));
        assert_eq!(parse_float("   "), Err(Error::Empty));
    }

    #[test]
    fn rejects_invalid() {
        for text in [
            "abc", "1,234.56", "1.2.3", "0x10", "1 2", "(1.5)", "5%", "NaN", "nan", "inf", "-inf",
            "infinity",
        ] {
            assert_eq!(parse_float(text), Err(Error::Invalid), "input: {text:?}");
        }
    }
}
