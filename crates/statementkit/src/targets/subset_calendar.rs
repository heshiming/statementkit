//! `subset-calendar`: the selection day by day, and what is still open.
//!
//! Two grains, neither interchangeable:
//!
//! - *calendar* — trades realized + commissions of the roots per
//!   booking day (the file's `Period` from, not the fill stamp),
//!   rolled to ISO weeks, with the K-invariant set on the daily
//!   series annualized ×√252. This grain is trades-only: the
//!   dailies template carries no dividend or borrow sections. It
//!   reconciles with `subset-ledger` only inside one month, never
//!   across one, and its level answers "how much settled that
//!   day", not P&L.
//! - *marks* — the last day the roots appear in `Open Positions`
//!   at all (the machinery walks back and says which day it
//!   used); `Value` at that same file's `Base Currency Exchange
//!   Rate`, option legs grouped by root, signed by quantity
//!   sign, weighed against that file's NAV.
//!
//! Marks are a weight, not a risk statement: short legs are
//! negative, option legs sit at premium notional, and gross can
//! exceed NAV. On a sparse selection (few active days) the
//! Sharpe is small-number arithmetic — read the dollars first.

use std::collections::BTreeMap;

use crate::dates;
use crate::dec::Dec;
use crate::json;
use crate::json::Value;
use crate::target::{RowRef, Target};
use crate::tickers::root_up;

fn dec(raw: Option<&str>) -> Option<Dec> {
    raw.and_then(|r| Dec::parse(r).ok())
}

/// ISO Monday of the week containing an ISO date.
fn week_monday(day: &str) -> Option<String> {
    let d = dates::iso_to_days(day)?;
    // 1970-01-01 (day 0) is a Thursday: weekday 3 with Monday = 0.
    let wday = (d + 3).rem_euclid(7);
    Some(dates::days_to_iso(d - wday))
}

type Marks = BTreeMap<String, (usize, Dec)>;

pub struct SubsetCalendar {
    base_ccy: String,
    day: Option<String>,
    rates: BTreeMap<String, Dec>,
    daily: BTreeMap<String, Dec>,
    non_usd: usize,
    marks_by_day: BTreeMap<String, Marks>,
    nav_by_day: BTreeMap<String, Dec>,
    cur_marks: Marks,
    cur_nav: Option<Dec>,
    unconverted_legs: usize,
}

impl SubsetCalendar {
    pub fn new() -> Self {
        SubsetCalendar {
            base_ccy: String::new(),
            day: None,
            rates: BTreeMap::new(),
            daily: BTreeMap::new(),
            non_usd: 0,
            marks_by_day: BTreeMap::new(),
            nav_by_day: BTreeMap::new(),
            cur_marks: BTreeMap::new(),
            cur_nav: None,
            unconverted_legs: 0,
        }
    }

    fn usd_value(&mut self, row: &RowRef) -> Option<Dec> {
        let ccy = row.cell("Currency").unwrap_or("USD").trim().to_string();
        let v = dec(row.cell("Value"))?;
        let v = match dec(row.cell("Quantity")) {
            Some(q) if q < Dec::ZERO => Dec::ZERO - v,
            _ => v,
        };
        if self.base_ccy.is_empty() || ccy == self.base_ccy {
            return Some(v);
        }
        match self.rates.get(&ccy) {
            Some(r) => {
                let r = *r;
                Some(v * r)
            }
            None => {
                self.unconverted_legs += 1;
                None
            }
        }
    }
}

