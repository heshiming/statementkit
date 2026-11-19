//! `twr`: IB's own time-weighted return per statement period.
//!
//! Every file can carry two `Net Asset Value` tables: table 1
//! (`Asset Class | Prior Total | Current Long | Current Short |
//! Current Total | Change`) and table 2 (`Asset Class | Time
//! Weighted Rate of Return`, same row order). The `Total` row is
//! the whole account. Where table 2 is absent, the fallback is the
//! NAV chain: (End - Start - Deposits & Withdrawals) / Start.
//! Periods chain multiplicatively; the TWR column is never averaged.

use crate::dates;
use crate::dec::Dec;
use crate::json::{self, Value};
use crate::target::{RowRef, Target};

const NAV_SECTION: &str = "Change in Combined NAV";
const TWR_FIELD: &str = "Time Weighted Rate of Return";

#[derive(Default)]
struct File {
    period: String,
    prior_total: Option<Dec>,
    current_total: Option<Dec>,
    stated: Option<Dec>,
    starting: Option<Dec>,
    ending: Option<Dec>,
    flows: Dec,
}

pub struct Twr {
    file: File,
    records: Vec<Value>,
    rates: Vec<(String, String, Option<f64>)>, // from, to, rate
    no_t2: usize,
    no_total: usize,
    bad_period: usize,
}

impl Twr {
    pub fn new() -> Self {
        Twr {
            file: File::default(),
            records: Vec::new(),
            rates: Vec::new(),
            no_t2: 0,
            no_total: 0,
            bad_period: 0,
        }
    }

    fn finish_file(&mut self) {
        let f = std::mem::take(&mut self.file);
        if f.prior_total.is_none() {
            return; // no NAV table 1 in this file
        }
        if f.stated.is_none() {
            self.no_t2 += 1;
        }
        let fallback = match (f.starting, f.ending) {
            (Some(s), Some(e)) if s > Dec::ZERO => Some((e - s - f.flows).to_f64() / s.to_f64()),
            _ => None,
        };
        let rate = f.stated.map(|d| d.to_f64()).or(fallback);
        let source = if f.stated.is_some() {
            "table2"
        } else if rate.is_some() {
            "nav-chain"
        } else {
            "none"
        };
        let period = dates::stated_period(&f.period);
        let (from, to, grain) = match &period {
            Some((Some(a), Some(b), false)) => (a.clone(), b.clone(), "day"),
            Some((Some(a), Some(b), true)) => (a.clone(), b.clone(), "range"),
            _ => {
                self.bad_period += 1;
                (String::new(), String::new(), "unknown")
            }
        };
        if f.current_total.is_none() {
            self.no_total += 1;
        }
        self.records.push(Value::Obj(vec![
            ("period".to_string(), Value::Str(f.period.clone())),
            (
                "period_from".to_string(),
                if from.is_empty() {
                    Value::Null
                } else {
                    Value::Str(from.clone())
                },
            ),
            (
                "period_to".to_string(),
                if to.is_empty() {
                    Value::Null
                } else {
                    Value::Str(to.clone())
                },
            ),
            ("grain".to_string(), Value::Str(grain.to_string())),
            (
                "prior_total".to_string(),
                match f.prior_total {
                    Some(v) => Value::num(&v),
                    None => Value::Null,
                },
            ),
            (
                "current_total".to_string(),
                match f.current_total {
                    Some(v) => Value::num(&v),
                    None => Value::Null,
                },
            ),
            (
                "twr_stated".to_string(),
                match f.stated {
                    Some(v) => Value::num(&v),
                    None => Value::Null,
                },
            ),
            ("fallback".to_string(), json::ratio(fallback)),
            ("source".to_string(), Value::Str(source.to_string())),
        ]));
        if rate.is_some() && !from.is_empty() {
            self.rates.push((from, to, rate));
        }
    }
}

impl Target for Twr {
    fn name(&self) -> &'static str {
        "twr"
    }

    fn on_file_begin(&mut self) {
        self.file = File::default();
    }

    fn on_meta(&mut self, key: &str, value: &str) {
        if key == "Period" {
            self.file.period = value.to_string();
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
                if let Some(raw) = row.cell(TWR_FIELD) {
                    self.file.stated = Dec::parse(raw).ok();
                }
            }
            NAV_SECTION => {
                let Some(field) = row.cell("Field") else {
                    return;
                };
                let Some(raw) = row.cell("Value") else {
                    return;
                };
                let Ok(v) = Dec::parse(raw) else {
                    return;
                };
                match field {
                    "Starting Value" => self.file.starting = Some(v),
                    "Ending Value" => self.file.ending = Some(v),
                    "Deposits & Withdrawals" => self.file.flows = v,
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn on_file_end(&mut self) {
        self.finish_file();
    }

    fn report(&self) -> Value {
        // chain in period order; overlapping statement periods are
        // chained anyway and reported.
        let mut rates = self.rates.clone();
        rates.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
        let mut overlaps = 0usize;
        let mut factor = 1.0f64;
        let mut broken = false;
        for (i, r) in rates.iter().enumerate() {
            if i > 0 && r.0 <= rates[i - 1].1 {
                overlaps += 1;
            }
            let f = 1.0 + r.2.unwrap_or(0.0);
            if f <= 0.0 {
                broken = true;
            }
            factor *= f;
        }
        let chain = if rates.is_empty() {
            Value::Null
        } else {
            let from = rates.iter().map(|r| r.0.as_str()).min().unwrap_or_default();
            let to = rates.iter().map(|r| r.1.as_str()).max().unwrap_or_default();
            let days = match (dates::iso_to_days(from), dates::iso_to_days(to)) {
                (Some(a), Some(b)) => (b - a).max(0) as f64,
                _ => 0.0,
            };
            let years = days / 365.25;
            let annualized = if !broken && factor > 0.0 && years > 0.0 {
                Some(factor.powf(1.0 / years) - 1.0)
            } else {
                None
            };
            Value::Obj(vec![
                ("periods".to_string(), Value::Num(rates.len().to_string())),
                ("from".to_string(), Value::Str(from.to_string())),
                ("to".to_string(), Value::Str(to.to_string())),
                ("years".to_string(), Value::Num(json::ratio_text(years))),
                (
                    "cumulative_return".to_string(),
                    if broken {
                        Value::Null
                    } else {
                        json::ratio(Some(factor - 1.0))
                    },
                ),
                ("annualized".to_string(), json::ratio(annualized)),
            ])
        };
        let mut notes = vec![Value::Str(
            "periods chain multiplicatively; the TWR column is never averaged".to_string(),
        )];
        if self.no_t2 > 0 {
            notes.push(Value::Str(format!(
                "files whose table 2 is absent, rate taken from the NAV chain: {}",
                self.no_t2
            )));
        }
        if self.no_total > 0 {
            notes.push(Value::Str(format!(
                "files whose NAV table 1 Total row is missing a parsed total: {}",
                self.no_total
            )));
        }
        if overlaps > 0 {
            notes.push(Value::Str(format!(
                "overlapping statement periods chained anyway: {overlaps}"
            )));
        }
        if self.bad_period > 0 {
            notes.push(Value::Str(format!(
                "files with an unparsable statement Period: {}",
                self.bad_period
            )));
        }
        Value::Obj(vec![
            ("records".to_string(), Value::Arr(self.records.clone())),
            ("chain".to_string(), chain),
            ("notes".to_string(), Value::Arr(notes)),
        ])
    }
}
