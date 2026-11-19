//! `subaccount-ledger`: dollars per attributed account, before
//! any convention.
//!
//! Sums `Account`-tagged USD rows per leg per file period:
//! `Trades` `Realized P/L` + `Comm/Fee`, `Dividends` `Amount`,
//! `Interest` `Amount`, `Borrow Fee Details` `Borrow Fee`. Rows
//! whose `Currency` differs from the base currency are counted
//! and dropped, never mixed. Files sharing one (leg, period) key
//! overwrite instead of double-adding.
//!
//! Then the metrics that never divide by capital:
//!
//! ```text
//! $MaxDD   = min over t of (cum_t - max_{s<=t} cum_s)
//! Sharpe   = mean(PnL_t) / sd(PnL_t) x sqrt(periods/year),
//!            population sd; annualized by the median spacing
//!            of the period series
//! Recovery = sum PnL / |$MaxDD|
//! PF       = sum PnL_t>0 / |sum PnL_t<0|
//! ```
//!
//! Sharpe here IS the ratio, not a proxy: for any constant
//! capital K, mean(PnL/K)/sd(PnL/K) = mean/sd — K cancels. What
//! dies without a base is only the level (%-return, %-vol).
//!
//! The ledger is realized + carry: `Open Positions` has no
//! `Account` column, so open marks are unattributable and across
//! window ends the ledger misses the unrealized delta — reconcile
//! with `Change in Combined NAV` before labelling it P&L.
//! Dividend accruals stay out (postings only). `Deposits &
//! Withdrawals` never names a member, so "cumulative ledger =
//! sub's balance" holds only if its master never funded it.

use std::collections::BTreeMap;

use crate::dates;
use crate::dec::Dec;
use crate::json;
use crate::json::Value;
use crate::target::{RowRef, Target};

fn bare_id(raw: &str) -> String {
    raw.split_whitespace()
        .next()
        .unwrap_or(raw)
        .trim_end_matches(',')
        .to_string()
}

fn dec(raw: Option<&str>) -> Option<Dec> {
    raw.and_then(|r| Dec::parse(r).ok())
}

type DayKey = (String, String, String);
type Series = Vec<((String, String), Dec)>;

pub struct SubaccountLedger {
    base_ccy: String,
    period: Option<(String, String)>,
    file_account: Option<String>,
    sums: BTreeMap<DayKey, Dec>,
    excluded: BTreeMap<String, BTreeMap<String, usize>>,
}

impl SubaccountLedger {
    pub fn new() -> Self {
        SubaccountLedger {
            base_ccy: String::new(),
            period: None,
            file_account: None,
            sums: BTreeMap::new(),
            excluded: BTreeMap::new(),
        }
    }
}

impl SubaccountLedger {
    fn add(&mut self, leg: &str, pnl: Dec) {
        let (from, to) = match &self.period {
            Some(p) => p.clone(),
            None => return,
        };
        let e = self.sums.entry((leg.to_string(), from, to)).or_default();
        *e = *e + pnl;
    }

    fn exclude(&mut self, leg: &str, section: &str) {
        *self
            .excluded
            .entry(leg.to_string())
            .or_default()
            .entry(format!("{section}: non-USD"))
            .or_default() += 1;
    }
}

