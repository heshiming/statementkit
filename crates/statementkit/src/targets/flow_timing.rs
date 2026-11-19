//! `flow-timing`: did the owner's own flows help or hurt.
//!
//! Behavior gap = XIRR − chained TWR over the same window, on the
//! period-grain curve built from each file's `Change in NAV` /
//! `Change in Combined NAV` block (Starting/Beginning, Ending,
//! `Deposits & Withdrawals` USD key). Flows are priced at
//! period-end (the EAV block gives one number per period — exact
//! in-month dates exist only where dated `Deposits & Withdrawals`
//! rows carry them; those rows are listed beside the curve and
//! joined by month, never merged into it).
//!
//! Monthly return: r = (Ending − Flows) ÷ Beginning − 1 — flows are
//! cash legs, not income. XIRR: −Beginning at window start, each
//! period's flows at its end date, Ending at window end, bisection.
//!
//! Event study: every non-zero-flow period against the compounded
//! NAV drift of the ≤3 periods before and after, classified
//! bought/sold strength|weakness at ±2%. With a handful of flows
//! this is narrative, not statistics — the event list ships beside
//! the stat and `n` is disclosed.
//!
//! Multi-currency flow rows are converted at the statement's own
//! `Base Currency Exchange Rate`; unconvertable rows are counted,
//! never guessed. Intra-household transfers appear as flows in each
//! account's files — out of scope here and a real bug when accounts
//! are combined.

use std::collections::BTreeMap;

use crate::dates;
use crate::dec::Dec;
use crate::json;
use crate::json::Value;
use crate::target::{RowRef, Target};

#[derive(Default)]
struct Period {
    from: String,
    to: String,
    beginning: Option<Dec>,
    ending: Option<Dec>,
    flows: Option<Dec>,
}

#[derive(Default)]
struct FileRec {
    period: Option<(String, String)>,
    period_eav: Period,
    row_flows: Option<Dec>,
}

pub struct FlowTiming {
    periods: BTreeMap<(String, String), Period>,
    file: FileRec,
    dated_flows: Vec<(String, Dec)>, // (iso date, USD)
    rates_by_ccy: BTreeMap<String, Dec>,
    base_ccy: String,
    unresolved_ccy: usize,
    broken: usize,
}

impl FlowTiming {
    pub fn new() -> Self {
        FlowTiming {
            periods: BTreeMap::new(),
            file: FileRec::default(),
            dated_flows: Vec::new(),
            rates_by_ccy: BTreeMap::new(),
            base_ccy: String::new(),
            unresolved_ccy: 0,
            broken: 0,
        }
    }
}

fn dec(raw: Option<&str>) -> Option<Dec> {
    raw.and_then(|r| Dec::parse(r).ok())
}

fn eav_value<'a>(row: &RowRef<'a>) -> Option<(&'a str, Dec)> {
    let key = row.cell("Key").or_else(|| row.cell("Field"))?;
    let raw = row
        .cell("Value")
        .or_else(|| row.cell("1 Day Total"))
        .or_else(|| row.cell("Month to Date Total"))
        .or_else(|| row.cell("Year to Date Total"))?;
    Some((key, Dec::parse(raw).ok()?))
}

