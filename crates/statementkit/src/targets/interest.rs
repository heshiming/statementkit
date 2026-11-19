//! `interest`: paid vs received in USD from `Interest` rows.
//!
//! Rows are deduplicated across files by (currency, date,
//! description, amount) because dailies and monthlies overlap.
//! Classification is by amount SIGN, never by label: linked-account
//! memos flip sign against their wording; the description only
//! sub-categorises into a product bucket. Rows are converted at
//! statement-date rates taken from `Base Currency Exchange Rate`
//! tables in the fed files (exact date, else nearest earlier date,
//! counted as a fallback). No external FX: rows with no fed rate are
//! unconverted and counted. The converted split is reconciled against
//! the USD net anchor, the `Change in NAV` keys `Interest` and
//! `Change in Interest Accruals`; the residual is conversion-basis
//! error and is reported, not absorbed. The USD accrual flow comes
//! from the `Interest Accruals` Base Currency Summary block.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::dates;
use crate::dec::Dec;
use crate::json::Value;
use crate::target::{RowRef, Target};

const ACCRUAL_BLOCK: &str = "Base Currency Summary";

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Row {
    ccy: String,
    date: String,
    desc: String,
    amount: Dec,
    base: String,
}

pub struct Interest {
    file_base: String,
    file_date: String,
    rates: BTreeMap<String, HashMap<String, Dec>>,
    rows: BTreeSet<Row>,
    dupes: usize,
    nav_interest: Option<Dec>,
    nav_delta: Option<Dec>,
    accrual: BTreeMap<String, Dec>,
    linked: usize,
}

impl Interest {
    pub fn new() -> Self {
        Interest {
            file_base: String::new(),
            file_date: String::new(),
            rates: BTreeMap::new(),
            rows: BTreeSet::new(),
            dupes: 0,
            nav_interest: None,
            nav_delta: None,
            accrual: BTreeMap::new(),
            linked: 0,
        }
    }

    /// (rate, fallback) for one row; None when no fed rate exists.
    fn rate_for(&self, row: &Row) -> Option<(Dec, bool)> {
        if row.ccy == row.base {
            return Some((Dec::ONE, false));
        }
        if let Some(map) = self.rates.get(&row.date) {
            if let Some(r) = map.get(&row.ccy) {
                return Some((*r, false));
            }
        }
        for (_, map) in self.rates.range(..row.date.clone()).rev() {
            if let Some(r) = map.get(&row.ccy) {
                return Some((*r, true));
            }
        }
        None
    }
}

fn bucket(desc: &str) -> &'static str {
    let d = desc.to_ascii_lowercase();
    if d.contains("borrow") {
        "borrow-fees"
    } else if d.contains("syep") {
        "syep"
    } else if d.contains("short") {
        "short-stock"
    } else if d.contains("insured") {
        "insured-deposit"
    } else if d.contains("debit") {
        "debit"
    } else if d.contains("credit") {
        "credit"
    } else {
        "other"
    }
}

