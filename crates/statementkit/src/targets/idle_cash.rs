//! `idle-cash`: what rate uninvested cash actually earns.
//!
//! Three honest views, kept apart:
//!
//! - sweep posted rate: the `Insured Bank Deposit Sweep Program - By
//!   Bank` table's own `Interest Rate (%)` per `Program Bank` — that
//!   column IS the earned rate for swept balances; min/median/max
//!   across rows, banks listed;
//! - realized view: monthly `Credit Interest` postings ÷ average
//!   `Cash Report` base-currency balance over the month, annualized
//!   by days in month; cash basis labeled `daily mean` when the
//!   month has several snapshots, `month-end approx` with one, and
//!   `n/m net short cash` when the balance is not positive;
//! - accrual view: `Interest Accruals` accrued ÷ the file's cash,
//!   annualized over the period span (the accrual BLOCK balance is a
//!   running ledger sum, not the cash that earned it — cash is the
//!   denominator).
//!
//! Segments (securities / futures / Paxos) are reported from the
//! latest file: they earn different rates, one blended number hides
//! that. The risk-free comparison line is external data, not here.

use std::collections::BTreeMap;

use crate::dates;
use crate::dec::Dec;
use crate::json;
use crate::json::Value;
use crate::target::{RowRef, Target};

#[derive(Default)]
struct FileRec {
    month: String,
    start_days: Option<i64>,
    end_days: Option<i64>,
    cash: Option<Dec>,
    securities: Dec,
    futures: Dec,
    paxos: Dec,
    accrued: Dec,
    has_accrued: bool,
}

pub struct IdleCash {
    files: Vec<FileRec>,
    cur: FileRec,
    sweep_rows: usize,
    sweep_rates: Vec<f64>,
    banks: Vec<String>,
    credit_by_month: BTreeMap<String, Dec>,
}

impl IdleCash {
    pub fn new() -> Self {
        IdleCash {
            files: Vec::new(),
            cur: FileRec::default(),
            sweep_rows: 0,
            sweep_rates: Vec::new(),
            banks: Vec::new(),
            credit_by_month: BTreeMap::new(),
        }
    }
}

fn dec(raw: Option<&str>) -> Option<Dec> {
    raw.and_then(|r| Dec::parse(r).ok())
}

fn days_in_month(m: &str) -> i64 {
    let y: i64 = m[..4].parse().unwrap_or(2000);
    let mo: i64 = m[5..7].parse().unwrap_or(1);
    let next = if mo == 12 {
        dates::iso_to_days(&format!("{}-01-01", y + 1)).unwrap_or(0)
    } else {
        dates::iso_to_days(&format!("{y}-{:02}-01", mo + 1)).unwrap_or(0)
    };
    let this = dates::iso_to_days(&format!("{y}-{mo:02}-01")).unwrap_or(0);
    next - this
}

