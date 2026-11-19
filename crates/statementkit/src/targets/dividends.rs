//! `dividends`: dividend calendar and income run-rate.
//!
//! Calendar is the cash view: accrual rows keyed by `Pay Date`,
//! labeled as such (the entitlement view would key `Ex Date` — pick
//! one, label it). Posted `Dividends` rows join back to accruals by
//! pay date; postings matching no accrual (specials booked without
//! an accrual window) are counted, and their free-text `Description`
//! never reaches the output (account-token risk — counts only).
//!
//! Run-rate = Σ net by pay month over the trailing 12 months ÷ 12.
//! Forward = the same quarter last year from the same table (in-file
//! seasonality), not a mean. `Gross Rate` is IB's own yield figure —
//! quoted, never recomputed. Accruals and postings are never summed
//! together: the same dividend appears once in each. `Withholding
//! Tax` shows only when the section is present in the export.

use std::collections::BTreeMap;

use crate::dates;
use crate::dec::Dec;
use crate::json;
use crate::json::Value;
use crate::target::{RowRef, Target};

pub struct Dividends {
    accruals: Vec<(String, String, Dec)>, // (pay month, symbol, net)
    rates: BTreeMap<String, Vec<Dec>>,
    posted: Vec<(String, Dec)>, // (date iso or raw, amount)
    withholding: Option<Dec>,
}

impl Dividends {
    pub fn new() -> Self {
        Dividends {
            accruals: Vec::new(),
            rates: BTreeMap::new(),
            posted: Vec::new(),
            withholding: None,
        }
    }
}

fn dec(raw: Option<&str>) -> Option<Dec> {
    raw.and_then(|r| Dec::parse(r).ok())
}

fn month_or_raw(raw: &str) -> String {
    dates::month_key(raw).unwrap_or_else(|| raw.to_string())
}

