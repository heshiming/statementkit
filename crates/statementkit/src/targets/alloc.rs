//! `allocation`: allocation by account, currency, asset class.
//!
//! `Net Asset Value` table 1 IS the allocation: `Asset Class` rows with
//! `Current Long` / `Current Short` / `Current Total`, base currency,
//! already netted, `Total` row as denominator — zero derivation. The
//! `Current Long`/`Current Short` columns are optional (some templates
//! omit them; they report as null). Long/Short/Total report exact.
//! IB's class labels are authoritative but IB-specific (`Equity and
//! Index Options`, `Ideal Pro`, `Paxos`) and pass through verbatim.
//!
//! Per-account grouping uses the NAV rows' `Account` column when the
//! file carries one (consolidated files) and the file's `Account ID`
//! otherwise. The latest snapshot per account is reported ranked.
//!
//! Currency allocation converts `Open Positions` `Value` at the same
//! file's `Base Currency Exchange Rate` — that table ships in the
//! daily template only, so outside dailies currency shares come only
//! from base-currency rows and the gap is noted.

use std::collections::BTreeMap;

use crate::dates;
use crate::dec::Dec;
use crate::json;
use crate::json::Value;
use crate::target::{RowRef, Target};

#[derive(Default)]
struct ClassRow {
    long: Option<Dec>,
    short: Option<Dec>,
    total: Option<Dec>,
}

#[derive(Default)]
struct FileState {
    account: String,
    base: String,
    end: Option<String>,
    seq: usize,
    // account -> (class -> row, total)
    nav: BTreeMap<String, (BTreeMap<String, ClassRow>, Option<Dec>)>,
    pos: Vec<(Dec, String, String, bool)>, // value, currency, account, is_futures
    rates: BTreeMap<String, Dec>,
    has_positions: bool,
}

pub struct Allocation {
    files: Vec<FileState>,
    cur: FileState,
    seq: usize,
}

impl Allocation {
    pub fn new() -> Self {
        Allocation {
            files: Vec::new(),
            cur: FileState::default(),
            seq: 0,
        }
    }
}

fn dec(raw: Option<&str>) -> Option<Dec> {
    raw.and_then(|r| Dec::parse(r).ok())
}

/// (period_end, file seq, classes ranked by total, nav total)
type Latest = (String, usize, Vec<(String, Dec)>, Dec);

fn share(part: Dec, total: Dec) -> Value {
    json::ratio(if total > Dec::ZERO {
        Some(part.to_f64() / total.to_f64())
    } else {
        None
    })
}

