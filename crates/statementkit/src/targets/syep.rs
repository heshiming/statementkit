//! `syep`: stock-lending ledger, annual files only.
//!
//! Per symbol and currency, lending income = `Interest Paid to
//! Customer` + `SYEP Fee Earned by Customer` — the two cash lines of
//! the two Details tables, kept separate (different products:
//! customer share vs collateral yield) and summed only under the
//! labeled `total_lending_income`. Loan windows join the two tables
//! on (symbol, Start Date, Value Date). Lending yield is
//! held-day-weighted: income ÷ Σ(collateral × days(Start→Value)) ×
//! 365. The annual `Interest` section's SYEP description bucket is a
//! control total only — compared, reported as residual, never added
//! to the detail rows (same money, two grains). No annual file
//! carries loan termination; closed loans appear only via their fee
//! windows.

use std::collections::BTreeMap;

use crate::dates;
use crate::dec::Dec;
use crate::json;
use crate::json::Value;
use crate::target::{RowRef, Target};

#[derive(Default)]
struct SymBook {
    interest: Dec,
    fee: Dec,
    collateral_days: f64,
}

#[derive(Default)]
struct Window {
    collateral: Dec,
    interest: Dec,
    fee: Dec,
    syep_rate: String,
}

pub struct Syep {
    present: bool,
    syms: BTreeMap<(String, String), SymBook>,
    windows: BTreeMap<(String, String, String), Window>,
    control_detail: BTreeMap<String, Dec>,
    control_postings: BTreeMap<String, Dec>,
    file_ccy: String,
}

impl Syep {
    pub fn new() -> Self {
        Syep {
            present: false,
            syms: BTreeMap::new(),
            windows: BTreeMap::new(),
            control_detail: BTreeMap::new(),
            control_postings: BTreeMap::new(),
            file_ccy: String::from("USD"),
        }
    }
}

fn dec(raw: Option<&str>) -> Option<Dec> {
    raw.and_then(|r| Dec::parse(r).ok())
}

fn iso(raw: &str) -> String {
    dates::date_to_days(raw)
        .map(dates::days_to_iso)
        .unwrap_or_else(|| raw.to_string())
}

