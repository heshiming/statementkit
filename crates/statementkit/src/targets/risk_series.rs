//! `risk-series`: equity curve per grain with volatility and drawdown.
//!
//! One point per statement file: equity = `Ending Value` minus
//! `Deposits & Withdrawals`, so funded days are de-flowed before the
//! walk. Points group by grain (single day vs period range); grains
//! never mix. Volatility = sample stdev of simple returns annualized
//! by sqrt(periods per year): 252 for daily, mean calendar spacing
//! otherwise. Drawdown walks the same curve: peak tracking, trough =
//! min ratio, recovery = first point back at the peak. Ratios are
//! f64; equity stays exact decimal.

use std::collections::BTreeMap;

use crate::dates;
use crate::dec::Dec;
use crate::json::Value;
use crate::target::{RowRef, Target};

const SECTION: &str = "Change in Combined NAV";

#[derive(Clone)]
struct Point {
    date: String,
    equity: Dec,
}

pub struct RiskSeries {
    period: String,
    ending: Option<Dec>,
    flows: Dec,
    points: BTreeMap<&'static str, Vec<Point>>,
    skipped: usize,
}

impl RiskSeries {
    pub fn new() -> Self {
        RiskSeries {
            period: String::new(),
            ending: None,
            flows: Dec::ZERO,
            points: BTreeMap::new(),
            skipped: 0,
        }
    }
}

impl Target for RiskSeries {
    fn name(&self) -> &'static str {
        "risk-series"
    }

    fn on_file_begin(&mut self) {
        self.period.clear();
        self.ending = None;
        self.flows = Dec::ZERO;
    }

    fn on_meta(&mut self, key: &str, value: &str) {
        if key == "Period" {
            self.period = value.to_string();
        }
    }

    fn on_row(&mut self, row: &RowRef) {
        if row.title != SECTION {
            return;
        }
        let Some(field) = row.cell("Field") else {
            return;
        };
        if field != "Ending Value" && field != "Deposits & Withdrawals" {
            return;
        }
        let Some(raw) = row.cell("Value") else {
            return;
        };
        let Ok(v) = Dec::parse(raw) else {
            self.skipped += 1;
            return;
        };
        if field == "Ending Value" {
            self.ending = Some(v);
        } else {
            self.flows = v;
        }
    }

    fn on_file_end(&mut self) {
        let Some(ending) = self.ending else {
            self.skipped += 1;
            return;
        };
        let date = match dates::stated_period(&self.period) {
            Some((Some(d), _, false)) => ("day", d),
            Some((_, Some(t), true)) => ("range", t),
            _ => {
                self.skipped += 1;
                return;
            }
        };
        let equity = ending - self.flows;
        if equity <= Dec::ZERO {
            self.skipped += 1;
            return;
        }
        let (grain, date) = date;
        self.points
            .entry(grain)
            .or_default()
            .push(Point { date, equity });
    }

    fn report(&self) -> Value {
        let mut series = Vec::new();
        let mut duplicates = 0usize;
        let mut notes = vec![Value::Str(
            "volatility and drawdown are f64 ratios rounded to 6 decimals".to_string(),
        )];
        for (grain, raw) in &self.points {
            let pts = sorted_dedup(raw);
            duplicates += raw.len() - pts.len();
            let vol = volatility(&pts, annual_freq(grain, &pts));
            let dd = drawdown(&pts);
            let mut members = vec![
                ("grain".to_string(), Value::Str(grain.to_string())),
                ("points".to_string(), Value::Num(pts.len().to_string())),
                ("from".to_string(), Value::Str(pts[0].date.clone())),
                (
                    "to".to_string(),
                    Value::Str(pts[pts.len() - 1].date.clone()),
                ),
                (
                    "equity".to_string(),
                    Value::Arr(
                        pts.iter()
                            .map(|p| {
                                Value::Obj(vec![
                                    ("date".to_string(), Value::Str(p.date.clone())),
                                    ("equity".to_string(), Value::num(&p.equity)),
                                ])
                            })
                            .collect(),
                    ),
                ),
                ("volatility_annualized".to_string(), crate::json::ratio(vol)),
                ("max_drawdown".to_string(), crate::json::ratio(dd.ratio)),
                (
                    "peak_date".to_string(),
                    match &dd.peak_date {
                        Some(d) => Value::Str(d.clone()),
                        None => Value::Null,
                    },
                ),
                (
                    "trough_date".to_string(),
                    match &dd.trough_date {
                        Some(d) => Value::Str(d.clone()),
                        None => Value::Null,
                    },
                ),
                (
                    "recovery_date".to_string(),
                    match &dd.recovery_date {
                        Some(d) => Value::Str(d.clone()),
                        None => Value::Null,
                    },
                ),
                (
                    "drawdown_duration_days".to_string(),
                    match dd.duration_days {
                        Some(d) => Value::Num(d.to_string()),
                        None => Value::Null,
                    },
                ),
            ];
            if *grain == "range" {
                members.push((
                    "note".to_string(),
                    Value::Str("range grain cannot see intra-period drawdown".to_string()),
                ));
            }
            series.push(Value::Obj(members));
        }
        if self.skipped > 0 {
            notes.push(Value::Str(format!(
                "files skipped (no Ending Value, bad Period, unparsable value, or non-positive equity): {}",
                self.skipped
            )));
        }
        if duplicates > 0 {
            notes.push(Value::Str(format!(
                "same-date points within a grain, last kept: {duplicates}"
            )));
        }
        Value::Obj(vec![
            ("series".to_string(), Value::Arr(series)),
            ("notes".to_string(), Value::Arr(notes)),
        ])
    }
}

