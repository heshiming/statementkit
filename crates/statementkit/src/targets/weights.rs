//! `weights`: position weights on a calendar from daily snapshots.
//!
//! The snapshot record is `Open Positions`, one per file at the file's
//! end date; there is no intraday position history. Class stack is
//! `Net Asset Value` table 1 read directly (indented sub-rows are
//! components, not classes). Symbol stack converts each `Value` at the
//! same file's `Base Currency Exchange Rate` and divides by the same
//! file's NAV `Total` — signed percent of NAV.
//!
//! Futures stay out of the slices: table 1 carries no futures class
//! (futures enter NAV as settled cash), so futures notional is reported
//! as an overlay sum instead of a slice, or the stack closure breaks.
//! Option legs group into (root, expiry) units; a unit reports net and
//! gross premium weight — Value is premium notional, not capped risk.
//! The stack sees overnight exposures only: intraday round trips appear
//! in no snapshot.
//!
//! Closure check per file: `Cash + accrual classes + non-futures
//! Value(USD) == NAV Total`. A failing file is reported as a parse-miss
//! suspect with its gap.

use std::collections::BTreeMap;

use crate::dates;
use crate::dec::Dec;
use crate::json;
use crate::json::Value;
use crate::target::{RowRef, Target};

struct Snap {
    date: Option<String>,
    base: String,
    total: Option<Dec>,
    classes: Vec<(String, Dec)>,
    rows: Vec<(String, String, Dec, String)>, // symbol, category, value, currency
    rates: BTreeMap<String, Dec>,
}

pub struct Weights {
    snaps: Vec<(usize, Snap)>,
    seq: usize,
    cur: Snap,
}

impl Weights {
    pub fn new() -> Self {
        Weights {
            snaps: Vec::new(),
            seq: 0,
            cur: blank_snap(),
        }
    }
}

fn blank_snap() -> Snap {
    Snap {
        date: None,
        base: String::new(),
        total: None,
        classes: Vec::new(),
        rows: Vec::new(),
        rates: BTreeMap::new(),
    }
}

fn dec(raw: Option<&str>) -> Option<Dec> {
    raw.and_then(|r| Dec::parse(r).ok())
}

fn weight(part: Dec, total: Dec) -> Value {
    json::ratio(if total != Dec::ZERO {
        Some(part.to_f64() / total.to_f64())
    } else {
        None
    })
}

fn option_unit(symbol: &str) -> Option<String> {
    let p: Vec<&str> = symbol.split_whitespace().collect();
    if p.len() == 4 && (p[3] == "C" || p[3] == "P") && p[1].len() == 7 {
        Some(format!("{} {}", p[0], p[1]))
    } else {
        None
    }
}

