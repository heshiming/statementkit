//! `fx-exposure`: how much of the account is not base currency.
//!
//! Per snapshot, signed net by currency and gross (Σ|·|), over the
//! snapshot's NAV: Forex Balances `Value in USD` (already USD —
//! converting them again is the classic double-conversion bug) +
//! non-base `Open Positions.Value` converted at the statement's own
//! `Base Currency Exchange Rate` + per-currency cash at the same
//! rate. Shorts are signed: a short non-USD position is a short of
//! that currency, so the net column shows the offset and the gross
//! column the size — both ship.
//!
//! Rates exist only inside the daily window: positions or cash in a
//! currency with no rate row in the same file are counted out and
//! reported, never interpolated. `Other FX Translations` (the Change
//! in NAV key) answers "what did that exposure cost" — kept as its
//! own line, separate from exposure levels. `Paxos`/`Ideal Pro` are
//! cash segments, not currencies.

use std::collections::BTreeMap;

use crate::dates;
use crate::dec::Dec;
use crate::json;
use crate::json::Value;
use crate::target::{RowRef, Target};

#[derive(Default)]
struct Snapshot {
    month: String,
    nav: Option<Dec>,
    net: BTreeMap<String, Dec>, // currency -> signed USD exposure
}

#[derive(Default)]
struct MonthAgg {
    days: usize,
    net: f64,
    gross: f64,
    gross_max: f64,
    per_ccy: BTreeMap<String, f64>,
}

pub struct FxExposure {
    snapshots: Vec<Snapshot>,
    cur: Snapshot,
    rates: BTreeMap<String, Dec>,
    base_ccy: String,
    other_fx: Dec,
    unconverted: usize,
}

impl FxExposure {
    pub fn new() -> Self {
        FxExposure {
            snapshots: Vec::new(),
            cur: Snapshot::default(),
            rates: BTreeMap::new(),
            base_ccy: String::from("USD"),
            other_fx: Dec::ZERO,
            unconverted: 0,
        }
    }
}

fn dec(raw: Option<&str>) -> Option<Dec> {
    raw.and_then(|r| Dec::parse(r).ok())
}