impl Target for IdleCash {
    fn name(&self) -> &'static str {
        "idle-cash"
    }

    fn on_meta(&mut self, key: &str, value: &str) {
        if key == "Period" {
            if let Some((s, e, _)) = dates::stated_period(value) {
                self.cur.start_days = s.and_then(|s| dates::iso_to_days(&s));
                self.cur.end_days = e.as_ref().and_then(|e| dates::iso_to_days(e));
                self.cur.month = e.as_deref().and_then(dates::month_key).unwrap_or_default();
            }
        }
    }

    fn on_file_end(&mut self) {
        self.files.push(std::mem::take(&mut self.cur));
    }

    fn on_row(&mut self, row: &RowRef) {
        match row.title {
            "Cash Report" => {
                if row.cell("Category").unwrap_or("") != "Base Currency Summary" {
                    return;
                }
                if let Some(v) = dec(row.cell("Total")) {
                    self.cur.cash = Some(v);
                }
                self.cur.securities = dec(row.cell("Securities")).unwrap_or(Dec::ZERO);
                self.cur.futures = dec(row.cell("Futures")).unwrap_or(Dec::ZERO);
                self.cur.paxos = dec(row.cell("Paxos")).unwrap_or(Dec::ZERO);
            }
            "Insured Bank Deposit Sweep Program - By Bank" => {
                self.sweep_rows += 1;
                if let Some(b) = row.cell("Program Bank") {
                    if !self.banks.iter().any(|x| x == b) {
                        self.banks.push(b.to_string());
                    }
                }
                if let Some(r) = dec(row.cell("Interest Rate (%)")) {
                    self.sweep_rates.push(r.to_f64() / 100.0);
                }
            }
            "Interest" => {
                let d = row.cell("Description").unwrap_or("").to_ascii_lowercase();
                if d.contains("credit interest") || d.contains("insured deposit credit") {
                    if let (Some(v), Some(m)) = (
                        dec(row.cell("Amount")),
                        row.cell("Date").and_then(dates::month_key),
                    ) {
                        let e = self.credit_by_month.entry(m).or_insert(Dec::ZERO);
                        *e = *e + v;
                    }
                }
            }
            "Interest Accruals" => {
                if let Some(v) = dec(row.cell("Interest Accrued")) {
                    self.cur.accrued = self.cur.accrued + v;
                    self.cur.has_accrued = true;
                }
            }
            _ => {}
        }
    }

    fn report(&self) -> Value {
        // realized view: credit interest vs average cash by month
        let mut cash_by_month: BTreeMap<String, (usize, Dec)> = BTreeMap::new();
        for f in &self.files {
            if !f.month.is_empty() {
                if let Some(c) = f.cash {
                    let e = cash_by_month
                        .entry(f.month.clone())
                        .or_insert((0, Dec::ZERO));
                    e.0 += 1;
                    e.1 = e.1 + c;
                }
            }
        }
        let mut months: Vec<Value> = Vec::new();
        let mut keys: Vec<String> = cash_by_month
            .keys()
            .chain(self.credit_by_month.keys())
            .cloned()
            .collect();
        keys.sort();
        keys.dedup();
        for m in keys {
            let Some((n, sum)) = cash_by_month.get(&m) else {
                continue;
            };
            let avg = Dec::parse(&json::ratio_text(sum.to_f64() / *n as f64)).ok();
            let interest = self.credit_by_month.get(&m).copied().unwrap_or(Dec::ZERO);
            let mut obj = vec![
                ("month".to_string(), Value::Str(m.clone())),
                ("credit_interest".to_string(), Value::num(&interest)),
            ];
            match avg {
                Some(a) if a > Dec::ZERO => {
                    obj.push(("avg_cash".to_string(), Value::num(&a)));
                    if let Some(a) = avg {
                        let y = interest.to_f64() / a.to_f64() * 365.0 / days_in_month(&m) as f64;
                        obj.push((
                            "annualized_yield".to_string(),
                            Value::Num(json::ratio_text(y)),
                        ));
                    }
                    obj.push((
                        "cash_basis".to_string(),
                        Value::Str(
                            if *n > 1 {
                                "daily mean"
                            } else {
                                "month-end approx"
                            }
                            .to_string(),
                        ),
                    ));
                }
                _ => {
                    if let Some(a) = avg {
                        obj.push(("avg_cash".to_string(), Value::num(&a)));
                    }
                    obj.push((
                        "annualized_yield".to_string(),
                        Value::Str("n/m".to_string()),
                    ));
                    obj.push((
                        "cash_basis".to_string(),
                        Value::Str("n/m net short cash".to_string()),
                    ));
                }
            }
            months.push(Value::Obj(obj));
        }
        // accrual view per file with cash and a dated period
        let accrual: Vec<Value> = self
            .files
            .iter()
            .filter(|f| f.has_accrued && f.cash.is_some_and(|c| c > Dec::ZERO))
            .filter_map(|f| {
                let (s, e) = (f.start_days?, f.end_days?);
                let days = (e - s).max(1);
                let c = f.cash?;
                Some(Value::Obj(vec![
                    ("month".to_string(), Value::Str(f.month.clone())),
                    ("accrued".to_string(), Value::num(&f.accrued)),
                    (
                        "annualized_yield".to_string(),
                        Value::Num(json::ratio_text(
                            f.accrued.to_f64() / c.to_f64() * 365.0 / days as f64,
                        )),
                    ),
                    ("days".to_string(), Value::Num(days.to_string())),
                ]))
            })
            .collect();
        // sweep rates + latest segments
        let mut obj = Vec::new();
        if self.sweep_rows > 0 {
            let mut r = self.sweep_rates.clone();
            r.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let med = if r.is_empty() {
                0.0
            } else if r.len() % 2 == 1 {
                r[r.len() / 2]
            } else {
                (r[r.len() / 2 - 1] + r[r.len() / 2]) / 2.0
            };
            obj.push((
                "sweep_posted_rate".to_string(),
                Value::Obj(vec![
                    ("rows".to_string(), Value::Num(self.sweep_rows.to_string())),
                    (
                        "banks".to_string(),
                        Value::Arr(self.banks.iter().map(|b| Value::Str(b.clone())).collect()),
                    ),
                    (
                        "min".to_string(),
                        Value::Num(json::ratio_text(r.first().copied().unwrap_or(0.0))),
                    ),
                    ("median".to_string(), Value::Num(json::ratio_text(med))),
                    (
                        "max".to_string(),
                        Value::Num(json::ratio_text(r.last().copied().unwrap_or(0.0))),
                    ),
                ]),
            ));
        }
        if let Some(f) = self.files.iter().rev().find(|f| f.cash.is_some()) {
            obj.push((
                "latest_segments".to_string(),
                Value::Obj(vec![
                    (
                        "cash_total".to_string(),
                        Value::num(&f.cash.unwrap_or(Dec::ZERO)),
                    ),
                    ("securities".to_string(), Value::num(&f.securities)),
                    ("futures".to_string(), Value::num(&f.futures)),
                    ("paxos".to_string(), Value::num(&f.paxos)),
                ]),
            ));
        }
        obj.push(("months".to_string(), Value::Arr(months)));
        obj.push(("accrual_view".to_string(), Value::Arr(accrual)));
        obj.push((
            "notes".to_string(),
            Value::Arr(vec![
                Value::Str("sweep vs brokerage vs futures vs Paxos cash earn different rates; one blended number hides that".to_string()),
                Value::Str("posting-vs-accrual timing: the two views are two honest answers, never averaged together".to_string()),
                Value::Str("the accrual block balance is a running ledger sum; cash is the denominator".to_string()),
                Value::Str("risk-free comparison line needs external data (docs 04), not fetched".to_string()),
            ]),
        ));
        Value::Obj(obj)
    }
}