impl Target for Dividends {
    fn name(&self) -> &'static str {
        "dividends"
    }

    fn on_row(&mut self, row: &RowRef) {
        match row.title {
            "Change in Dividend Accruals" => {
                let Some(net) = dec(row.cell("Net Amount")) else {
                    return;
                };
                let pay = row.cell("Pay Date").unwrap_or("????");
                let symbol = row.cell("Symbol").unwrap_or("?").to_string();
                self.accruals.push((month_or_raw(pay), symbol.clone(), net));
                if let Some(r) = dec(row.cell("Gross Rate")) {
                    let v = self.rates.entry(symbol.clone()).or_default();
                    if !v.contains(&r) {
                        v.push(r);
                    }
                }
            }
            "Dividends" => {
                if let Some(v) = dec(row.cell("Amount")) {
                    let d = row.cell("Date").unwrap_or("????");
                    self.posted.push((month_or_raw(d), v));
                }
            }
            "Withholding Tax" => {
                if let Some(v) = dec(row.cell("Amount")) {
                    let w = self.withholding.get_or_insert(Dec::ZERO);
                    self.withholding = Some(*w + v);
                }
            }
            _ => {}
        }
    }

    fn report(&self) -> Value {
        let mut by_month: BTreeMap<String, Dec> = BTreeMap::new();
        let mut by_symbol: BTreeMap<String, Dec> = BTreeMap::new();
        for (m, s, net) in &self.accruals {
            let e = by_month.entry(m.clone()).or_insert(Dec::ZERO);
            *e = *e + *net;
            let e = by_symbol.entry(s.clone()).or_insert(Dec::ZERO);
            *e = *e + *net;
        }
        let months: Vec<&String> = by_month.keys().collect();
        // trailing 12 calendar months back from the latest pay month
        let mk = |m: &str| -> Option<i32> {
            let y: i32 = m.get(..4)?.parse().ok()?;
            let mo: u32 = m.get(5..7)?.parse().ok()?;
            Some(y * 12 + mo as i32)
        };
        let trailing = match months.last().and_then(|m| mk(m)) {
            Some(last) => months
                .iter()
                .filter(|m| mk(m).is_some_and(|k| k > last - 12))
                .copied()
                .collect(),
            None => vec![],
        };
        let mut trailing_net = Dec::ZERO;
        for m in &trailing {
            trailing_net = trailing_net + by_month[m.as_str()];
        }
        let run_rate = trailing_net.to_f64() / 12.0;
        // forward: same calendar quarter of the prior year, same table
        let forward = months.last().and_then(|m| {
            let y: i32 = m[..4].parse().ok()?;
            let mo: u32 = m[5..7].parse().ok()?;
            let q = (mo - 1) / 3 + 1;
            let mut sum = None;
            for i in 0..3 {
                let key = format!("{:04}-{:02}", y - 1, (q - 1) * 3 + 1 + i);
                if let Some(v) = by_month.get(&key) {
                    sum = Some(sum.unwrap_or(Dec::ZERO) + *v);
                }
            }
            sum.map(|v| (format!("Q{q} {}", y - 1), v))
        });
        let calendar: Vec<Value> = months
            .iter()
            .map(|m| {
                Value::Obj(vec![
                    ("month".to_string(), Value::Str(m.to_string())),
                    ("net".to_string(), Value::num(&by_month[m.as_str()])),
                ])
            })
            .collect();
        let mut sym_rows: Vec<(&String, &Dec)> = by_symbol.iter().collect();
        sym_rows.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        let symbols: Vec<Value> = sym_rows
            .iter()
            .map(|(s, net)| {
                let rates = self
                    .rates
                    .get(*s)
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .map(Value::num)
                    .collect();
                Value::Obj(vec![
                    ("symbol".to_string(), Value::Str((*s).clone())),
                    ("net".to_string(), Value::num(net)),
                    ("quoted_gross_rates".to_string(), Value::Arr(rates)),
                ])
            })
            .collect();
        let unmatched = self
            .posted
            .iter()
            .filter(|(m, _)| !self.accruals.iter().any(|(am, _, _)| am == m))
            .count();
        let posted_net: Dec = self.posted.iter().fold(Dec::ZERO, |a, (_, v)| a + *v);
        let mut obj = vec![
            (
                "calendar_view".to_string(),
                Value::Str("cash (pay date)".to_string()),
            ),
            ("net_by_pay_month".to_string(), Value::Arr(calendar)),
            (
                "trailing_12m".to_string(),
                Value::Obj(vec![
                    ("months".to_string(), Value::Num(trailing.len().to_string())),
                    ("net".to_string(), Value::num(&trailing_net)),
                    (
                        "run_rate_monthly".to_string(),
                        Value::Num(json::ratio_text(run_rate)),
                    ),
                ]),
            ),
        ];
        if let Some((label, v)) = &forward {
            obj.push((
                "forward_same_quarter_prior_year".to_string(),
                Value::Obj(vec![
                    ("quarter".to_string(), Value::Str(label.clone())),
                    ("net".to_string(), Value::num(v)),
                ]),
            ));
        }
        obj.push(("symbols".to_string(), Value::Arr(symbols)));
        obj.push((
            "postings".to_string(),
            Value::Obj(vec![
                (
                    "rows".to_string(),
                    Value::Num(self.posted.len().to_string()),
                ),
                ("net".to_string(), Value::num(&posted_net)),
                (
                    "unmatched_by_pay_month".to_string(),
                    Value::Num(unmatched.to_string()),
                ),
            ]),
        ));
        obj.push((
            "withholding".to_string(),
            match self.withholding {
                Some(w) => Value::Obj(vec![
                    ("present".to_string(), Value::Str("true".to_string())),
                    ("amount".to_string(), Value::num(&w)),
                ]),
                None => Value::Obj(vec![(
                    "present".to_string(),
                    Value::Str("false".to_string()),
                )]),
            },
        ));
        obj.push((
            "notes".to_string(),
            Value::Arr(vec![
                Value::Str("accruals and postings are never summed: the same dividend appears once as each".to_string()),
                Value::Str("Gross Rate is IB's quoted figure, never recomputed".to_string()),
                Value::Str("postings are counted, never rendered: Description is free text with account-token risk".to_string()),
                Value::Str("special one-off dividends distort the 12-month mean; forward uses same-quarter-last-year seasonality, not the mean".to_string()),
            ]),
        ));
        Value::Obj(obj)
    }
}
