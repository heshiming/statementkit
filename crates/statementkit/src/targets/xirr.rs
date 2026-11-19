//! `xirr`: money-weighted return per statement period.
//!
//! IB publishes no money-weighted figure; this is constructed from
//! dated `Deposits & Withdrawals` rows (settle date + amount) when
//! present, else the USD net `Deposits & Withdrawals` key treated as
//! one flow at the period midpoint. Endpoints are `Net Asset Value`
//! table-1 `Prior Total` / `Current Total`. Income stays inside the
//! portfolio value: dividends and interest are NOT flows.
//!
//! The period rate r solves `prior*(1+r) + sum(f_i*(1+r)^e_i) =
//! current` by bisection (e_i = days from settle date to period end
//! over period days), annualized `(1+r)^(365/D)-1`. Alongside, the
//! same period P&L over three denominators: start (TWR-style),
//! end (ROI-style), days-weighted average investment (ROAI-style).
//! Ratios are f64; money stays exact decimal.

use crate::dates;
use crate::dec::Dec;
use crate::json::{self, Value};
use crate::target::{RowRef, Target};

const NAV_SECTION: &str = "Change in Combined NAV";

#[derive(Clone)]
struct Flow {
    date: String,
    amount: Dec,
}

#[derive(Default)]
struct File {
    period: String,
    base_ccy: String,
    prior_total: Option<Dec>,
    current_total: Option<Dec>,
    key_flows: Dec,
    has_key: bool,
    dated: Vec<Flow>,
    skipped_ccy: usize,
}

pub struct Xirr {
    file: File,
    records: Vec<Value>,
    no_solve: usize,
    net_mismatch: usize,
    outside: usize,
    skipped_ccy: usize,
}

impl Xirr {
    pub fn new() -> Self {
        Xirr {
            file: File::default(),
            records: Vec::new(),
            no_solve: 0,
            net_mismatch: 0,
            outside: 0,
            skipped_ccy: 0,
        }
    }

    fn finish_file(&mut self) {
        let f = std::mem::take(&mut self.file);
        let (Some(prior), Some(current)) = (f.prior_total, f.current_total) else {
            return; // no NAV table 1 Total
        };
        let Some((Some(from), Some(to), _)) = dates::stated_period(&f.period) else {
            return;
        };
        let (Some(d0), Some(d1)) = (dates::iso_to_days(&from), dates::iso_to_days(&to)) else {
            return;
        };
        let days = (d1 - d0) as f64;
        if days <= 0.0 {
            return;
        }
        self.skipped_ccy += f.skipped_ccy;

        // flow leg: dated rows when present, else the net key at the
        // midpoint; the USD key is the control total for either leg.
        let dated: Vec<&Flow> = f
            .dated
            .iter()
            .filter(|fl| {
                let keep = dates::iso_to_days(&fl.date).is_some_and(|d| d > d0 && d <= d1);
                if !keep {
                    self.outside += 1;
                }
                keep
            })
            .collect();
        let mut used_dated = false;
        let mut used_key = false;
        let flows: Vec<Flow> = if !dated.is_empty() {
            used_dated = true;
            dated.into_iter().cloned().collect()
        } else if f.has_key && f.key_flows != Dec::ZERO {
            used_key = true;
            let mid = dates::days_to_iso(d0 + (days / 2.0).round() as i64);
            vec![Flow {
                date: mid,
                amount: f.key_flows,
            }]
        } else {
            Vec::new()
        };
        let source = if used_dated {
            "dated"
        } else if used_key {
            "net-key"
        } else {
            "none"
        };
        if used_dated && f.has_key {
            let sum = flows.iter().fold(Dec::ZERO, |a, b| a + b.amount);
            if sum != f.key_flows {
                self.net_mismatch += 1;
            }
        }

        let net = flows.iter().fold(Dec::ZERO, |a, b| a + b.amount);
        let pnl = current - prior - net;
        // without a dated flow the money-weighted solve is meaningless
        let period_r = if !flows.is_empty() && prior > Dec::ZERO && current > Dec::ZERO {
            let exps: Vec<f64> = flows
                .iter()
                .filter_map(|fl| dates::iso_to_days(&fl.date))
                .map(|d| (d1 - d) as f64 / days)
                .collect();
            let amts: Vec<f64> = flows.iter().map(|fl| fl.amount.to_f64()).collect();
            solve(prior.to_f64(), current.to_f64(), &amts, &exps)
        } else {
            None
        };
        let annualized = period_r
            .filter(|r| *r > -1.0)
            .map(|r| (1.0 + r).powf(365.25 / days) - 1.0);
        if period_r.is_none() && prior > Dec::ZERO && current > Dec::ZERO && source != "none" {
            self.no_solve += 1;
        }
        let pnl_f = pnl.to_f64();
        let weighted_avg = {
            let mut acc = prior.to_f64() * days;
            for fl in &flows {
                if let Some(d) = dates::iso_to_days(&fl.date) {
                    acc += fl.amount.to_f64() * (d1 - d).max(0) as f64;
                }
            }
            acc / days
        };
        let mut members = vec![
            ("period".to_string(), Value::Str(f.period.clone())),
            ("period_from".to_string(), Value::Str(from.clone())),
            ("period_to".to_string(), Value::Str(to.clone())),
            ("prior_total".to_string(), Value::num(&prior)),
            ("current_total".to_string(), Value::num(&current)),
            ("flows_net".to_string(), Value::num(&net)),
            ("flow_source".to_string(), Value::Str(source.to_string())),
            (
                "flows".to_string(),
                Value::Arr(
                    flows
                        .iter()
                        .map(|fl| {
                            Value::Obj(vec![
                                ("date".to_string(), Value::Str(fl.date.clone())),
                                ("amount".to_string(), Value::num(&fl.amount)),
                            ])
                        })
                        .collect(),
                ),
            ),
            ("pnl".to_string(), Value::num(&pnl)),
            ("xirr_period".to_string(), json::ratio(period_r)),
            ("xirr_annualized".to_string(), json::ratio(annualized)),
        ];
        if source == "none" {
            members.push(("xirr_note".to_string(), Value::Str("no flow".to_string())));
        } else if period_r.is_none() {
            members.push(("xirr_note".to_string(), Value::Str("no solve".to_string())));
        }
        members.extend(vec![
            (
                "twr_style".to_string(),
                json::ratio(ratio(pnl_f, prior.to_f64())),
            ),
            (
                "roi_style".to_string(),
                json::ratio(ratio(pnl_f, current.to_f64())),
            ),
            (
                "roai_style".to_string(),
                json::ratio(ratio(pnl_f, weighted_avg)),
            ),
        ]);
        self.records.push(Value::Obj(members));
    }
}