impl Target for SubsetCalendar {
    fn name(&self) -> &'static str {
        "subset-calendar"
    }

    fn on_file_begin(&mut self) {
        self.day = None;
        self.rates.clear();
        self.cur_marks.clear();
        self.cur_nav = None;
    }

    fn on_meta(&mut self, key: &str, value: &str) {
        match key {
            "Period" => {
                self.day = dates::stated_period(value)
                    .and_then(|(f, _, _)| f)
                    .filter(|f| f.len() >= 10);
            }
            "Currency" => self.base_ccy = value.trim().to_string(),
            _ => {}
        }
    }

    fn on_row(&mut self, row: &RowRef) {
        match row.title {
            "Trades" => {
                let r = dec(row.cell("Realized P/L")).unwrap_or(Dec::ZERO);
                let c = dec(row.cell("Comm/Fee")).unwrap_or(Dec::ZERO);
                if row.cell("Realized P/L").is_none() && row.cell("Comm/Fee").is_none() {
                    return;
                }
                if row.cell("Symbol").unwrap_or("").trim().is_empty() {
                    return;
                }
                let ccy = row.cell("Currency").unwrap_or("USD").trim();
                if !self.base_ccy.is_empty() && ccy != self.base_ccy {
                    self.non_usd += 1;
                    return;
                }
                if let Some(day) = &self.day {
                    let e = self.daily.entry(day.clone()).or_default();
                    *e = *e + r + c;
                }
            }
            "Base Currency Exchange Rate" => {
                if let (Some(c), Some(r)) = (row.cell("Currency"), dec(row.cell("Exchange Rate"))) {
                    self.rates.insert(c.trim().to_string(), r);
                }
            }
            "Open Positions" => {
                let sym = row.cell("Symbol").unwrap_or("").trim();
                if sym.is_empty() {
                    return;
                }
                let root = root_up(sym);
                let v = self.usd_value(row);
                let legs = self.cur_marks.entry(root).or_insert((0, Dec::ZERO));
                legs.0 += 1;
                if let Some(v) = v {
                    legs.1 = legs.1 + v;
                }
            }
            "Change in NAV" | "Change in Combined NAV"
                if row.cell("Field") == Some("Ending Value") =>
            {
                if let Some(v) = dec(row.cell("Value")) {
                    self.cur_nav = Some(v);
                }
            }
            _ => {}
        }
    }

    fn on_file_end(&mut self) {
        if let Some(day) = &self.day {
            if !self.cur_marks.is_empty() {
                self.marks_by_day
                    .insert(day.clone(), self.cur_marks.clone());
            }
            if let Some(n) = self.cur_nav {
                self.nav_by_day.insert(day.clone(), n);
            }
        }
    }

    fn report(&self) -> Value {
        let days_with: usize = self.daily.values().filter(|v| **v != Dec::ZERO).count();
        let total: Dec = self.daily.values().fold(Dec::ZERO, |a, b| a + *b);
        let mut weeks: BTreeMap<String, (Dec, usize)> = BTreeMap::new();
        for (day, v) in &self.daily {
            if let Some(w) = week_monday(day) {
                let e = weeks.entry(w).or_insert((Dec::ZERO, 0));
                e.0 = e.0 + *v;
                if *v != Dec::ZERO {
                    e.1 += 1;
                }
            }
        }
        let week_v: Vec<Value> = weeks
            .iter()
            .map(|(w, (t, d))| {
                Value::Obj(vec![
                    ("week_of".to_string(), Value::Str(w.clone())),
                    ("total".to_string(), Value::num(t)),
                    ("days_with_money".to_string(), Value::Num(d.to_string())),
                ])
            })
            .collect();
        let pnls: Vec<f64> = self.daily.values().map(|v| v.to_f64()).collect();
        let mut calendar = vec![
            (
                "days_with_money".to_string(),
                Value::Num(days_with.to_string()),
            ),
            ("total".to_string(), Value::num(&total)),
            ("weeks".to_string(), Value::Arr(week_v)),
            ("stats".to_string(), stats(&pnls)),
        ];
        let mut notes = vec![
            Value::Str("trades-only grain: the dailies template carries no Dividends or Borrow Fee Details section".to_string()),
            Value::Str("reconciles with subset-ledger only inside one month; the level is settled dollars, not P&L".to_string()),
        ];
        if pnls.len() < 10 {
            notes.push(Value::Str(format!(
                "sparse: {} daily prints — the Sharpe here is small-number arithmetic, read the dollars first",
                pnls.len()
            )));
        }
        calendar.push(("notes".to_string(), Value::Arr(notes)));

        // marks: last day the roots appear at all
        let day_used = self.marks_by_day.keys().next_back().cloned();
        let mut marks_obj = Vec::new();
        if let Some(day) = &day_used {
            let marks = &self.marks_by_day[day];
            let nav = self.nav_by_day.get(day);
            let mut root_rows: Vec<Value> = Vec::new();
            let mut sum = Dec::ZERO;
            for (root, (legs, v)) in marks {
                sum = sum + *v;
                let pct = match nav {
                    Some(n) if *n != Dec::ZERO => Some(v.to_f64() / n.to_f64()),
                    _ => None,
                };
                root_rows.push(Value::Obj(vec![
                    ("root".to_string(), Value::Str(root.clone())),
                    ("legs".to_string(), Value::Num(legs.to_string())),
                    ("mark_usd".to_string(), Value::num(v)),
                    (
                        "pct_of_nav".to_string(),
                        pct.map_or(Value::Null, |p| Value::Num(json::ratio_text(p))),
                    ),
                ]));
            }
            marks_obj.push(("day_used".to_string(), Value::Str(day.clone())));
            marks_obj.push(("roots".to_string(), Value::Arr(root_rows)));
            marks_obj.push(("total".to_string(), Value::num(&sum)));
            marks_obj.push(("nav".to_string(), nav.map_or(Value::Null, Value::num)));
        } else {
            marks_obj.push(("day_used".to_string(), Value::Null));
            marks_obj.push(("roots".to_string(), Value::Arr(Vec::new())));
        }
        marks_obj.push((
            "notes".to_string(),
            Value::Arr(vec![
                Value::Str("marks are signed (short legs negative); option legs carry premium notional, so %-of-NAV is a weight, not a risk statement".to_string()),
                Value::Str(format!(
                    "legs with no same-file exchange rate are counted, never interpolated: {}",
                    self.unconverted_legs
                )),
            ]),
        ));

        Value::Obj(vec![
            ("calendar".to_string(), Value::Obj(calendar)),
            ("marks".to_string(), Value::Obj(marks_obj)),
            (
                "non_usd_rows".to_string(),
                Value::Num(self.non_usd.to_string()),
            ),
        ])
    }
}

