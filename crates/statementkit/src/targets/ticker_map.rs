//! `ticker-map`: which sections a ticker selection can see, and
//! which money has no ticker to be selected by.
//!
//! Five sections carry a `Symbol` column (counts include rows
//! whose symbol cell is empty). Two money sections carry the
//! ticker only inside a free-text `Description` — the
//! `TICKER(USCUSIP…)` prefix of `Dividends`, and the same shape
//! in `Option Cash Settlements` and `Corporate Actions` — at
//! less than full coverage. Financing, withholding, deposits &
//! withdrawals, transfers, and every NAV/TWR number carry no
//! ticker at all: no selection can price them.
//!
//! `Financial Instrument Information` is the file's own symbol
//! vocabulary: a typed selection that matches zero vocabulary
//! entries is indistinguishable from a typo, so the size of the
//! vocabulary ships with the map.
//!
//! Template asymmetry is by construction: a dailies file carries
//! no `Dividends` or `Borrow Fee Details` section, so a
//! per-ticker calendar at daily grain is trades-only there. The
//! untitled `Dividends` postings (empty `Description`) are real
//! money with no recoverable attribution — counted, never guessed.

use std::collections::BTreeMap;

use crate::json::Value;
use crate::target::{RowRef, Target};

#[derive(Default)]
struct Section {
    rows: usize,
    with_ticker: usize,
    has_symbol_col: bool,
    carrier: &'static str,
}

fn is_ticker_prefix(desc: &str) -> bool {
    // `SPY(US78462X1060) CASH DIVIDEND` style: a bare uppercase
    // token immediately before an opening parenthesis.
    let Some(paren) = desc.find('(') else {
        return false;
    };
    let token = desc[..paren].trim();
    !token.is_empty()
        && token.len() <= 8
        && token
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'.')
}

fn carrier_of(title: &str, has_symbol_col: bool) -> &'static str {
    if has_symbol_col {
        "symbol-column"
    } else if matches!(
        title,
        "Dividends" | "Option Cash Settlements" | "Corporate Actions"
    ) {
        "description-prefix"
    } else {
        "none"
    }
}

pub struct TickerMap {
    sections: BTreeMap<String, Section>,
    fii_symbols: Vec<String>,
    dividends_untitled: usize,
}

impl TickerMap {
    pub fn new() -> Self {
        TickerMap {
            sections: BTreeMap::new(),
            fii_symbols: Vec::new(),
            dividends_untitled: 0,
        }
    }
}

impl Target for TickerMap {
    fn name(&self) -> &'static str {
        "ticker-map"
    }

    fn on_section(&mut self, title: &str, fields: &[String]) {
        let s = self.sections.entry(title.to_string()).or_default();
        s.has_symbol_col = fields.iter().any(|f| f == "Symbol");
    }

    fn on_row(&mut self, row: &RowRef) {
        let s = self.sections.entry(row.title.to_string()).or_default();
        s.rows += 1;
        s.carrier = carrier_of(row.title, s.has_symbol_col);
        let has = match s.carrier {
            "symbol-column" => row
                .cell("Symbol")
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false),
            "description-prefix" => is_ticker_prefix(row.cell("Description").unwrap_or("")),
            _ => false,
        };
        if has {
            s.with_ticker += 1;
        }
        if row.title == "Dividends" {
            let d = row.cell("Description").unwrap_or("").trim();
            if d.is_empty() {
                self.dividends_untitled += 1;
            }
        }
        if row.title == "Financial Instrument Information" {
            let sym = row.cell("Symbol").unwrap_or("").trim().to_string();
            if !sym.is_empty() && !self.fii_symbols.contains(&sym) {
                self.fii_symbols.push(sym);
            }
        }
    }

    fn report(&self) -> Value {
        let seen: Vec<(&String, &Section)> =
            self.sections.iter().filter(|(_, s)| s.rows > 0).collect();
        let sections: Vec<Value> = seen
            .iter()
            .map(|(_, s)| {
                Value::Obj(vec![
                    ("carrier".to_string(), Value::Str(s.carrier.to_string())),
                    ("rows".to_string(), Value::Num(s.rows.to_string())),
                    (
                        "with_ticker".to_string(),
                        Value::Num(s.with_ticker.to_string()),
                    ),
                ])
            })
            .collect();
        let mut notes = vec![
            Value::Str("Interest, Withholding Tax, Deposits & Withdrawals, Transfers and every NAV/TWR number carry no ticker: no selection can price financing or a return denominator".to_string()),
            Value::Str("a dailies template carries no Dividends or Borrow Fee Details section: a daily-grain ticker ledger is trades-only by construction".to_string()),
            Value::Str("attribution rule: Symbol column where present; description TICKER( prefix where absent; empty-symbol rows fall back to the same rule on their description — an empty Description stays unattributed".to_string()),
        ];
        if self.dividends_untitled > 0 {
            notes.push(Value::Str(format!(
                "Dividends rows with empty Description (unattributable, counted): {}",
                self.dividends_untitled
            )));
        }
        Value::Obj(vec![
            (
                "sections".to_string(),
                Value::Obj(
                    seen.into_iter()
                        .map(|(t, _)| t.clone())
                        .zip(sections)
                        .collect(),
                ),
            ),
            (
                "vocabulary_symbols".to_string(),
                Value::Num(self.fii_symbols.len().to_string()),
            ),
            ("notes".to_string(), Value::Arr(notes)),
        ])
    }
}
