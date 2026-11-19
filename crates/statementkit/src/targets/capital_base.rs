//! `capital-base`: express the ledger in percent of a declared K.
//!
//! The dollar ledger (`Account`-tagged USD rows: Trades realized +
//! commission, Dividends/Interest amounts, Borrow Fee) per leg per
//! file period, then percent levels against disclosed bases:
//!
//! - `notional`: the managed-futures convention (100k / 500k / 2M),
//!   applied identically to every leg so %-levels are at least
//!   comparable among themselves;
//! - `family-nav`: the collateral actually standing there — K is the
//!   time-average of the file `Ending Value` NAVs; watch outlier
//!   months (one deposit can multiply the base and silently dilute
//!   the ratio — the base min/max is disclosed);
//! - `vol-target`: K = realized annual $-vol ÷ target-vol (15%),
//!   the capital whose risk budget equals the risk actually taken.
//!
//! K is held constant and never compounded: levels annualize
//! additively (`ann_return = mean(pnl)/K x periods/yr`) and the
//! level label is "cumulated % of notional capital", never CAGR.
//! Across every base Sharpe, Recovery and PF are identical — the
//! level is the convention, the ratios are the fact.
//!
//! Margin consumed — the cleanest economic base for a margin-funded
//! book — is invisible: no Flex export in this family carries any
//! margin field, so no base of that kind is offered.

use std::collections::BTreeMap;

use crate::dates;
use crate::dec::Dec;
use crate::json;
use crate::json::Value;
use crate::target::{RowRef, Target};

const NOTIONALS: [f64; 3] = [100_000.0, 500_000.0, 2_000_000.0];
const VOL_TARGET: f64 = 0.15;

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

pub struct CapitalBase {
    base_ccy: String,
    period: Option<(String, String)>,
    file_account: Option<String>,
    sums: BTreeMap<DayKey, Dec>,
    navs: BTreeMap<String, Dec>,
    excluded_non_usd: usize,
}

impl CapitalBase {
    pub fn new() -> Self {
        CapitalBase {
            base_ccy: String::new(),
            period: None,
            file_account: None,
            sums: BTreeMap::new(),
            navs: BTreeMap::new(),
            excluded_non_usd: 0,
        }
    }
}