/// ISO dates sort lexicographically; stable sort keeps file order for
/// equal dates, of which the last one is kept.
fn sorted_dedup(raw: &[Point]) -> Vec<Point> {
    let mut pts = raw.to_vec();
    pts.sort_by(|a, b| a.date.cmp(&b.date));
    let mut out: Vec<Point> = Vec::with_capacity(pts.len());
    for p in pts {
        if out.last().is_some_and(|q| q.date == p.date) {
            let n = out.len() - 1;
            out[n] = p;
        } else {
            out.push(p);
        }
    }
    out
}

/// Periods per year: 252 for daily points, else from the mean
/// calendar gap between consecutive point dates.
fn annual_freq(grain: &str, pts: &[Point]) -> f64 {
    if grain == "day" {
        return 252.0;
    }
    let gaps: Vec<i64> = pts
        .windows(2)
        .filter_map(|w| {
            let (a, b) = (
                dates::iso_to_days(&w[0].date)?,
                dates::iso_to_days(&w[1].date)?,
            );
            (b > a).then_some(b - a)
        })
        .collect();
    if gaps.is_empty() {
        return 1.0;
    }
    let mean = gaps.iter().sum::<i64>() as f64 / gaps.len() as f64;
    365.25 / mean
}

/// Sample stdev of simple returns times sqrt(freq); None below 2 returns.
fn volatility(pts: &[Point], freq: f64) -> Option<f64> {
    let rets: Vec<f64> = pts
        .windows(2)
        .map(|w| w[1].equity.to_f64() / w[0].equity.to_f64() - 1.0)
        .collect();
    if rets.len() < 2 {
        return None;
    }
    let mean = rets.iter().sum::<f64>() / rets.len() as f64;
    let var = rets.iter().map(|r| (r - mean) * (r - mean)).sum::<f64>() / (rets.len() - 1) as f64;
    Some(var.sqrt() * freq.sqrt())
}

struct Dd {
    ratio: Option<f64>,
    peak_date: Option<String>,
    trough_date: Option<String>,
    recovery_date: Option<String>,
    duration_days: Option<i64>,
}

/// Walk the curve: track the running peak; worst equity/peak - 1 is
/// the trough; recovery is the first later point back at that peak.
fn drawdown(pts: &[Point]) -> Dd {
    let empty = || Dd {
        ratio: None,
        peak_date: None,
        trough_date: None,
        recovery_date: None,
        duration_days: None,
    };
    if pts.len() < 2 {
        return empty();
    }
    let mut peak_idx = 0usize;
    let mut best: Option<(f64, usize, usize)> = None; // ratio, peak, trough
    for (i, p) in pts.iter().enumerate().skip(1) {
        if p.equity > pts[peak_idx].equity {
            peak_idx = i;
            continue;
        }
        let ratio = p.equity.to_f64() / pts[peak_idx].equity.to_f64() - 1.0;
        if best.is_none_or(|(b, _, _)| ratio < b) {
            best = Some((ratio, peak_idx, i));
        }
    }
    let Some((ratio, peak, trough)) = best else {
        return empty();
    };
    if ratio >= 0.0 {
        return empty();
    }
    let recovery = pts[trough + 1..]
        .iter()
        .find(|p| p.equity >= pts[peak].equity);
    let duration = recovery.map(|r| {
        dates::iso_to_days(&r.date).unwrap_or_default()
            - dates::iso_to_days(&pts[trough].date).unwrap_or_default()
    });
    Dd {
        ratio: Some(ratio),
        peak_date: Some(pts[peak].date.clone()),
        trough_date: Some(pts[trough].date.clone()),
        recovery_date: recovery.map(|r| r.date.clone()),
        duration_days: duration,
    }
}