fn ratio(num: f64, den: f64) -> Option<f64> {
    if den > 0.0 { Some(num / den) } else { None }
}

/// Bisection on f(r) = prior*(1+r) + sum(a_i*(1+r)^e_i) - current,
/// monotone increasing for prior > 0.
fn solve(prior: f64, current: f64, amts: &[f64], exps: &[f64]) -> Option<f64> {
    let f = |r: f64| {
        let base = 1.0 + r;
        let mut v = prior * base;
        for (a, e) in amts.iter().zip(exps) {
            v += a * base.powf(*e);
        }
        v - current
    };
    let mut lo = -0.999999_f64;
    let mut hi = 1_000_000.0_f64;
    if f(lo) > 0.0 || f(hi) < 0.0 {
        return None;
    }
    for _ in 0..200 {
        let mid = (lo + hi) / 2.0;
        if f(mid) < 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some((lo + hi) / 2.0)
}

impl Target for Xirr {
    fn name(&self) -> &'static str {
        "xirr"
    }

    fn on_file_begin(&mut self) {
        self.file = File::default();
    }

    fn on_meta(&mut self, key: &str, value: &str) {
        match key {
            "Period" => self.file.period = value.to_string(),
            "Currency" => self.file.base_ccy = value.to_string(),
            _ => {}
        }
    }

    fn on_row(&mut self, row: &RowRef) {
        match row.title {
            "Net Asset Value" => {
                if row.cell("Asset Class") != Some("Total") {
                    return;
                }
                if let Some(raw) = row.cell("Prior Total") {
                    self.file.prior_total = Dec::parse(raw).ok();
                }
                if let Some(raw) = row.cell("Current Total") {
                    self.file.current_total = Dec::parse(raw).ok();
                }
            }
            "Deposits & Withdrawals" => {
                let Some(raw) = row.cell("Amount") else {
                    return;
                };
                let Ok(v) = Dec::parse(raw) else {
                    return;
                };
                if row.cell("Currency").unwrap_or(&self.file.base_ccy) != self.file.base_ccy {
                    self.file.skipped_ccy += 1;
                    return;
                }
                let Some(raw_date) = row.cell("Settle Date") else {
                    return;
                };
                let Some(iso) = dates::stated_to_iso(raw_date) else {
                    return;
                };
                self.file.dated.push(Flow {
                    date: iso,
                    amount: v,
                });
            }
            NAV_SECTION => {
                if row.cell("Field") != Some("Deposits & Withdrawals") {
                    return;
                }
                if let Some(raw) = row.cell("Value") {
                    if let Ok(v) = Dec::parse(raw) {
                        self.file.key_flows = v;
                        self.file.has_key = true;
                    }
                }
            }
            _ => {}
        }
    }

    fn on_file_end(&mut self) {
        self.finish_file();
    }

    fn report(&self) -> Value {
        let mut notes = vec![Value::Str(
            "income (dividends, interest) stays inside the portfolio value; only deposits and withdrawals are flows"
                .to_string(),
        )];
        notes.push(Value::Str(
            "without dated flow rows the period's net USD D&W key is one flow at the period midpoint"
                .to_string(),
        ));
        if self.no_solve > 0 {
            notes.push(Value::Str(format!(
                "periods whose bracket [−0.999999, 1e6] found no sign change: {}",
                self.no_solve
            )));
        }
        if self.net_mismatch > 0 {
            notes.push(Value::Str(format!(
                "files where dated flows do not sum to the USD net key: {}",
                self.net_mismatch
            )));
        }
        if self.outside > 0 {
            notes.push(Value::Str(format!(
                "dated flows outside the statement period, dropped: {}",
                self.outside
            )));
        }
        if self.skipped_ccy > 0 {
            notes.push(Value::Str(format!(
                "dated flows in a currency other than the base, dropped: {}",
                self.skipped_ccy
            )));
        }
        Value::Obj(vec![
            ("records".to_string(), Value::Arr(self.records.clone())),
            ("notes".to_string(), Value::Arr(notes)),
        ])
    }
}