impl Target for CapitalBase {
    fn name(&self) -> &'static str {
        "capital-base"
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
        if matches!(row.title, "Change in NAV" | "Change in Combined NAV") {
            let key = row.cell("Key").or_else(|| row.cell("Field"));
            if key == Some("Ending Value") {
                if let (Some(v), Some((_, to))) = (dec(row.cell("Value")), &self.period) {
                    self.navs.insert(to.clone(), v);
                }
            }
            return;
        }
        let pnl = match row.title {
            "Trades" => {
                if row.cell("Realized P/L").is_none() && row.cell("Comm/Fee").is_none() {
                    return;
                }
                let r = dec(row.cell("Realized P/L")).unwrap_or(Dec::ZERO);
                let c = dec(row.cell("Comm/Fee")).unwrap_or(Dec::ZERO);
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
        if let Some(ccy) = row.cell("Currency") {
            if !self.base_ccy.is_empty() && ccy != self.base_ccy {
                self.excluded_non_usd += 1;
                return;
            }
        }
        let leg = row
            .cell("Account")
            .map(bare_id)
            .or_else(|| self.file_account.clone())
            .unwrap_or_else(|| "unknown".to_string());
        let (from, to) = match &self.period {
            Some(p) => p.clone(),
            None => return,
        };
        let e = self.sums.entry((leg, from, to)).or_default();
        *e = *e + pnl;
    }

    fn report(&self) -> Value {
        let mut legs: BTreeMap<String, Series> = BTreeMap::new();
        for (leg, from, to) in self
            .sums
            .keys()
            .map(|(l, f, t)| (l.clone(), f.clone(), t.clone()))
        {
            let pnl = self.sums[&(leg.clone(), from.clone(), to.clone())];
            legs.entry(leg).or_default().push(((from, to), pnl));
        }
        let nav_vals: Vec<f64> = self.navs.values().map(|v| v.to_f64()).collect();
        let nav_mean = if nav_vals.is_empty() {
            None
        } else {
            Some(nav_vals.iter().sum::<f64>() / nav_vals.len() as f64)
        };
        let mut list: Vec<Value> = Vec::new();
        for (leg, mut series) in legs {
            series.sort_by(|a, b| a.0.cmp(&b.0));
            let pnls: Vec<f64> = series.iter().map(|(_, p)| p.to_f64()).collect();
            let total: f64 = pnls.iter().sum();
            let n = pnls.len() as f64;
            let mean = total / n;
            let var = pnls.iter().map(|p| (p - mean) * (p - mean)).sum::<f64>() / n;
            let sd = var.sqrt();
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
                Some(365.25 / gaps[gaps.len() / 2] as f64)
            };
            let mut bases: Vec<Value> = Vec::new();
            for k in NOTIONALS {
                bases.push(levels("notional", k, mean, sd, total, pp_year, Vec::new()));
            }
            if let Some(k) = nav_mean {
                if k > 0.0 {
                    let extra = vec![
                        ("nav_min".to_string(), min_v(&nav_vals)),
                        ("nav_max".to_string(), max_v(&nav_vals)),
                    ];
                    bases.push(levels("family-nav", k, mean, sd, total, pp_year, extra));
                }
            }
            if sd > 0.0 {
                if let Some(pp) = pp_year {
                    let ann_vol_d = sd * pp.sqrt();
                    let k = ann_vol_d / VOL_TARGET;
                    bases.push(levels(
                        "vol-target",
                        k,
                        mean,
                        sd,
                        total,
                        pp_year,
                        Vec::new(),
                    ));
                }
            }
            let sharpe = if sd > 0.0 {
                mean / sd * pp_year.unwrap_or(1.0).sqrt()
            } else {
                f64::NAN
            };
            let mut cum = 0.0f64;
            let mut peak = 0.0f64;
            let mut max_dd = 0.0f64;
            for p in &pnls {
                cum += p;
                peak = peak.max(cum);
                max_dd = max_dd.min(cum - peak);
            }
            let pos: f64 = pnls.iter().filter(|p| **p > 0.0).sum();
            let neg: f64 = pnls.iter().filter(|p| **p < 0.0).sum();
            list.push(Value::Obj(vec![
                ("account".to_string(), Value::Str(leg)),
                (
                    "ratios".to_string(),
                    Value::Obj(vec![
                        (
                            "sharpe_annualized".to_string(),
                            Value::Num(if sharpe.is_finite() {
                                json::ratio_text(sharpe)
                            } else {
                                "null".to_string()
                            }),
                        ),
                        (
                            "max_drawdown".to_string(),
                            Value::Num(json::ratio_text(max_dd)),
                        ),
                        (
                            "recovery".to_string(),
                            Value::Num(if max_dd < 0.0 {
                                json::ratio_text(total / max_dd.abs())
                            } else {
                                "null".to_string()
                            }),
                        ),
                        (
                            "profit_factor".to_string(),
                            Value::Num(if neg < 0.0 {
                                json::ratio_text(pos / neg.abs())
                            } else {
                                "null".to_string()
                            }),
                        ),
                    ]),
                ),
                ("bases".to_string(), Value::Arr(bases)),
            ]));
        }
        let mut notes = vec![
            Value::Str("K is constant and never compounded: levels annualize additively; the level label is cumulated % of notional capital, never CAGR".to_string()),
            Value::Str("across every base Sharpe, Recovery and PF are identical: the level is the convention, the ratios are the fact".to_string()),
            Value::Str("margin-consumed base not offered: no export in this statement family carries any margin field".to_string()),
        ];
        if self.excluded_non_usd > 0 {
            notes.push(Value::Str(format!(
                "non-USD ledger rows counted and dropped, never mixed: {}",
                self.excluded_non_usd
            )));
        }
        Value::Obj(vec![
            ("legs".to_string(), Value::Arr(list)),
            (
                "vol_target".to_string(),
                Value::Num(json::ratio_text(VOL_TARGET)),
            ),
            ("notes".to_string(), Value::Arr(notes)),
        ])
    }
}

fn num(x: f64) -> Value {
    Value::Num(json::ratio_text(x))
}

fn levels(
    kind: &str,
    k: f64,
    mean: f64,
    sd: f64,
    total: f64,
    pp_year: Option<f64>,
    extra: Vec<(String, Value)>,
) -> Value {
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
            Value::Num(if k > 0.0 {
                json::ratio_text(total / k)
            } else {
                "null".to_string()
            }),
        ),
    ];
    obj.extend(extra);
    Value::Obj(obj)
}

fn min_v(vals: &[f64]) -> Value {
    num(vals.iter().cloned().fold(f64::INFINITY, f64::min))
}

fn max_v(vals: &[f64]) -> Value {
    num(vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max))
}
