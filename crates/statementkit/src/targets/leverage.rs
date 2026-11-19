//! `leverage`: gross and net exposure over time.
//!
//! Per snapshot file: `Open Positions` `Value` converted to USD at the
//! same file's `Base Currency Exchange Rate` (rows with no rate are
//! excluded and counted), gross = `Σ|Value|`, net = `Σ Value`. Cash
//! legs enter both numerators (the labeled choice): base-currency
//! `Total Cash` from `Cash Report` plus `Forex Balances` `Value in
//! USD` (already USD). Denominator is the same file's NAV `Total`.
//!
//! Verified identity per row: `Value == Close Price × Mult × Quantity`,
//! `sign(Value) == sign(Quantity)`; futures `Value` is full notional,
//! options `Value` is premium notional — a market-value statement, the
//! capped-risk ledger stays alongside, not inside.
//!
//! Snapshots see overnight positions only; a same-day round trip
//! appears in no snapshot. No margin data exists in statements.

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
    rows: Vec<(Dec, String)>, // value, currency
    cash: Option<Dec>,
    fx_usd: Dec,
    has_cash_block: bool,
    has_fx_block: bool,
    nav: Option<Dec>,
    rates: std::collections::BTreeMap<String, Dec>,
}

pub struct Leverage {
    files: Vec<FileState>,
    cur: FileState,
    seq: usize,
}

impl Leverage {
    pub fn new() -> Self {
        Leverage {
            files: Vec::new(),
            cur: FileState::default(),
            seq: 0,
        }
    }
}

fn dec(raw: Option<&str>) -> Option<Dec> {
    raw.and_then(|r| Dec::parse(r).ok())
}

fn ratio(part: Dec, total: Dec) -> Value {
    json::ratio(if total > Dec::ZERO {
        Some(part.to_f64() / total.to_f64())
    } else {
        None
    })
}

impl Target for Leverage {
    fn name(&self) -> &'static str {
        "leverage"
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
            "Open Positions" => {
                let Some(value) = dec(row.cell("Value")) else {
                    return;
                };
                let ccy = row.cell("Currency").unwrap_or(&self.cur.base).to_string();
                self.cur.rows.push((value, ccy));
            }
            "Base Currency Exchange Rate" => {
                if let (Some(ccy), Some(rate)) =
                    (row.cell("Currency"), dec(row.cell("Exchange Rate")))
                {
                    self.cur.rates.insert(ccy.to_string(), rate);
                }
            }
            "Cash Report" => {
                self.cur.has_cash_block = true;
                let summary = row.cell("Currency Summary").unwrap_or("");
                if summary.contains("Total Cash")
                    && row.cell("Currency").unwrap_or(&self.cur.base) == self.cur.base
                {
                    self.cur.cash = dec(row.cell("Total"));
                }
            }
            "Forex Balances" => {
                self.cur.has_fx_block = true;
                if let Some(v) = dec(row.cell("Value in USD")) {
                    self.cur.fx_usd = self.cur.fx_usd + v;
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
        if !done.rows.is_empty() || done.nav.is_some() {
            self.files.push(done);
        }
    }

    fn report(&self) -> Value {
        let mut files: Vec<&FileState> = self.files.iter().collect();
        files.sort_by_key(|f| f.seq);
        let mut out: Vec<Value> = Vec::new();
        let mut no_cash = 0usize;
        let mut unrateable = 0usize;
        for f in files {
            if !f.has_cash_block && !f.has_fx_block {
                no_cash += 1;
            }
            // convert at report time: rate tables may trail the position rows
            let mut sum = Dec::ZERO;
            let mut abs = Dec::ZERO;
            let mut unrate = 0usize;
            for (value, ccy) in &f.rows {
                let usd = if *ccy == f.base {
                    Some(*value)
                } else {
                    f.rates.get(ccy).map(|r| *value * *r)
                };
                match usd {
                    Some(u) => {
                        sum = sum + u;
                        abs = abs + if u < Dec::ZERO { -u } else { u };
                    }
                    None => unrate += 1,
                }
            }
            unrateable += unrate;
            let cash = f.cash.unwrap_or(Dec::ZERO) + f.fx_usd;
            let gross = abs + cash;
            let net = sum + cash;
            let check = match f.nav {
                Some(n) => net - n,
                None => Dec::ZERO,
            };
            out.push(Value::Obj(vec![
                (
                    "period_end".to_string(),
                    match &f.end {
                        Some(d) => Value::Str(d.clone()),
                        None => Value::Null,
                    },
                ),
                (
                    "nav".to_string(),
                    f.nav.map(|v| Value::num(&v)).unwrap_or(Value::Null),
                ),
                (
                    "position_rows".to_string(),
                    Value::Num(f.rows.len().to_string()),
                ),
                ("position_sum".to_string(), Value::num(&sum)),
                ("position_abs".to_string(), Value::num(&abs)),
                ("cash_total".to_string(), Value::num(&cash)),
                ("cash_in_numerator".to_string(), Value::Bool(true)),
                (
                    "gross_over_nav".to_string(),
                    f.nav.map(|n| ratio(gross, n)).unwrap_or(Value::Null),
                ),
                (
                    "net_over_nav".to_string(),
                    f.nav.map(|n| ratio(net, n)).unwrap_or(Value::Null),
                ),
                (
                    "check_pos_plus_cash_minus_nav".to_string(),
                    f.nav.map(|n| ratio(check, n)).unwrap_or(Value::Null),
                ),
            ]));
        }
        let mut notes = vec![
            Value::Str("numerator choice labeled: gross = |positions| + cash, net = positions + cash".to_string()),
            Value::Str("futures Value is full notional, option Value is premium notional; no capped-risk here".to_string()),
            Value::Str("overnight snapshots only; same-day round trips appear nowhere".to_string()),
            Value::Str("no margin requirement exists in statements; no buying power is produced".to_string()),
        ];
        if no_cash > 0 {
            notes.push(Value::Str(format!(
                "files with positions or NAV but no cash blocks (cash leg = 0): {no_cash}"
            )));
        }
        if unrateable > 0 {
            notes.push(Value::Str(format!(
                "position rows excluded for missing rate: {unrateable}"
            )));
        }
        Value::Obj(vec![
            ("snapshots".to_string(), Value::Arr(out)),
            ("notes".to_string(), Value::Arr(notes)),
        ])
    }
}
