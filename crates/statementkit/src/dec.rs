//! Fixed-point decimal money: i128 scaled by 1e8 (IB prints <= 8 dp).
//!
//! Parsing accepts exactly the cell forms real statement exports use;
//! anything else is rejected, never truncated. JSON output is plain
//! numeric text, never formatted.

use std::fmt;

const UNIT: i128 = 100_000_000;
const FRAC_DIGITS: usize = 8;

/// Failure reasons for decimal cell parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecError {
    /// Cell was empty after trimming.
    Empty,
    /// Cell was not a supported numeric form.
    Invalid,
}

/// Decimal scaled by 1e8; exact for all statement values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Dec(i128);

impl Dec {
    pub const ZERO: Dec = Dec(0);
    pub const ONE: Dec = Dec(UNIT);

    pub fn scaled(v: i128) -> Dec {
        Dec(v)
    }

    pub fn to_scaled(self) -> i128 {
        self.0
    }

    /// Lossy conversion for ratio math (volatility, drawdown) only.
    pub fn to_f64(self) -> f64 {
        self.0 as f64 / UNIT as f64
    }

    /// Parse a statement numeric cell.
    ///
    /// Accepted: optional sign, digits with `,` thousands separators, at
    /// most 8 fraction digits, whole-form negatives in parentheses, and a
    /// 3-letter ISO-4217 affix with spaces (`USD 123.45`, `123.45 USD`).
    /// Rejected: exponent forms, `NaN`/`Infinity`, more than 8 fraction
    /// digits, anything else.
    pub fn parse(text: &str) -> Result<Dec, DecError> {
        let mut s = text.trim();
        if s.is_empty() {
            return Err(DecError::Empty);
        }
        let parens = s.starts_with('(') && s.ends_with(')');
        if parens {
            s = s[1..s.len() - 1].trim();
        }
        if s.is_empty() {
            return Err(DecError::Empty);
        }
        let stripped = strip_affix(s);
        let bare: String = stripped.chars().filter(|&c| c != ',').collect();
        if bare.is_empty() {
            return Err(DecError::Empty);
        }
        let value = parse_fixed(&bare)?;
        if parens {
            value.0.checked_neg().map(Dec).ok_or(DecError::Invalid)
        } else {
            Ok(value)
        }
    }

    /// Exact numeric text: no trailing zeros, no separators.
    pub fn to_text(self) -> String {
        if self.0 == 0 {
            return "0".to_string();
        }
        let neg = self.0 < 0;
        let mag = self.0.unsigned_abs();
        let int = mag / UNIT as u128;
        let frac = mag % UNIT as u128;
        let mut out = String::new();
        if neg {
            out.push('-');
        }
        out.push_str(&int.to_string());
        if frac != 0 {
            let digits = format!("{frac:0FRAC_DIGITS$}");
            out.push('.');
            out.push_str(digits.trim_end_matches('0'));
        }
        out
    }
}

impl std::ops::Add for Dec {
    type Output = Dec;
    fn add(self, other: Dec) -> Dec {
        Dec(self.0 + other.0)
    }
}

impl std::ops::Sub for Dec {
    type Output = Dec;
    fn sub(self, other: Dec) -> Dec {
        Dec(self.0 - other.0)
    }
}

impl std::ops::Neg for Dec {
    type Output = Dec;
    fn neg(self) -> Dec {
        Dec(-self.0)
    }
}

impl std::ops::Mul for Dec {
    type Output = Dec;
    /// Product rounded to the 1e8 scale (half away from zero). Used for
    /// FX conversion; inputs stay exact, only the product rounds.
    fn mul(self, other: Dec) -> Dec {
        let neg = (self.0 < 0) != (other.0 < 0);
        let p = self.0.unsigned_abs() * other.0.unsigned_abs();
        let q = ((p + (UNIT as u128) / 2) / (UNIT as u128)) as i128;
        Dec(if neg { -q } else { q })
    }
}

impl fmt::Display for Dec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_text())
    }
}

/// Drop one leading and one trailing ISO-4217 affix (`USD `, ` USD`).
fn strip_affix(s: &str) -> &str {
    let is_code = |b: &str| {
        b.len() == 3
            && b.bytes()
                .all(|c| c.is_ascii_alphabetic() && c.is_ascii_uppercase())
    };
    let mut s = s;
    if let Some(rest) = s.get(..3)
        && is_code(rest)
        && s.as_bytes().get(3) == Some(&b' ')
    {
        s = s[4..].trim_start();
    }
    let bytes = s.as_bytes();
    if bytes.len() >= 4 && bytes[bytes.len() - 4] == b' ' && is_code(&s[bytes.len() - 3..]) {
        s = s[..bytes.len() - 4].trim_end();
    }
    s
}

/// Sign, integer digits, at most `FRAC_DIGITS` fraction digits.
fn parse_fixed(s: &str) -> Result<Dec, DecError> {
    let (neg, digits) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let (int_part, frac_part) = match digits.split_once('.') {
        Some((a, b)) => (a, b),
        None => (digits, ""),
    };
    if int_part.is_empty() && frac_part.is_empty() {
        return Err(DecError::Invalid);
    }
    if !int_part.bytes().all(|c| c.is_ascii_digit())
        || !frac_part.bytes().all(|c| c.is_ascii_digit())
        || frac_part.len() > FRAC_DIGITS
    {
        return Err(DecError::Invalid);
    }
    let int: i128 = if int_part.is_empty() {
        0
    } else {
        int_part.parse().map_err(|_| DecError::Invalid)?
    };
    let mut frac: i128 = if frac_part.is_empty() {
        0
    } else {
        let padded = format!("{:0<FRAC_DIGITS$}", frac_part);
        padded.parse().map_err(|_| DecError::Invalid)?
    };
    frac += int.checked_mul(UNIT).ok_or(DecError::Invalid)?;
    if neg {
        frac = frac.checked_neg().ok_or(DecError::Invalid)?;
    }
    Ok(Dec(frac))
}