/// The K-invariant set over the daily dollar series, annualized ×√252.
fn stats(pnls: &[f64]) -> Value {
    let total: f64 = pnls.iter().sum();
    let mut cum = 0.0f64;
    let mut peak = 0.0f64;
    let mut max_dd = 0.0f64;
    for p in pnls {
        cum += p;
        if cum > peak {
            peak = cum;
        }
        if cum - peak < max_dd {
            max_dd = cum - peak;
        }
    }
    let pos: f64 = pnls.iter().filter(|p| **p > 0.0).sum();
    let neg: f64 = pnls.iter().filter(|p| **p < 0.0).sum();
    let n = pnls.len() as f64;
    let mean = if n > 0.0 { total / n } else { 0.0 };
    let var = if n > 0.0 {
        pnls.iter().map(|p| (p - mean) * (p - mean)).sum::<f64>() / n
    } else {
        0.0
    };
    let sd = var.sqrt();
    let sharpe = if sd > 0.0 {
        Some(mean / sd * 252.0f64.sqrt())
    } else {
        None
    };
    Value::Obj(vec![
        ("pnl".to_string(), Value::Num(json::ratio_text(total))),
        (
            "max_drawdown".to_string(),
            Value::Num(json::ratio_text(max_dd)),
        ),
        (
            "sharpe_annualized".to_string(),
            sharpe.map_or(Value::Null, |s| Value::Num(json::ratio_text(s))),
        ),
        (
            "recovery".to_string(),
            if max_dd < 0.0 {
                Value::Num(json::ratio_text(total / max_dd.abs()))
            } else {
                Value::Null
            },
        ),
        (
            "profit_factor".to_string(),
            if neg < 0.0 {
                Value::Num(json::ratio_text(pos / neg.abs()))
            } else {
                Value::Null
            },
        ),
        ("days".to_string(), Value::Num(pnls.len().to_string())),
    ])
}