impl Target for Weights {
    fn name(&self) -> &'static str {
        "weights"
    }

    fn on_meta(&mut self, key: &str, value: &str) {
        if key == "Currency" {
            self.cur.base = value.to_string();
        } else if key == "Period" {
            if let Some((start, end, _)) = dates::stated_period(value) {
                self.cur.date = end.or(start);
            }
        }
    }

    fn on_row(&mut self, row: &RowRef) {
        match row.title {
            "Open Positions" => {
                if row.cell("DataDiscriminator").unwrap_or("Summary") != "Summary" {
                    return;
                }
                let (Some(symbol), Some(value), Some(category)) = (
                    row.cell("Symbol"),
                    dec(row.cell("Value")),
                    row.cell("Asset Category"),
                ) else {
                    return;
                };
                let ccy = row.cell("Currency").unwrap_or(&self.cur.base).to_string();
                self.cur
                    .rows
                    .push((symbol.to_string(), category.to_string(), value, ccy));
            }
            "Net Asset Value" => {
                // table 1 rows: Asset Class + Current Total; Total row is the denominator
                let (Some(class), Some(current)) =
                    (row.cell("Asset Class"), dec(row.cell("Current Total")))
                else {
                    return;
                };
                if class.trim() == "Total" {
                    self.cur.total = Some(current);
                } else if !class.starts_with(' ') {
                    self.cur.classes.push((class.to_string(), current));
                }
            }
            "Base Currency Exchange Rate" => {
                if let (Some(ccy), Some(rate)) =
                    (row.cell("Currency"), dec(row.cell("Exchange Rate")))
                {
                    self.cur.rates.insert(ccy.to_string(), rate);
                }
            }
            _ => {}
        }
    }

    fn on_file_end(&mut self) {
        self.snaps
            .push((self.seq, std::mem::replace(&mut self.cur, blank_snap())));
        self.seq += 1;
    }

    fn report(&self) -> Value {
        let mut snaps: Vec<&(usize, Snap)> = self.snaps.iter().collect();
        snaps.sort_by_key(|(seq, _)| *seq);
        let mut out: Vec<Value> = Vec::new();
        let mut note_dated: usize = 0;
        let mut closure_fail = 0usize;
        let mut skipped: usize = 0;

        for (_, s) in snaps {
            if s.rows.is_empty() && s.total.is_none() {
                continue;
            }
            let (Some(total), Some(date)) = (s.total, s.date.clone()) else {
                skipped += 1;
                continue;
            };
            note_dated += 1;
            // class stack
            let mut classes: Vec<Value> = Vec::new();
            let mut cash = Dec::ZERO;
            let mut accruals = Dec::ZERO;
            for (name, current) in &s.classes {
                if name == "Cash" {
                    cash = *current;
                } else if name.contains("Accrual") {
                    accruals = accruals + *current;
                }
                classes.push(Value::Obj(vec![
                    ("class".to_string(), Value::Str(name.clone())),
                    ("current".to_string(), Value::num(current)),
                    ("weight".to_string(), weight(*current, total)),
                ]));
            }
            // symbol stack: convert, split futures overlay, group option units
            let mut unrateable: BTreeMap<String, usize> = BTreeMap::new();
            let mut overlay = Dec::ZERO;
            let mut nonfut = Dec::ZERO;
            let mut symbols: BTreeMap<(String, String), Dec> = BTreeMap::new();
            let mut units: BTreeMap<String, (Dec, Dec)> = BTreeMap::new();
            let mut plain = 0usize;
            for (symbol, category, value, ccy) in &s.rows {
                let usd = if *ccy == s.base {
                    Some(*value)
                } else {
                    s.rates.get(ccy).map(|r| *value * *r)
                };
                let Some(usd) = usd else {
                    *unrateable.entry(ccy.clone()).or_insert(0) += 1;
                    continue;
                };
                if category == "Futures" {
                    overlay = overlay + usd;
                    continue;
                }
                nonfut = nonfut + usd;
                match option_unit(symbol) {
                    Some(unit) => {
                        let e = units.entry(unit).or_insert((Dec::ZERO, Dec::ZERO));
                        e.0 = e.0 + usd;
                        e.1 = e.1 + if usd < Dec::ZERO { -usd } else { usd };
                    }
                    None => {
                        plain += 1;
                        let e = symbols
                            .entry((symbol.clone(), category.clone()))
                            .or_insert(Dec::ZERO);
                        *e = *e + usd;
                    }
                }
            }
            let mut sym_list: Vec<Value> = Vec::new();
            for ((symbol, category), usd) in &symbols {
                sym_list.push(Value::Obj(vec![
                    ("symbol".to_string(), Value::Str(symbol.clone())),
                    ("category".to_string(), Value::Str(category.clone())),
                    ("value_usd".to_string(), Value::num(usd)),
                    ("weight".to_string(), weight(*usd, total)),
                ]));
            }
            let mut unit_list: Vec<Value> = Vec::new();
            for (unit, (net, gross)) in &units {
                unit_list.push(Value::Obj(vec![
                    ("unit".to_string(), Value::Str(unit.clone())),
                    ("net_usd".to_string(), Value::num(net)),
                    ("gross_usd".to_string(), Value::num(gross)),
                    ("net_weight".to_string(), weight(*net, total)),
                    ("gross_weight".to_string(), weight(*gross, total)),
                ]));
            }
            let gap = total - (cash + accruals + nonfut);
            if gap != Dec::ZERO {
                closure_fail += 1;
            }
            let mut notes: Vec<Value> = Vec::new();
            for (ccy, n) in &unrateable {
                notes.push(Value::Str(format!(
                    "{n} rows in {ccy} excluded: no rate in this file"
                )));
            }
            if gap != Dec::ZERO {
                notes.push(Value::Str(format!(
                    "closure gap {gap}: Cash+accruals+non-futures != NAV (suspect parse miss)"
                )));
            }
            out.push(Value::Obj(vec![
                ("date".to_string(), Value::Str(date)),
                ("nav_total".to_string(), Value::num(&total)),
                ("classes".to_string(), Value::Arr(classes)),
                ("symbols".to_string(), Value::Arr(sym_list)),
                ("plain_rows".to_string(), Value::Num(plain.to_string())),
                ("option_units".to_string(), Value::Arr(unit_list)),
                (
                    "futures_overlay".to_string(),
                    Value::Obj(vec![
                        ("value_usd".to_string(), Value::num(&overlay)),
                        ("weight".to_string(), weight(overlay, total)),
                    ]),
                ),
                (
                    "closure".to_string(),
                    Value::Obj(vec![
                        ("pass".to_string(), Value::Bool(gap == Dec::ZERO)),
                        ("gap".to_string(), Value::num(&gap)),
                    ]),
                ),
                ("notes".to_string(), Value::Arr(notes)),
            ]));
        }
        let mut notes = vec![
            Value::Str("class stack reads NAV table 1 directly; indented sub-rows are components and are skipped".to_string()),
            Value::Str("futures are an overlay line, never a slice: NAV table 1 has no futures class".to_string()),
            Value::Str("option unit weights are premium notional (net and gross); capped risk is a separate ledger".to_string()),
            Value::Str("snapshots show overnight positions only; intraday round trips appear nowhere".to_string()),
        ];
        if skipped > 0 {
            notes.push(Value::Str(format!(
                "files with positions but no NAV total or no date, skipped: {skipped}"
            )));
        }
        if closure_fail > 0 {
            notes.push(Value::Str(format!("files failing closure: {closure_fail}")));
        }
        Value::Obj(vec![
            ("snapshots".to_string(), Value::Arr(out)),
            (
                "dated_snapshots".to_string(),
                Value::Num(note_dated.to_string()),
            ),
            ("notes".to_string(), Value::Arr(notes)),
        ])
    }
}
