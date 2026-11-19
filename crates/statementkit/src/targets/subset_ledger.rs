//! `subset-ledger`: the dollar ledger with the root taking the
//! account's place.
//!
//! Per selected root, USD rows only: `Trades` `Realized P/L` +
//! `Comm/Fee` (commissions ride every fill; realized appears on
//! the closes), `Dividends` postings by their description ticker,
//! `Borrow Fee Details` by `Symbol`. Booking periods follow the
//! file's `Period`, not the fill stamp. Rows whose `Currency`
//! differs from the base are counted and dropped, never mixed.
//!
//! The metric set is the K-invariant one, for the same reason as
//! the per-leg ledger:
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
//! No subset NAV exists — the statements carry no ticker on any
//! return denominator — so nothing divides by capital: no
//! %-return, no subset TWR, no %-drawdown.
//!
//! Dividends are postings, not accruals. At subset grain a
//! printed 0-month is a fact (no row of that root in that
//! file). A root that prints nothing was either never traded or
//! is misspelled — the file's `Financial Instrument Information`
//! (`ticker-map` vocabulary) is how to tell.

use std::collections::BTreeMap;

use crate::dates;
use crate::dec::Dec;
use crate::json;
use crate::json::Value;
use crate::target::{RowRef, Target};
use crate::tickers::{root_up, ticker_prefix};

fn dec(raw: Option<&str>) -> Option<Dec> {
    raw.and_then(|r| Dec::parse(r).ok())
}

#[derive(Default)]
struct Buckets {
    realized: Dec,
    commissions: Dec,
    dividends: Dec,
    borrow_fee: Dec,
}

impl Buckets {
    fn total(&self) -> Dec {
        ((self.realized + self.commissions) + self.dividends) + self.borrow_fee
    }
}

/// (root, period_from, period_to) -> buckets.
type PeriodKey = (String, String, String);
type RootSeries = Vec<((String, String), Dec)>;

pub struct SubsetLedger {
    base_ccy: String,
    period: Option<(String, String)>,
    sums: BTreeMap<PeriodKey, Buckets>,
    navs: BTreeMap<String, Dec>,
    excluded: usize,
    unattributed: usize,
}

impl SubsetLedger {
    pub fn new() -> Self {
        SubsetLedger {
            base_ccy: String::new(),
            period: None,
            sums: BTreeMap::new(),
            navs: BTreeMap::new(),
            excluded: 0,
            unattributed: 0,
        }
    }

    fn add(&mut self, root: &str, f: impl FnOnce(&mut Buckets)) {
        let (from, to) = match &self.period {
            Some(p) => p.clone(),
            None => return,
        };
        f(self.sums.entry((root.to_string(), from, to)).or_default());
    }

    fn usd(&self, row: &RowRef) -> bool {
        match row.cell("Currency") {
            Some(c) => self.base_ccy.is_empty() || c == self.base_ccy,
            None => true,
        }
    }
}