impl Target for Allocation {
    fn name(&self) -> &'static str {
        "allocation"
    }

    fn on_meta(&mut self, key: &str, value: &str) {
        match key {
            "Account ID" => self.cur.account = value.to_string(),
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
            "Net Asset Value" => {
                let Some(class) = row.cell("Asset Class") else {
                    return;
                };
                if class.starts_with(' ') || class.is_empty() {
                    return; // indented component sub-rows
                }
                let acct = row.cell("Account").unwrap_or(&self.cur.account).to_string();
                let (entry, total) = self.cur.nav.entry(acct).or_default();
                if class.trim() == "Total" {
                    *total = dec(row.cell("Current Total"));
                    return;
                }
                let c = entry.entry(class.to_string()).or_default();
                c.long = dec(row.cell("Current Long"));
                c.short = dec(row.cell("Current Short"));
                c.total = dec(row.cell("Current Total"));
            }
            "Open Positions" => {
                let Some(value) = dec(row.cell("Value")) else {
                    return;
                };
                self.cur.has_positions = true;
                let ccy = row.cell("Currency").unwrap_or(&self.cur.base).to_string();
                let acct = row.cell("Account").unwrap_or(&self.cur.account).to_string();
                let futures = row.cell("Asset Category") == Some("Futures");
                self.cur.pos.push((value, ccy, acct, futures));
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
        self.cur.seq = self.seq;
        self.seq += 1;
        let done = std::mem::take(&mut self.cur);
        if !done.nav.is_empty() {
            self.files.push(done);
        }
    }

    fn report(&self) -> Value {
        let mut snaps: Vec<Value> = Vec::new();
        // account -> (end, seq, ranked classes, total)
        let mut latest: BTreeMap<String, Latest> = BTreeMap::new();
        let mut no_rate_excluded = 0usize;
        let mut rateless_files = 0usize;

        let mut files: Vec<&FileState> = self.files.iter().collect();
        files.sort_by_key(|f| f.seq);
        for f in files {
            let mut class_notes: Vec<Value> = Vec::new();
            for (acct, (classes, total)) in &f.nav {
                let total = match total {
                    Some(t) => *t,
                    None => {
                        class_notes.push(Value::Str(format!(
                            "account {acct}: no Total row, snapshot skipped"
                        )));
                        continue;
                    }
                };
                let mut ranked: Vec<(String, Dec)> = classes
                    .iter()
                    .map(|(k, v)| (k.clone(), v.total.unwrap_or(Dec::ZERO)))
                    .collect();
                ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
                let class_list: Vec<Value> = ranked
                    .iter()
                    .map(|(name, tot)| {
                        let c = &classes[name];
                        Value::Obj(vec![
                            ("class".to_string(), Value::Str(name.clone())),
                            (
                                "long".to_string(),
                                c.long.map(|v| Value::num(&v)).unwrap_or(Value::Null),
                            ),
                            (
                                "short".to_string(),
                                c.short.map(|v| Value::num(&v)).unwrap_or(Value::Null),
                            ),
                            ("total".to_string(), Value::num(tot)),
                            ("share".to_string(), share(*tot, total)),
                        ])
                    })
                    .collect();
                // currency shares from positions of this account in this file
                let mut by_ccy: BTreeMap<String, Dec> = BTreeMap::new();
                let mut mut_no_rate = 0usize;
                for (value, ccy, pacct, futures) in &f.pos {
                    if pacct != acct || *futures {
                        continue; // futures notional has no NAV slice to sit in
                    }
                    let usd = if *ccy == f.base {
                        Some(*value)
                    } else {
                        f.rates.get(ccy).map(|r| *value * *r)
                    };
                    match usd {
                        Some(u) => {
                            *by_ccy.entry(ccy.clone()).or_insert(Dec::ZERO) =
                                *by_ccy.entry(ccy.clone()).or_insert(Dec::ZERO) + u
                        }
                        None => {
                            mut_no_rate += 1;
                        }
                    }
                }
                no_rate_excluded += mut_no_rate;
                if mut_no_rate > 0 {
                    class_notes.push(Value::Str(format!(
                        "account {acct}: {mut_no_rate} position rows excluded from currency shares (no rate in file)"
                    )));
                }
                let ccy_list: Vec<Value> = by_ccy
                    .iter()
                    .map(|(ccy, u)| {
                        Value::Obj(vec![
                            ("currency".to_string(), Value::Str(ccy.clone())),
                            ("value_usd".to_string(), Value::num(u)),
                            ("share".to_string(), share(*u, total)),
                        ])
                    })
                    .collect();
                snaps.push(Value::Obj(vec![
                    ("account".to_string(), Value::Str(acct.clone())),
                    (
                        "period_end".to_string(),
                        match &f.end {
                            Some(d) => Value::Str(d.clone()),
                            None => Value::Null,
                        },
                    ),
                    ("nav_total".to_string(), Value::num(&total)),
                    ("classes".to_string(), Value::Arr(class_list)),
                    ("currency_shares".to_string(), Value::Arr(ccy_list)),
                ]));
                if total > Dec::ZERO {
                    if let Some(d) = &f.end {
                        let best = latest
                            .entry(acct.clone())
                            .or_insert_with(|| (d.clone(), f.seq, Vec::new(), total));
                        if (d.as_str(), f.seq) >= (best.0.as_str(), best.1) {
                            *best = (d.clone(), f.seq, ranked.clone(), total);
                        }
                    }
                }
            }
            if f.rates.is_empty() && f.has_positions {
                rateless_files += 1;
            }
            if !class_notes.is_empty() {
                snaps.push(Value::Obj(vec![
                    ("file_seq".to_string(), Value::Num(f.seq.to_string())),
                    ("notes".to_string(), Value::Arr(class_notes)),
                ]));
            }
        }
        let latest_list: Vec<Value> = latest
            .iter()
            .map(|(acct, (end, _, ranked, total))| {
                let top: Vec<Value> = ranked
                    .iter()
                    .take(2)
                    .map(|(name, tot)| {
                        Value::Obj(vec![
                            ("class".to_string(), Value::Str(name.clone())),
                            ("share".to_string(), share(*tot, *total)),
                        ])
                    })
                    .collect();
                Value::Obj(vec![
                    ("account".to_string(), Value::Str(acct.clone())),
                    ("period_end".to_string(), Value::Str(end.clone())),
                    ("nav_total".to_string(), Value::num(total)),
                    ("top_classes".to_string(), Value::Arr(top)),
                ])
            })
            .collect();
        let mut notes = vec![
            Value::Str("allocation reads NAV table 1 as-is; IB class labels pass through verbatim".to_string()),
            Value::Str("shares are null where NAV is zero or negative (share of a negative book is meaningless)".to_string()),
            Value::Str("currency shares need the same file's rate table (daily template only); unrateable rows are excluded and counted".to_string()),
        ];
        if no_rate_excluded > 0 {
            notes.push(Value::Str(format!(
                "position rows excluded from currency shares (no rate): {no_rate_excluded}"
            )));
        }
        if rateless_files > 0 {
            notes.push(Value::Str(format!(
                "files with positions but no rate table: {rateless_files} (currency shares base-currency only there)"
            )));
        }
        Value::Obj(vec![
            ("snapshots".to_string(), Value::Arr(snaps)),
            ("latest_by_account".to_string(), Value::Arr(latest_list)),
            ("notes".to_string(), Value::Arr(notes)),
        ])
    }
}