impl Target for Syep {
    fn name(&self) -> &'static str {
        "syep"
    }

    fn on_meta(&mut self, key: &str, value: &str) {
        if key == "Currency" {
            self.file_ccy = value.trim().to_string();
        }
    }

    fn on_row(&mut self, row: &RowRef) {
        let title = row.title;
        if title == "Interest" {
            let desc = row.cell("Description").unwrap_or("");
            let d = desc.to_ascii_lowercase();
            if d.contains("syep") || d.contains("yield enhancement") {
                if let Some(v) = dec(row.cell("Amount")) {
                    self.present = true;
                    let c = row.cell("Currency").unwrap_or(&self.file_ccy).to_string();
                    let e = self.control_postings.entry(c).or_insert(Dec::ZERO);
                    *e = *e + v;
                }
            }
            return;
        }
        let details = match title {
            "Stock Yield Enhancement Program Securities Lent Interest Details" => 1u8,
            "Stock Yield Enhancement Program Securities Lent Fee Earned Details" => 2,
            "Stock Yield Enhancement Program Securities Lent" => 3,
            _ => 0,
        };
        if details == 0 {
            return;
        }
        self.present = true;
        let Some(symbol) = row.cell("Symbol") else {
            return;
        };
        let ccy = row.cell("Currency").unwrap_or(&self.file_ccy).to_string();
        let start = iso(row.cell("Start Date").unwrap_or(""));
        let value = iso(row.cell("Value Date").unwrap_or(""));
        let collateral = dec(row.cell("Collateral Amount")).unwrap_or(Dec::ZERO);
        match details {
            1 => {
                let v = dec(row.cell("Interest Paid to Customer")).unwrap_or(Dec::ZERO);
                let b = self
                    .syms
                    .entry((symbol.to_string(), ccy.clone()))
                    .or_default();
                b.interest = b.interest + v;
                let days = dates::date_to_days(&value)
                    .zip(dates::date_to_days(&start))
                    .map(|(e, s)| (e - s).max(1))
                    .unwrap_or(1) as f64;
                b.collateral_days += collateral.to_f64() * days;
                let w = self
                    .windows
                    .entry((symbol.to_string(), start.clone(), value.clone()))
                    .or_default();
                w.collateral = collateral;
                w.interest = w.interest + v;
                let e = self.control_detail.entry(ccy).or_insert(Dec::ZERO);
                *e = *e + v;
            }
            2 => {
                let v = dec(row.cell("SYEP Fee Earned by Customer")).unwrap_or(Dec::ZERO);
                let b = self
                    .syms
                    .entry((symbol.to_string(), ccy.clone()))
                    .or_default();
                b.fee = b.fee + v;
                let w = self
                    .windows
                    .entry((symbol.to_string(), start.clone(), value.clone()))
                    .or_default();
                w.collateral = collateral;
                w.fee = w.fee + v;
                w.syep_rate = row
                    .cell("SYEP Rate on Customer Collateral (%)")
                    .unwrap_or("")
                    .to_string();
                let e = self.control_detail.entry(ccy).or_insert(Dec::ZERO);
                *e = *e + v;
            }
            _ => {}
        }
    }

    fn report(&self) -> Value {
        if !self.present {
            return Value::Obj(vec![
                ("present".to_string(), Value::Str("false".to_string())),
                (
                    "notes".to_string(),
                    Value::Arr(vec![Value::Str(
                        "no SYEP tables in these files; annual grain only in this corpus"
                            .to_string(),
                    )]),
                ),
            ]);
        }
        let symbols: Vec<Value> = self
            .syms
            .iter()
            .map(|((symbol, ccy), b)| {
                let total = b.interest + b.fee;
                let yield_ = if b.collateral_days > 0.0 {
                    Some(json::ratio_text(total.to_f64() / b.collateral_days * 365.0))
                } else {
                    None
                };
                let mut obj = vec![
                    ("symbol".to_string(), Value::Str(symbol.clone())),
                    ("currency".to_string(), Value::Str(ccy.clone())),
                    ("interest_paid".to_string(), Value::num(&b.interest)),
                    ("fee_earned".to_string(), Value::num(&b.fee)),
                    ("total_lending_income".to_string(), Value::num(&total)),
                ];
                if let Some(y) = yield_ {
                    obj.push(("annualized_yield_on_collateral".to_string(), Value::Num(y)));
                }
                Value::Obj(obj)
            })
            .collect();
        let windows: Vec<Value> = self
            .windows
            .iter()
            .filter(|(_, w)| w.interest != Dec::ZERO || w.fee != Dec::ZERO)
            .map(|((symbol, start, value), w)| {
                Value::Obj(vec![
                    ("symbol".to_string(), Value::Str(symbol.clone())),
                    ("start".to_string(), Value::Str(start.clone())),
                    ("value_date".to_string(), Value::Str(value.clone())),
                    ("collateral".to_string(), Value::num(&w.collateral)),
                    ("syep_rate_pct".to_string(), Value::Str(w.syep_rate.clone())),
                    ("interest".to_string(), Value::num(&w.interest)),
                    ("fee".to_string(), Value::num(&w.fee)),
                ])
            })
            .collect();
        let control: Vec<Value> = self
            .control_postings
            .keys()
            .chain(self.control_detail.keys())
            .collect::<std::collections::BTreeSet<_>>()
            .iter()
            .map(|c| {
                let d = self.control_detail.get(*c).copied().unwrap_or(Dec::ZERO);
                let p = self.control_postings.get(*c).copied().unwrap_or(Dec::ZERO);
                Value::Obj(vec![
                    ("currency".to_string(), Value::Str((*c).clone())),
                    ("detail_cash_total".to_string(), Value::num(&d)),
                    ("interest_postings_total".to_string(), Value::num(&p)),
                    ("residual".to_string(), Value::num(&(p - d))),
                ])
            })
            .collect();
        Value::Obj(vec![
            ("present".to_string(), Value::Str("true".to_string())),
            ("symbols".to_string(), Value::Arr(symbols)),
            ("loan_windows".to_string(), Value::Arr(windows)),
            ("control".to_string(), Value::Arr(control)),
            (
                "notes".to_string(),
                Value::Arr(vec![
                    Value::Str("interest and fee are different products; summed only as labeled total lending income".to_string()),
                    Value::Str("Interest-section SYEP bucket is a control total: compared, never added to detail rows".to_string()),
                    Value::Str("no annual file carries loan termination; closed loans appear only via their fee windows".to_string()),
                    Value::Str("monthly lending series would need the Interest rows' text buckets, which carry no symbol".to_string()),
                ]),
            ),
        ])
    }
}
