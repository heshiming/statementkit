//! `cash-coverage`: liquidity from the cash book, not a ledger residual.
//!
//! Coverage per file: base-currency `Total Cash` from `Cash Report`
//! over the file's NAV `Total`. The `Cash Report` carries segment
//! columns (`Securities`, `Futures`, `Paxos`) whose field-set layout
//! differs per template; segment extraction is deferred, the base
//! currency total is the coverage number. Trailing unnamed header
//! columns are empty header cells, not data shift.
//!
//! `Forex Balances` rows are already USD-twins (`Value in USD`) and
//! sum as context next to the coverage.
//!
//! The statements disclose no margin requirement, so this target
//! stops at cash coverage: no buying power is derivable.

use crate::dates;
use crate::dec::Dec;
use crate::json;
use crate::json::Value;
use crate::target::{RowRef, Target};

#[derive(Default)]
struct FileState {
    base: String,
    end: Option<String>,
    seq: usize,
    cash: Option<Dec>,
    fx_usd: Dec,
    has_fx_rows: bool,
    nav: Option<Dec>,
}

pub struct CashCoverage {
    files: Vec<FileState>,
    cur: FileState,
    seq: usize,
}

impl CashCoverage {
    pub fn new() -> Self {
        CashCoverage {
            files: Vec::new(),
            cur: FileState::default(),
            seq: 0,
        }
    }
}

fn dec(raw: Option<&str>) -> Option<Dec> {
    raw.and_then(|r| Dec::parse(r).ok())
}

impl Target for CashCoverage {
    fn name(&self) -> &'static str {
        "cash-coverage"
    }

    fn on_meta(&mut self, key: &str, value: &str) {
        match key {
            "Currency" => self.cur.base = value.to_string(),
            "Period" => {
                if let Some((_, end, _)) = dates::stated_period(value) {
                    self.cur.end = end;
                }
            }
            _ => {}
        }
    }

    fn on_row(&mut self, row: &RowRef) {
        match row.title {
            "Cash Report" => {
                let Some(summary) = row.cell("Currency Summary") else {
                    return;
                };
                if !summary.contains("Total Cash") {
                    return; // segment/other summary rows carry other books
                }
                let Some(total) = dec(row.cell("Total")) else {
                    return;
                };
                let ccy = row.cell("Currency").unwrap_or(&self.cur.base).to_string();
                if ccy == self.cur.base {
                    self.cur.cash = Some(total);
                }
            }
            "Forex Balances" => {
                if let Some(v) = dec(row.cell("Value in USD")) {
                    self.cur.fx_usd = self.cur.fx_usd + v;
                    self.cur.has_fx_rows = true;
                }
            }
            "Net Asset Value" if row.cell("Asset Class").map(|c| c.trim()) == Some("Total") => {
                self.cur.nav = dec(row.cell("Current Total"));
            }
            _ => {}
        }
    }

    fn on_file_end(&mut self) {
        self.cur.seq = self.seq;
        self.seq += 1;
        let done = std::mem::take(&mut self.cur);
        if done.nav.is_some() || done.cash.is_some() || done.has_fx_rows {
            self.files.push(done);
        }
    }

    fn report(&self) -> Value {
        let mut files: Vec<&FileState> = self.files.iter().collect();
        files.sort_by_key(|f| f.seq);
        let mut out: Vec<Value> = Vec::new();
        let mut no_cash_rows = 0usize;
        for f in files {
            if f.cash.is_none() && (f.nav.is_some() || f.has_fx_rows) {
                no_cash_rows += 1;
            }
            let coverage = match (f.cash, f.nav) {
                (Some(c), Some(n)) if n > Dec::ZERO => json::ratio(Some(c.to_f64() / n.to_f64())),
                _ => Value::Null,
            };
            let mut snap = vec![
                (
                    "period_end".to_string(),
                    match &f.end {
                        Some(d) => Value::Str(d.clone()),
                        None => Value::Null,
                    },
                ),
                (
                    "cash_base".to_string(),
                    match f.cash {
                        Some(c) => Value::num(&c),
                        None => Value::Null,
                    },
                ),
                (
                    "nav_total".to_string(),
                    match f.nav {
                        Some(n) => Value::num(&n),
                        None => Value::Null,
                    },
                ),
                ("coverage".to_string(), coverage),
            ];
            if f.has_fx_rows {
                snap.push(("forex_balances_usd".to_string(), Value::num(&f.fx_usd)));
            }
            out.push(Value::Obj(snap));
        }
        let mut notes = vec![
            Value::Str("cash is IB's cash book (base-currency Total Cash row), not a deposits-minus-buys residual".to_string()),
            Value::Str("segment columns (Securities/Futures/Paxos) have per-template layouts and are deferred".to_string()),
            Value::Str("no margin requirement is disclosed in statements: no buying-power number is produced".to_string()),
        ];
        if no_cash_rows > 0 {
            notes.push(Value::Str(format!(
                "files with NAV or FX balances but no base-currency Total Cash row: {no_cash_rows}"
            )));
        }
        Value::Obj(vec![
            ("periods".to_string(), Value::Arr(out)),
            ("notes".to_string(), Value::Arr(notes)),
        ])
    }
}
