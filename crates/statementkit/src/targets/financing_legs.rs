//! `financing-legs`: financing is stated on the funded leg.
//!
//! Buckets `Interest` rows by attributed `Account` and by
//! description keyword — SYEP, credit, debit, other — over USD
//! rows; non-USD rows are counted and excluded. Description
//! decides the bucket, not sign: debit-descript rows arriving as
//! positive reversals belong to the debit side, which is how a
//! funded leg can net its debit interest positive.
//!
//! A listed member that carries no interest rows is reported in
//! `members_without_interest`: a zero-funded sub trading on family
//! margin books no funding cost against its own P&L, so its raw
//! dollar ledger is systematically overstated by exactly the
//! collateral return booked elsewhere in the family.
//!
//! The corrective charge — average negative balance × family debit
//! rate, before grading the trader — cannot be computed from these
//! files: interest rows carry amounts, not rates. No rate is
//! invented here.

use std::collections::BTreeMap;

use crate::dec::Dec;
use crate::json::Value;
use crate::target::{RowRef, Target};

fn bare_id(raw: &str) -> String {
    raw.split_whitespace()
        .next()
        .unwrap_or(raw)
        .trim_end_matches(',')
        .to_string()
}

#[derive(Default)]
struct Bucket {
    rows: usize,
    usd: Dec,
}

#[derive(Default)]
struct Leg {
    buckets: [Bucket; 4],
    non_usd: usize,
}

const NAMES: [&str; 4] = ["syep", "credit", "debit", "other"];

fn classify(desc: &str, amount: Dec) -> usize {
    let d = desc.to_uppercase();
    if d.contains("SYEP") || d.contains("STOCK YIELD") || d.contains("BY CUSTOMER") {
        return 0;
    }
    if d.contains("INT CHG") || d.contains("DEBIT") || d.contains("CHARGED") {
        return 2;
    }
    if d.contains("CR INT") || d.contains("CREDIT INT") || amount > Dec::ZERO {
        return 1;
    }
    3
}

pub struct FinancingLegs {
    base_ccy: String,
    file_account: Option<String>,
    stated: std::collections::BTreeSet<String>,
    included: Vec<String>,
    info: BTreeMap<String, String>,
    legs: BTreeMap<String, Leg>,
}

impl FinancingLegs {
    pub fn new() -> Self {
        FinancingLegs {
            base_ccy: String::new(),
            file_account: None,
            stated: std::collections::BTreeSet::new(),
            included: Vec::new(),
            info: BTreeMap::new(),
            legs: BTreeMap::new(),
        }
    }
}

impl Target for FinancingLegs {
    fn name(&self) -> &'static str {
        "financing-legs"
    }

    fn on_file_begin(&mut self) {
        self.file_account = None;
        self.info.clear();
    }

    fn on_meta(&mut self, key: &str, value: &str) {
        match key {
            "Currency" => self.base_ccy = value.trim().to_string(),
            "Account ID" => self.file_account = Some(bare_id(value)),
            _ => {}
        }
    }

    fn on_row(&mut self, row: &RowRef) {
        match row.title {
            "Account Information" => {
                if let (Some(f), Some(v)) = (row.cell("Field"), row.cell("Value")) {
                    self.info.insert(f.to_string(), v.to_string());
                }
                let acc = self.info.get("Account").map(|v| bare_id(v));
                if let Some(list) = self.info.get("Accounts Included") {
                    let list: Vec<String> = list
                        .split(',')
                        .map(|s| bare_id(s.trim()))
                        .filter(|s| !s.is_empty())
                        .collect();
                    if list.len() > self.included.len() {
                        self.included = list;
                    }
                }
                if let Some(a) = acc.or_else(|| self.file_account.clone()) {
                    self.stated.insert(a.clone());
                    self.legs.entry(a).or_default();
                }
            }
            "Interest" => {
                let Some(raw) = row.cell("Amount") else {
                    return;
                };
                let Ok(amount) = Dec::parse(raw) else {
                    return;
                };
                let leg = row
                    .cell("Account")
                    .map(bare_id)
                    .or_else(|| self.file_account.clone())
                    .unwrap_or_else(|| "unknown".to_string());
                if let Some(ccy) = row.cell("Currency") {
                    if !self.base_ccy.is_empty() && ccy != self.base_ccy {
                        self.legs.entry(leg).or_default().non_usd += 1;
                        return;
                    }
                }
                let b = classify(row.cell("Description").unwrap_or(""), amount);
                let leg = self.legs.entry(leg).or_default();
                leg.buckets[b].rows += 1;
                leg.buckets[b].usd = leg.buckets[b].usd + amount;
            }
            _ => {}
        }
    }

    fn report(&self) -> Value {
        let mut list: Vec<Value> = Vec::new();
        for (leg, st) in &self.legs {
            let buckets: Vec<Value> = st
                .buckets
                .iter()
                .enumerate()
                .map(|(i, b)| {
                    Value::Obj(vec![
                        ("bucket".to_string(), Value::Str(NAMES[i].to_string())),
                        ("rows".to_string(), Value::Num(b.rows.to_string())),
                        ("usd".to_string(), Value::num(&b.usd)),
                    ])
                })
                .collect();
            list.push(Value::Obj(vec![
                ("account".to_string(), Value::Str(leg.clone())),
                ("buckets".to_string(), Value::Arr(buckets)),
                (
                    "non_usd_rows".to_string(),
                    Value::Num(st.non_usd.to_string()),
                ),
            ]));
        }
        let members_without_interest: Vec<Value> = self
            .included
            .iter()
            .filter(|a| {
                !self.stated.contains(*a)
                    && self
                        .legs
                        .get(a.as_str())
                        .map(|l| l.buckets.iter().all(|b| b.rows == 0))
                        .unwrap_or(true)
            })
            .map(|a| Value::Str(a.clone()))
            .collect();
        Value::Obj(vec![
            ("legs".to_string(), Value::Arr(list)),
            (
                "members_without_interest".to_string(),
                Value::Arr(members_without_interest),
            ),
            (
                "notes".to_string(),
                Value::Arr(vec![
                    Value::Str("description decides the bucket, not sign: debit-descript reversals stay on the debit side".to_string()),
                    Value::Str("a member with no interest rows books no funding cost: its raw dollar ledger is overstated by the collateral return booked elsewhere in the family".to_string()),
                    Value::Str("the corrective charge (average negative balance x family debit rate) is not computed: interest rows carry amounts, not rates".to_string()),
                ]),
            ),
        ])
    }
}