impl Target for SubsetLedger {
    fn name(&self) -> &'static str {
        "subset-ledger"
    }

    fn on_file_begin(&mut self) {
        self.period = None;
    }

    fn on_meta(&mut self, key: &str, value: &str) {
        match key {
            "Period" => {
                self.period = dates::stated_period(value).and_then(|(f, t, _)| Some((f?, t?)));
            }
            "Currency" => self.base_ccy = value.trim().to_string(),
            _ => {}
        }
    }

    fn on_row(&mut self, row: &RowRef) {
        if matches!(row.title, "Change in NAV" | "Change in Combined NAV")
            && row.cell("Field") == Some("Ending Value")
        {
            if let (Some((_, to)), Some(v)) = (&self.period, dec(row.cell("Value"))) {
                self.navs.insert(to.clone(), v);
            }
            return;
        }
        let root: Option<String> = match row.title {
            "Trades" => {
                let sym = row.cell("Symbol").unwrap_or("").trim();
                if sym.is_empty() {
                    None
                } else {
                    Some(root_up(sym))
                }
            }
            "Dividends" => row.cell("Description").and_then(ticker_prefix).or_else(|| {
                row.cell("Symbol")
                    .filter(|s| !s.trim().is_empty())
                    .map(root_up)
            }),
            "Borrow Fee Details" => {
                let sym = row.cell("Symbol").unwrap_or("").trim();
                if sym.is_empty() {
                    None
                } else {
                    Some(root_up(sym))
                }
            }
            _ => return,
        };
        let Some(root) = root else {
            if self.period.is_some() {
                self.unattributed += 1;
            }
            return;
        };
        if !self.usd(row) {
            self.excluded += 1;
            return;
        }
        match row.title {
            "Trades" => {
                let r = dec(row.cell("Realized P/L")).unwrap_or(Dec::ZERO);
                let c = dec(row.cell("Comm/Fee")).unwrap_or(Dec::ZERO);
                if row.cell("Realized P/L").is_none() && row.cell("Comm/Fee").is_none() {
                    return;
                }
                self.add(&root, |b| {
                    b.realized = b.realized + r;
                    b.commissions = b.commissions + c;
                });
            }
            "Dividends" => {
                if let Some(v) = dec(row.cell("Amount")) {
                    self.add(&root, |b| b.dividends = b.dividends + v);
                }
            }
            "Borrow Fee Details" => {
                if let Some(v) = dec(row.cell("Borrow Fee")) {
                    self.add(&root, |b| b.borrow_fee = b.borrow_fee + v);
                }
            }
            _ => {}
        }
    }

    fn report(&self) -> Value {
        // root -> period series
        let mut roots: BTreeMap<String, RootSeries> = BTreeMap::new();
        // period -> subset total
        let mut periods: BTreeMap<(String, String), Dec> = BTreeMap::new();
        for ((root, from, to), b) in &self.sums {
            let t = b.total();
            roots
                .entry(root.clone())
                .or_default()
                .push(((from.clone(), to.clone()), t));
            let e = periods.entry((from.clone(), to.clone())).or_default();
            *e = *e + t;
        }
        let mut root_list: Vec<Value> = Vec::new();
        for (root, series) in &roots {
            let (realized, commissions, dividends, borrow_fee): (Dec, Dec, Dec, Dec) =
                self.sums.iter().filter(|((r, _, _), _)| r == root).fold(
                    (Dec::ZERO, Dec::ZERO, Dec::ZERO, Dec::ZERO),
                    |mut acc, (_, b)| {
                        acc.0 = acc.0 + b.realized;
                        acc.1 = acc.1 + b.commissions;
                        acc.2 = acc.2 + b.dividends;
                        acc.3 = acc.3 + b.borrow_fee;
                        acc
                    },
                );
            let pnls: Vec<f64> = series.iter().map(|(_, p)| p.to_f64()).collect();
            let total = ((realized + commissions) + dividends) + borrow_fee;
            root_list.push(Value::Obj(vec![
                ("root".to_string(), Value::Str(root.clone())),
                ("realized".to_string(), Value::num(&realized)),
                ("commissions".to_string(), Value::num(&commissions)),
                ("dividends".to_string(), Value::num(&dividends)),
                ("borrow_fee".to_string(), Value::num(&borrow_fee)),
                ("total".to_string(), Value::num(&total)),
                ("metrics".to_string(), metrics(&pnls, &span_days(series))),
            ]));
        }
        let period_v: Vec<Value> = periods
            .iter()
            .map(|((from, to), t)| {
                Value::Obj(vec![
                    ("period_from".to_string(), Value::Str(from.clone())),
                    ("period_to".to_string(), Value::Str(to.clone())),
                    ("total".to_string(), Value::num(t)),
                ])
            })
            .collect();
        let subset_pnls: Vec<f64> = periods.values().map(|t| t.to_f64()).collect();
        let subset_spans: Vec<i64> = periods
            .keys()
            .filter_map(|(f, _)| dates::iso_to_days(f))
            .collect::<Vec<i64>>()
            .windows(2)
            .map(|w| (w[1] - w[0]).max(1))
            .collect();
        let grand: Dec = periods.values().fold(Dec::ZERO, |a, b| a + *b);
        let bases = declared_bases(&subset_pnls, &subset_spans, &self.navs);
        Value::Obj(vec![
            ("roots".to_string(), Value::Arr(root_list)),
            ("subset_periods".to_string(), Value::Arr(period_v)),
            (
                "subset_metrics".to_string(),
                metrics(&subset_pnls, &subset_spans),
            ),
            (
                "capital_bases".to_string(),
                Value::Arr(bases),
            ),
            ("grand_total".to_string(), Value::num(&grand)),
            (
                "non_usd_rows".to_string(),
                Value::Num(self.excluded.to_string()),
            ),
            (
                "unattributed_rows".to_string(),
                Value::Num(self.unattributed.to_string()),
            ),
            (
                "notes".to_string(),
                Value::Arr(vec![
                    Value::Str("no subset NAV exists: nothing here divides by capital — no %-return, no subset TWR, no %-drawdown".to_string()),
                    Value::Str("booking period is the file's Period, not the fill stamp".to_string()),
                    Value::Str("dividends are postings by description ticker; accruals stay out".to_string()),
                    Value::Str("a printed 0-month is a fact: no row of that root in that file".to_string()),
                    Value::Str("a root that prints nothing was never traded or is misspelled: check the ticker-map vocabulary".to_string()),
                    Value::Str("capital_bases levels are cumulated % of the declared K, never CAGR; K is constant and never compounded; Sharpe/Recovery/PF are base-free and identical across every K".to_string()),
                ]),
            ),
        ])
    }
}