impl Target for FxExposure {
    fn name(&self) -> &'static str {
        "fx-exposure"
    }

    fn on_meta(&mut self, key: &str, value: &str) {
        match key {
            "Currency" => self.base_ccy = value.trim().to_string(),
            "Period" => {
                self.cur.month = dates::stated_period(value)
                    .and_then(|(_, e, _)| e)
                    .and_then(|e| dates::month_key(&e))
                    .unwrap_or_default();
            }
            _ => {}
        }
    }

    fn on_file_end(&mut self) {
        if self.cur.nav.is_some() && !self.cur.month.is_empty() {
            self.snapshots.push(std::mem::take(&mut self.cur));
        }
        self.cur = Snapshot::default();
        self.rates.clear();
    }

    fn on_row(&mut self, row: &RowRef) {
        match row.title {
            "Forex Balances" => {
                if let (Some(c), Some(v)) = (row.cell("Currency"), dec(row.cell("Value in USD"))) {
                    if c != self.base_ccy {
                        let e = self.cur.net.entry(c.to_string()).or_insert(Dec::ZERO);
                        *e = *e + v;
                    }
                }
            }
            "Open Positions" => {
                if row.cell("DataDiscriminator").unwrap_or("Summary") != "Summary" {
                    return;
                }
                let (Some(c), Some(v)) = (row.cell("Currency"), dec(row.cell("Value"))) else {
                    return;
                };
                if c == self.base_ccy {
                    return;
                }
                match self.rates.get(c) {
                    Some(r) => {
                        let e = self.cur.net.entry(c.to_string()).or_insert(Dec::ZERO);
                        *e = *e + v * *r;
                    }
                    None => self.unconverted += 1,
                }
            }
            "Cash Report" => {
                if row.cell("Category").unwrap_or("") != "Currency Summary" {
                    return;
                }
                let (Some(c), Some(v)) = (row.cell("Currency"), dec(row.cell("Total"))) else {
                    return;
                };
                if c == self.base_ccy {
                    return;
                }
                match self.rates.get(c) {
                    Some(r) => {
                        let e = self.cur.net.entry(c.to_string()).or_insert(Dec::ZERO);
                        *e = *e + v * *r;
                    }
                    None => self.unconverted += 1,
                }
            }
            "Base Currency Exchange Rate" => {
                if let (Some(c), Some(r)) = (row.cell("Currency"), dec(row.cell("Exchange Rate"))) {
                    self.rates.insert(c.to_string(), r);
                }
            }
            "Change in Combined NAV" => {
                let Some(k) = row.cell("Key") else { return };
                match k {
                    "Ending Value" => self.cur.nav = dec(row.cell("Value")),
                    "Other FX Translations" => {
                        if let Some(v) = dec(row.cell("Value")) {
                            self.other_fx = self.other_fx + v;
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn report(&self) -> Value {
        let mut months: BTreeMap<String, MonthAgg> = BTreeMap::new();
        for s in &self.snapshots {
            let nav = s.nav.unwrap_or(Dec::ZERO).to_f64();
            if nav <= 0.0 {
                continue;
            }
            let net: f64 = s.net.values().map(|v| v.to_f64()).sum();
            let gross: f64 = s.net.values().map(|v| v.to_f64().abs()).sum();
            let m = months.entry(s.month.clone()).or_default();
            m.days += 1;
            m.net += net / nav;
            m.gross += gross / nav;
            if m.days == 1 || gross / nav > m.gross_max {
                m.gross_max = gross / nav;
            }
            for (c, v) in &s.net {
                *m.per_ccy.entry(c.clone()).or_insert(0.0) += v.to_f64() / nav;
            }
        }
        let rows: Vec<Value> = months
            .iter()
            .map(|(month, m)| {
                let mut top: Vec<(&String, f64)> = m
                    .per_ccy
                    .iter()
                    .map(|(c, v)| (c, v / m.days as f64))
                    .collect();
                top.sort_by(|a, b| {
                    b.1.abs()
                        .partial_cmp(&a.1.abs())
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                let top: Vec<Value> = top
                    .iter()
                    .take(3)
                    .map(|(c, v)| {
                        Value::Obj(vec![
                            ("currency".to_string(), Value::Str((*c).clone())),
                            (
                                "net_over_nav_avg".to_string(),
                                Value::Num(json::ratio_text(*v)),
                            ),
                        ])
                    })
                    .collect();
                Value::Obj(vec![
                    ("month".to_string(), Value::Str(month.clone())),
                    ("days".to_string(), Value::Num(m.days.to_string())),
                    (
                        "net_over_nav_avg".to_string(),
                        Value::Num(json::ratio_text(m.net / m.days as f64)),
                    ),
                    (
                        "gross_over_nav_avg".to_string(),
                        Value::Num(json::ratio_text(m.gross / m.days as f64)),
                    ),
                    (
                        "gross_over_nav_max".to_string(),
                        Value::Num(json::ratio_text(m.gross_max)),
                    ),
                    ("top_currencies".to_string(), Value::Arr(top)),
                ])
            })
            .collect();
        let mut notes = vec![
            Value::Str("net shows the signed offset, gross the size; a short non-USD position is a short of that currency".to_string()),
            Value::Str("Forex Balances USD columns are already converted and never converted again".to_string()),
            Value::Str("no rate in the same file = counted out, never interpolated; outside the daily window the statement ships no rate table".to_string()),
        ];
        if self.unconverted > 0 {
            notes.push(Value::Str(format!(
                "rows excluded for missing same-file rate: {}",
                self.unconverted
            )));
        }
        Value::Obj(vec![
            (
                "snapshots".to_string(),
                Value::Num(self.snapshots.len().to_string()),
            ),
            ("months".to_string(), Value::Arr(rows)),
            (
                "other_fx_translations".to_string(),
                Value::num(&self.other_fx),
            ),
            ("notes".to_string(), Value::Arr(notes)),
        ])
    }
}