impl Target for SubaccountLedger {
    fn name(&self) -> &'static str {
        "subaccount-ledger"
    }

    fn on_file_begin(&mut self) {
        self.period = None;
        self.file_account = None;
    }

    fn on_meta(&mut self, key: &str, value: &str) {
        match key {
            "Period" => {
                self.period = dates::stated_period(value).and_then(|(f, t, _)| Some((f?, t?)));
            }
            "Currency" => self.base_ccy = value.trim().to_string(),
            "Account ID" => self.file_account = Some(bare_id(value)),
            _ => {}
        }
    }

    fn on_row(&mut self, row: &RowRef) {
        let pnl = match row.title {
            "Trades" => {
                let r = dec(row.cell("Realized P/L")).unwrap_or(Dec::ZERO);
                let c = dec(row.cell("Comm/Fee")).unwrap_or(Dec::ZERO);
                if row.cell("Realized P/L").is_none() && row.cell("Comm/Fee").is_none() {
                    return;
                }
                r + c
            }
            "Dividends" | "Interest" => match dec(row.cell("Amount")) {
                Some(v) => v,
                None => return,
            },
            "Borrow Fee Details" => match dec(row.cell("Borrow Fee")) {
                Some(v) => v,
                None => return,
            },
            _ => return,
        };
        let leg = row
            .cell("Account")
            .map(bare_id)
            .or_else(|| self.file_account.clone())
            .unwrap_or_else(|| "unknown".to_string());
        if let Some(ccy) = row.cell("Currency") {
            if !self.base_ccy.is_empty() && ccy != self.base_ccy {
                self.exclude(&leg, row.title);
                return;
            }
        }
        self.add(&leg, pnl);
    }

    fn report(&self) -> Value {
        let mut legs: BTreeMap<String, Series> = BTreeMap::new();
        for (leg, from, to) in self
            .sums
            .keys()
            .map(|(leg, from, to)| (leg.clone(), from.clone(), to.clone()))
        {
            let pnl = self.sums[&(leg.clone(), from.clone(), to.clone())];
            legs.entry(leg).or_default().push(((from, to), pnl));
        }
        let mut list: Vec<Value> = Vec::new();
        for (leg, mut series) in legs {
            series.sort_by(|a, b| a.0.cmp(&b.0));
            let pnls: Vec<f64> = series.iter().map(|(_, p)| p.to_f64()).collect();
            let total: f64 = pnls.iter().sum();
            let mut cum = 0.0f64;
            let mut peak = 0.0f64;
            let mut max_dd = 0.0f64;
            for p in &pnls {
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
            let mean = total / n;
            let var = pnls.iter().map(|p| (p - mean) * (p - mean)).sum::<f64>() / n;
            let sd = var.sqrt();
            // median period spacing -> periods per year
            let mut gaps: Vec<i64> = Vec::new();
            for w in series.windows(2) {
                if let (Some(a), Some(b)) =
                    (dates::iso_to_days(&w[0].0.0), dates::iso_to_days(&w[1].0.0))
                {
                    gaps.push((b - a).max(1));
                }
            }
            let pp_year = if gaps.is_empty() {
                None
            } else {
                gaps.sort();
                let median = gaps[gaps.len() / 2] as f64;
                Some(365.25 / median)
            };
            let sharpe = match (sd > 0.0, pp_year) {
                (true, Some(pp)) => Some(mean / sd * pp.sqrt()),
                _ => None,
            };
            let recovery = if max_dd < 0.0 {
                Some(total / max_dd.abs())
            } else {
                None
            };
            let pf = if neg < 0.0 {
                Some(pos / neg.abs())
            } else {
                None
            };
            let series_v: Vec<Value> = series
                .iter()
                .map(|((from, to), pnl)| {
                    Value::Obj(vec![
                        ("period_from".to_string(), Value::Str(from.clone())),
                        ("period_to".to_string(), Value::Str(to.clone())),
                        ("pnl".to_string(), Value::num(pnl)),
                    ])
                })
                .collect();
            let excluded_v = self
                .excluded
                .get(&leg)
                .map(|m| {
                    Value::Arr(
                        m.iter()
                            .map(|(k, c)| {
                                Value::Obj(vec![
                                    ("reason".to_string(), Value::Str(k.clone())),
                                    ("rows".to_string(), Value::Num(c.to_string())),
                                ])
                            })
                            .collect(),
                    )
                })
                .unwrap_or_else(|| Value::Arr(Vec::new()));
            list.push(Value::Obj(vec![
                ("account".to_string(), Value::Str(leg)),
                ("series".to_string(), Value::Arr(series_v)),
                (
                    "metrics".to_string(),
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
                            "periods_per_year".to_string(),
                            pp_year.map_or(Value::Null, |p| Value::Num(json::ratio_text(p))),
                        ),
                        (
                            "recovery".to_string(),
                            recovery.map_or(Value::Null, |r| Value::Num(json::ratio_text(r))),
                        ),
                        (
                            "profit_factor".to_string(),
                            pf.map_or(Value::Null, |p| Value::Num(json::ratio_text(p))),
                        ),
                        ("periods".to_string(), Value::Num(pnls.len().to_string())),
                    ]),
                ),
                ("excluded_rows".to_string(), excluded_v),
            ]));
        }
        Value::Obj(vec![
            ("legs".to_string(), Value::Arr(list)),
            (
                "notes".to_string(),
                Value::Arr(vec![
                    Value::Str("realized + carry only: Open Positions has no Account column, open marks are unattributable; reconcile with Change in Combined NAV before labelling it P&L".to_string()),
                    Value::Str("Sharpe is the ratio itself, not a proxy: a constant capital base cancels; only %-level dies without one".to_string()),
                    Value::Str("postings only: dividend accruals stay out; non-USD rows counted and dropped, never mixed".to_string()),
                    Value::Str("Deposits & Withdrawals never name a member: cumulative ledger equals a sub's balance only if its master never funded it".to_string()),
                    Value::Str("population sd; annualized by the median spacing of the period series".to_string()),
                ]),
            ),
        ])
    }
}