/// Declared-K percent disclosure on the subset stream: notional
/// conventions plus the real average family NAV. Levels are
/// cumulated % of K, never CAGR; the ratios do not move.
fn declared_bases(pnls: &[f64], spans: &[i64], navs: &BTreeMap<String, Dec>) -> Vec<Value> {
    if pnls.is_empty() {
        return Vec::new();
    }
    let total: f64 = pnls.iter().sum();
    let mean = total / pnls.len() as f64;
    let var = pnls.iter().map(|p| (p - mean) * (p - mean)).sum::<f64>() / pnls.len() as f64;
    let sd = var.sqrt();
    let pp_year = if spans.is_empty() {
        None
    } else {
        let mut g = spans.to_vec();
        g.sort_unstable();
        Some(365.25 / g[g.len() / 2] as f64)
    };
    let num = |v: f64| Value::Num(json::ratio_text(v));
    let levels = |kind: &str, k: f64, extra: Vec<(String, Value)>| {
        let (ann_return, ann_vol) = match pp_year {
            Some(pp) if k > 0.0 => (Some(mean / k * pp), Some(sd * pp.sqrt() / k)),
            _ => (None, None),
        };
        let mut obj = vec![
            ("kind".to_string(), Value::Str(kind.to_string())),
            (
                "k".to_string(),
                Value::Num(if k == k.round() {
                    (k as i64).to_string()
                } else {
                    json::ratio_text(k)
                }),
            ),
            (
                "ann_return".to_string(),
                ann_return.map_or(Value::Null, num),
            ),
            ("ann_vol".to_string(), ann_vol.map_or(Value::Null, num)),
            (
                "cum_pct".to_string(),
                if k > 0.0 {
                    Value::Num(json::ratio_text(total / k))
                } else {
                    Value::Null
                },
            ),
        ];
        obj.extend(extra);
        Value::Obj(obj)
    };
    let mut out = Vec::new();
    for k in [100_000.0f64, 500_000.0, 2_000_000.0] {
        out.push(levels("notional", k, Vec::new()));
    }
    let vals: Vec<f64> = navs.values().map(|v| v.to_f64()).collect();
    if !vals.is_empty() {
        let avg = vals.iter().sum::<f64>() / vals.len() as f64;
        out.push(levels(
            "family-nav-avg",
            avg,
            vec![
                (
                    "nav_min".to_string(),
                    num(vals.iter().cloned().fold(f64::INFINITY, f64::min)),
                ),
                (
                    "nav_max".to_string(),
                    num(vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max)),
                ),
            ],
        ));
    }
    out
}

fn span_days(series: &[((String, String), Dec)]) -> Vec<i64> {
    series
        .iter()
        .filter_map(|((f, _), _)| dates::iso_to_days(f))
        .collect::<Vec<i64>>()
        .windows(2)
        .map(|w| (w[1] - w[0]).max(1))
        .collect()
}

/// The K-invariant metric set over a per-period dollar series.
fn metrics(pnls: &[f64], spans: &[i64]) -> Value {
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
    let pp_year = if spans.is_empty() {
        None
    } else {
        let mut g = spans.to_vec();
        g.sort_unstable();
        Some(365.25 / g[g.len() / 2] as f64)
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
    ])
}