impl Target for Interest {
    fn name(&self) -> &'static str {
        "interest"
    }

    fn on_file_begin(&mut self) {
        self.file_base.clear();
        self.file_date.clear();
    }

    fn on_meta(&mut self, key: &str, value: &str) {
        match key {
            "Currency" => self.file_base = value.to_string(),
            "Period" => {
                if let Some((Some(d), _, false)) = dates::stated_period(value) {
                    self.file_date = d;
                }
            }
            _ => {}
        }
    }

    fn on_row(&mut self, row: &RowRef) {
        match row.title {
            "Base Currency Exchange Rate" => {
                let (Some(ccy), Some(raw)) = (row.cell("Currency"), row.cell("Rate")) else {
                    return;
                };
                if let Ok(r) = Dec::parse(raw) {
                    if !self.file_date.is_empty() {
                        self.rates
                            .entry(self.file_date.clone())
                            .or_default()
                            .insert(ccy.to_string(), r);
                    }
                }
            }
            "Interest" => {
                let (Some(ccy), Some(raw_date), Some(desc), Some(raw)) = (
                    row.cell("Currency"),
                    row.cell("Date"),
                    row.cell("Description"),
                    row.cell("Amount"),
                ) else {
                    return;
                };
                let (Ok(amount), Some(date)) = (Dec::parse(raw), dates::stated_to_iso(raw_date))
                else {
                    return;
                };
                let d = desc.to_string();
                if d.to_ascii_lowercase().contains(" from ")
                    || d.to_ascii_lowercase().contains(" to ")
                {
                    self.linked += 1;
                }
                let r = Row {
                    ccy: ccy.to_string(),
                    date,
                    desc: d,
                    amount,
                    base: self.file_base.clone(),
                };
                if !self.rows.insert(r) {
                    self.dupes += 1;
                }
            }
            "Interest Accruals" => {
                if row.cell("Currency") != Some(ACCRUAL_BLOCK) {
                    return;
                }
                let (Some(field), Some(raw)) = (row.cell("Field Name"), row.cell("Field Value"))
                else {
                    return;
                };
                if let Ok(v) = Dec::parse(raw) {
                    let e = self.accrual.entry(field.to_string()).or_insert(Dec::ZERO);
                    *e = *e + v;
                }
            }
            "Change in Combined NAV" => {
                let (Some(field), Some(raw)) = (row.cell("Field"), row.cell("Value")) else {
                    return;
                };
                let Ok(v) = Dec::parse(raw) else {
                    return;
                };
                match field {
                    "Interest" => {
                        let e = self.nav_interest.get_or_insert(Dec::ZERO);
                        *e = *e + v;
                    }
                    "Change in Interest Accruals" => {
                        let e = self.nav_delta.get_or_insert(Dec::ZERO);
                        *e = *e + v;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn report(&self) -> Value {
        let mut fallbacks = 0usize;
        let mut unconverted: BTreeSet<String> = BTreeSet::new();
        let mut buckets: BTreeMap<(String, String), (usize, Dec, Dec)> = BTreeMap::new();
        let (mut received, mut paid) = (Dec::ZERO, Dec::ZERO);
        for r in &self.rows {
            let Some((rate, fell_back)) = self.rate_for(r) else {
                unconverted.insert(r.ccy.clone());
                continue;
            };
            if fell_back {
                fallbacks += 1;
            }
            let usd = r.amount * rate;
            let group = if r.ccy == r.base { "base" } else { "forex" };
            let e = buckets
                .entry((bucket(&r.desc).to_string(), group.to_string()))
                .or_insert((0, Dec::ZERO, Dec::ZERO));
            e.0 += 1;
            if usd >= Dec::ZERO {
                e.1 = e.1 + usd;
                received = received + usd;
            } else {
                e.2 = e.2 - usd;
                paid = paid - usd;
            }
        }
        let list: Vec<Value> = buckets
            .iter()
            .map(|((b, g), (n, rec, pd))| {
                Value::Obj(vec![
                    ("bucket".to_string(), Value::Str(b.clone())),
                    ("currency_group".to_string(), Value::Str(g.clone())),
                    ("rows".to_string(), Value::Num(n.to_string())),
                    ("usd_received".to_string(), Value::num(rec)),
                    ("usd_paid".to_string(), Value::num(pd)),
                ])
            })
            .collect();
        let net = received - paid;
        let anchor = self
            .nav_interest
            .map(|i| i + self.nav_delta.unwrap_or(Dec::ZERO));
        let residual = anchor.map(|a| net - a);
        let accrual = Value::Obj(
            self.accrual
                .iter()
                .map(|(k, v)| (k.clone(), Value::num(v)))
                .collect(),
        );
        let mut notes = vec![
            Value::Str("paid/received classified by amount sign; description only sub-categorises".to_string()),
            Value::Str("rates come only from fed statements; no external FX is built in".to_string()),
            Value::Str("residual against the USD net anchor is conversion-basis error, reported not absorbed".to_string()),
        ];
        if self.dupes > 0 {
            notes.push(Value::Str(format!(
                "duplicate rows dropped (daily/monthly overlap): {}",
                self.dupes
            )));
        }
        if fallbacks > 0 {
            notes.push(Value::Str(format!(
                "rows converted at the nearest earlier statement date: {fallbacks}"
            )));
        }
        if !unconverted.is_empty() {
            notes.push(Value::Str(format!(
                "currencies with no fed rate, rows excluded from the USD split: {}",
                unconverted.into_iter().collect::<Vec<_>>().join(", ")
            )));
        }
        if self.linked > 0 {
            notes.push(Value::Str(format!(
                "rows whose Description names a related account: {}",
                self.linked
            )));
        }
        if anchor.is_none() {
            notes.push(Value::Str(
                "no fed file carried the Change in NAV Interest key, no reconciliation".to_string(),
            ));
        }
        Value::Obj(vec![
            ("rows".to_string(), Value::Num(self.rows.len().to_string())),
            (
                "rate_dates".to_string(),
                Value::Num(self.rates.len().to_string()),
            ),
            ("buckets".to_string(), Value::Arr(list)),
            ("usd_received".to_string(), Value::num(&received)),
            ("usd_paid".to_string(), Value::num(&paid)),
            ("net".to_string(), Value::num(&net)),
            (
                "nav_anchor".to_string(),
                match anchor {
                    Some(a) => Value::num(&a),
                    None => Value::Null,
                },
            ),
            (
                "residual".to_string(),
                match residual {
                    Some(r) => Value::num(&r),
                    None => Value::Null,
                },
            ),
            ("accrual_summary".to_string(), accrual),
            ("notes".to_string(), Value::Arr(notes)),
        ])
    }
}
