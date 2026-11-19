//! `basis-recon`: cost-basis method choice, honestly labeled.
//!
//! IB ships WAC as ground truth: `Trades` `Basis` and `Open
//! Positions` `Cost Price`/`Cost Basis`. Alternate methods are
//! reconstructions over `Trades` fills only, so this target compares
//! the in-window quantity-weighted average fill against IB's own cost
//! price per symbol and reports the relative difference. Deltas are
//! "vs IB average basis", never ground truth: opens that predate the
//! window are left-censored (the same censoring as the trades
//! matching), transfers enter at `Xfer Price` outside `Trades`, and
//! corporate actions adjust basis through proceeds rows — all
//! invisible to a fills-only reconstruction.
//!
//! Buys (positive quantity, positive `Trade Price`, non-transfer
//! codes) weight the average; sells do not. Fills dedup across
//! overlapping files by (symbol, date/time, quantity, realized). The
//! IB cost price is the symbol's latest snapshot (`Open Positions`
//! `Cost Price` by period end).

use std::collections::{BTreeMap, BTreeSet};

use crate::dates;
use crate::dec::Dec;
use crate::json;
use crate::json::Value;
use crate::target::{RowRef, Target};

#[derive(Default)]
struct BuyAcc {
    cost: Dec, // Σ Trade Price × |Quantity|
    qty: Dec,  // Σ |Quantity|
}

pub struct BasisRecon {
    seen: BTreeSet<(String, String, String, String)>,
    ib: BTreeMap<String, (Dec, String, usize)>, // symbol -> (cost price, period end, seq)
    buys: BTreeMap<String, BuyAcc>,
    base_end: Option<String>,
    seq: usize,
    dupes: usize,
    no_price: usize,
}

impl BasisRecon {
    pub fn new() -> Self {
        BasisRecon {
            seen: BTreeSet::new(),
            ib: BTreeMap::new(),
            buys: BTreeMap::new(),
            base_end: None,
            seq: 0,
            dupes: 0,
            no_price: 0,
        }
    }
}

fn dec(raw: Option<&str>) -> Option<Dec> {
    raw.and_then(|r| Dec::parse(r).ok())
}

impl Target for BasisRecon {
    fn name(&self) -> &'static str {
        "basis-recon"
    }

    fn on_meta(&mut self, key: &str, value: &str) {
        if key == "Period" {
            self.base_end = dates::stated_period(value).and_then(|(_, e, _)| e);
        }
    }

    fn on_file_end(&mut self) {
        self.seq += 1;
    }

    fn on_row(&mut self, row: &RowRef) {
        match row.title {
            "Trades" => {
                let (Some(symbol), Some(qty)) = (row.cell("Symbol"), dec(row.cell("Quantity")))
                else {
                    return;
                };
                let key = (
                    symbol.to_string(),
                    row.cell("Date/Time").unwrap_or("").to_string(),
                    qty.to_text(),
                    row.cell("Realized P/L").unwrap_or("").to_string(),
                );
                if !self.seen.insert(key) {
                    self.dupes += 1;
                    return;
                }
                let code = row.cell("Code").unwrap_or("");
                if code.contains("IA") || code.contains("IM") || qty <= Dec::ZERO {
                    return;
                }
                match dec(row.cell("Trade Price")) {
                    Some(p) if p > Dec::ZERO => {
                        let b = self.buys.entry(symbol.to_string()).or_default();
                        b.cost = b.cost + p * qty;
                        b.qty = b.qty + qty;
                    }
                    Some(_) => self.no_price += 1,
                    None => self.no_price += 1,
                }
            }
            "Open Positions" => {
                if row.cell("DataDiscriminator").unwrap_or("Summary") != "Summary" {
                    return;
                }
                let (Some(symbol), Some(cost_price)) =
                    (row.cell("Symbol"), dec(row.cell("Cost Price")))
                else {
                    return;
                };
                let end = self.base_end.clone().unwrap_or_default();
                let e = self.ib.entry(symbol.to_string()).or_insert((
                    cost_price,
                    end.clone(),
                    self.seq,
                ));
                if (end.as_str(), self.seq) >= (e.1.as_str(), e.2) {
                    *e = (cost_price, end, self.seq);
                }
            }
            _ => {}
        }
    }

    fn report(&self) -> Value {
        let mut rows: Vec<(String, Dec, f64, f64)> = Vec::new();
        let mut no_fill = 0usize;
        for (symbol, (ib_price, _, _)) in &self.ib {
            match self.buys.get(symbol) {
                Some(b) if b.qty > Dec::ZERO => {
                    let avg = b.cost.to_f64() / b.qty.to_f64();
                    let rel = (ib_price.to_f64() - avg).abs() / ib_price.to_f64();
                    rows.push((symbol.clone(), *ib_price, avg, rel));
                }
                _ => no_fill += 1,
            }
        }
        rows.sort_by(|a, b| {
            b.3.partial_cmp(&a.3)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
        });
        let row_list: Vec<Value> = rows
            .iter()
            .map(|(symbol, ib, avg, rel)| {
                Value::Obj(vec![
                    ("symbol".to_string(), Value::Str(symbol.clone())),
                    ("ib_cost_price".to_string(), Value::num(ib)),
                    (
                        "in_window_avg_fill".to_string(),
                        Value::Num(json::ratio_text(*avg)),
                    ),
                    (
                        "rel_diff_vs_ib".to_string(),
                        Value::Num(json::ratio_text(*rel)),
                    ),
                ])
            })
            .collect();
        let no_position =
            self.buys.len() - rows.iter().filter(|r| self.ib.contains_key(&r.0)).count();
        let mut notes = vec![
            Value::Str("IB Cost Price (WAC) is the ground truth; reconstructions report deltas vs IB, never as truth".to_string()),
            Value::Str("in-window avg fill is quantity-weighted over buy fills with Trade Price; sells excluded".to_string()),
            Value::Str("left-censored by construction: opens before the window, Xfer Price transfers and corporate-action basis are invisible to fills".to_string()),
        ];
        if no_fill > 0 {
            notes.push(Value::Str(format!(
                "positions with no in-window buy fills (pre-window or transfer opens): {no_fill}"
            )));
        }
        if no_position > 0 {
            notes.push(Value::Str(format!(
                "symbols with in-window buys but no Open Positions cost price: {no_position}"
            )));
        }
        if self.no_price > 0 {
            notes.push(Value::Str(format!(
                "buy fills without a positive Trade Price excluded from the average: {}",
                self.no_price
            )));
        }
        if self.dupes > 0 {
            notes.push(Value::Str(format!(
                "duplicate fills dropped (overlapping files): {}",
                self.dupes
            )));
        }
        Value::Obj(vec![
            ("compared".to_string(), Value::Num(rows.len().to_string())),
            ("symbols".to_string(), Value::Arr(row_list)),
            ("notes".to_string(), Value::Arr(notes)),
        ])
    }
}