impl Target for FlowTiming {
    fn name(&self) -> &'static str {
        "flow-timing"
    }

    fn on_meta(&mut self, key: &str, value: &str) {
        match key {
            "Period" => {
                if let Some((f, t)) =
                    dates::stated_period(value).and_then(|(f, t, _)| Some((f?, t?)))
                {
                    self.file.period = Some((f, t));
                }
            }
            "Currency" => self.base_ccy = value.trim().to_string(),
            _ => {}
        }
    }

    fn on_file_end(&mut self) {
        let f = std::mem::take(&mut self.file);
        let Some((from, to)) = f.period else { return };
        let p = self.periods.entry((from.clone(), to.clone())).or_default();
        if p.from.is_empty() {
            p.from = from;
            p.to = to;
        }
        if p.beginning.is_none() {
            p.beginning = f.period_eav.beginning;
        }
        if p.ending.is_none() {
            p.ending = f.period_eav.ending;
        }
        if p.flows.is_none() {
            p.flows = f.period_eav.flows.or(f.row_flows);
        }
    }

    fn on_row(&mut self, row: &RowRef) {
        match row.title {
            "Change in NAV" | "Change in Combined NAV" => {
                let Some((key, v)) = eav_value(row) else {
                    return;
                };
                match key {
                    "Starting Value" | "Beginning Value" => {
                        self.file.period_eav.beginning = Some(v)
                    }
                    "Ending Value" => self.file.period_eav.ending = Some(v),
                    "Deposits & Withdrawals" => self.file.period_eav.flows = Some(v),
                    _ => {}
                }
            }
            "Base Currency Exchange Rate" => {
                if let (Some(ccy), Some(rate)) =
                    (row.cell("Currency"), dec(row.cell("Exchange Rate")))
                {
                    if ccy != self.base_ccy {
                        self.rates_by_ccy.insert(ccy.to_string(), rate);
                    }
                }
            }
            "Deposits & Withdrawals" => {
                let Some(amount) = dec(row.cell("Amount")) else {
                    return;
                };
                let ccy = row.cell("Currency").unwrap_or(&self.base_ccy).to_string();
                let usd = if ccy == self.base_ccy || self.base_ccy.is_empty() {
                    Some(amount)
                } else if let Some(rate) = self.rates_by_ccy.get(&ccy) {
                    Some(amount * *rate)
                } else {
                    self.unresolved_ccy += 1;
                    None
                };
                if let Some(u) = usd {
                    self.file.row_flows = Some(self.file.row_flows.unwrap_or(Dec::ZERO) + u);
                    if let Some(d) = row.cell("Settle Date").and_then(dates::date_to_days) {
                        self.dated_flows.push((dates::days_to_iso(d), u));
                    }
                }
            }
            _ => {}
        }
    }

    fn report(&self) -> Value {
        // returns per period, flow-adjusted, in date order
        let mut periods: Vec<&Period> = self.periods.values().collect();
        periods.sort_by_key(|p| (p.from.clone(), p.to.clone()));
        let mut rets: Vec<f64> = Vec::new();
        let mut broken = self.broken;
        for p in periods.iter() {
            let (Some(b), Some(e)) = (p.beginning, p.ending) else {
                continue;
            };
            if b <= Dec::ZERO {
                broken += 1;
                continue;
            }
            let flows = p.flows.unwrap_or(Dec::ZERO);
            let r = (e - flows).to_f64() / b.to_f64() - 1.0;
            rets.push(r);
        }
        let twr_total: f64 = rets.iter().fold(1.0, |a, r| a * (1.0 + r));
        let twr_total = twr_total - 1.0;
        let (from, to) = (
            periods.first().map(|p| p.from.clone()).unwrap_or_default(),
            periods.last().map(|p| p.to.clone()).unwrap_or_default(),
        );
        let days = match (dates::iso_to_days(&from), dates::iso_to_days(&to)) {
            (Some(a), Some(b)) => (b - a).max(1) as f64,
            _ => 0.0,
        };
        let years = days / 365.25;
        let twr_ann = if years > 0.0 && twr_total > -1.0 {
            (1.0 + twr_total).powf(1.0 / years) - 1.0
        } else {
            f64::NAN
        };
        // XIRR legs from the same curve
        let mut legs: Vec<(f64, f64)> = Vec::new();
        let t0 = dates::iso_to_days(&from).unwrap_or(0) as f64;
        if let Some(b) = periods.first().and_then(|p| p.beginning) {
            legs.push((0.0, -b.to_f64()));
        }
        for p in periods.iter() {
            let d = dates::iso_to_days(&p.to).unwrap_or(0) as f64 - t0;
            if let Some(fl) = p.flows {
                if fl != Dec::ZERO {
                    legs.push((d / 365.25, fl.to_f64()));
                }
            }
        }
        if let Some(e) = periods.last().and_then(|p| p.ending) {
            legs.push((years, e.to_f64()));
        }
        let xirr = xirr(&legs);
        let gap = match xirr {
            Some(x) if twr_ann.is_finite() => Some(x - twr_ann),
            _ => None,
        };
        // event study on the period-grain curve
        let mut events: Vec<Value> = Vec::new();
        let idx: Vec<&Period> = periods.clone();
        for (i, p) in idx.iter().enumerate() {
            let Some(fl) = p.flows else { continue };
            if fl == Dec::ZERO {
                continue;
            }
            let drift = |range: std::ops::Range<usize>| -> Option<f64> {
                let r: Vec<f64> = range.filter_map(|j| rets.get(j).copied()).collect();
                if r.is_empty() {
                    return None;
                }
                Some(r.iter().fold(1.0, |a, x| a * (1.0 + x)) - 1.0)
            };
            let prior = drift(i.saturating_sub(3)..i);
            let next = drift(i + 1..(i + 4).min(rets.len()));
            let class = match prior {
                Some(d) if d >= 0.02 && fl > Dec::ZERO => "bought-strength",
                Some(d) if d <= -0.02 && fl > Dec::ZERO => "bought-weakness",
                Some(d) if d >= 0.02 && fl < Dec::ZERO => "sold-strength",
                Some(d) if d <= -0.02 && fl < Dec::ZERO => "sold-weakness",
                Some(_) => "neutral-timing",
                None => "no-prior-window",
            };
            let nav = p.ending.unwrap_or(Dec::ZERO);
            let mut obj = vec![
                ("period_from".to_string(), Value::Str(p.from.clone())),
                ("period_to".to_string(), Value::Str(p.to.clone())),
                ("flow_usd".to_string(), Value::num(&fl)),
            ];
            if nav > Dec::ZERO {
                obj.push((
                    "flow_pct_of_nav".to_string(),
                    Value::Num(json::ratio_text(fl.to_f64() / nav.to_f64())),
                ));
            }
            obj.push((
                "prior_drift".to_string(),
                prior.map_or(Value::Null, |d| Value::Num(json::ratio_text(d))),
            ));
            obj.push((
                "next_drift".to_string(),
                next.map_or(Value::Null, |d| Value::Num(json::ratio_text(d))),
            ));
            obj.push(("class".to_string(), Value::Str(class.to_string())));
            events.push(Value::Obj(obj));
        }
        let dated: Vec<Value> = self
            .dated_flows
            .iter()
            .map(|(d, a)| {
                Value::Obj(vec![
                    ("date".to_string(), Value::Str(d.clone())),
                    ("amount_usd".to_string(), Value::num(a)),
                ])
            })
            .collect();
        let mut notes = vec![
            Value::Str("flows priced at period end: the EAV block is one number per period; dated rows are listed, not merged".to_string()),
            Value::Str("with a handful of flows the gap and drift columns are narrative, not statistics; n is disclosed".to_string()),
            Value::Str("intra-household transfers show as flows in each account file — combined-account use double-counts them".to_string()),
        ];
        if self.unresolved_ccy > 0 {
            notes.push(Value::Str(format!(
                "flow rows with no same-file exchange rate (excluded): {}",
                self.unresolved_ccy
            )));
        }
        if broken > 0 {
            notes.push(Value::Str(format!(
                "periods skipped for non-positive or missing NAV: {}",
                broken
            )));
        }
        Value::Obj(vec![
            (
                "window".to_string(),
                Value::Obj(vec![
                    ("from".to_string(), Value::Str(from)),
                    ("to".to_string(), Value::Str(to)),
                    ("periods".to_string(), Value::Num(periods.len().to_string())),
                ]),
            ),
            (
                "twr".to_string(),
                Value::Obj(vec![
                    ("total".to_string(), Value::Num(json::ratio_text(twr_total))),
                    (
                        "annualized".to_string(),
                        Value::Num(if twr_ann.is_finite() {
                            json::ratio_text(twr_ann)
                        } else {
                            "null".to_string()
                        }),
                    ),
                ]),
            ),
            (
                "mwr_xirr_annualized".to_string(),
                xirr.map_or(Value::Null, |x| Value::Num(json::ratio_text(x))),
            ),
            (
                "behavior_gap_pp_yr".to_string(),
                gap.map_or(Value::Null, |g| Value::Num(json::ratio_text(g))),
            ),
            (
                "flow_periods".to_string(),
                Value::Num(events.len().to_string()),
            ),
            ("events".to_string(), Value::Arr(events)),
            ("dated_flows".to_string(), Value::Arr(dated)),
            ("notes".to_string(), Value::Arr(notes)),
        ])
    }
}

/// Solve sum a_i * (1+r)^(T-t_i) = 0 by bisection on r.
fn xirr(legs: &[(f64, f64)]) -> Option<f64> {
    if legs.len() < 2 {
        return None;
    }
    if legs.iter().all(|(_, a)| *a >= 0.0) || legs.iter().all(|(_, a)| *a <= 0.0) {
        return None;
    }
    let f = |r: f64| -> f64 {
        let t_max = legs.iter().map(|(t, _)| *t).fold(0.0f64, f64::max);
        legs.iter()
            .map(|(t, a)| a * (1.0 + r).powf(t_max - t))
            .fold(0.0, |x, y| x + y)
    };
    let (mut lo, mut hi) = (-0.9999f64, 10.0f64);
    let (flo, fhi) = (f(lo), f(hi));
    if flo * fhi > 0.0 {
        return None;
    }
    for _ in 0..200 {
        let mid = (lo + hi) / 2.0;
        let fm = f(mid);
        if fm * flo <= 0.0 {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    Some((lo + hi) / 2.0)
}
