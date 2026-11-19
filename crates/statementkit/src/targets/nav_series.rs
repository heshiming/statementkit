//! `nav-series`: the NAV chain one record per statement block, from the
//! `Change in Combined NAV` EAV (rows arrive tagged `Change in NAV` and
//! are retagged by the session).
//!
//! Per record: period (raw and ISO), starting/ending value, each change
//! component, all-in P&L (`Ending - Starting - Deposits & Withdrawals`)
//! and the block's self-reconciliation (`Ending - Starting` vs the sum
//! of the other keys). An absent component is reported `null` and
//! counted as 0 in the identity; it is not a printed zero.

use crate::dates;
use crate::dec::Dec;
use crate::json::Value;
use crate::target::{RowRef, Target};

const SECTION: &str = "Change in Combined NAV";
const COMPONENTS: [&str; 10] = [
    "Mark-to-Market",
    "Interest",
    "Dividends",
    "Commissions",
    "Withholding Tax",
    "Change in Interest Accruals",
    "Change in Dividend Accruals",
    "Other FX Translations",
    "Position Transfers",
    "Deposits & Withdrawals",
];

#[derive(Default)]
struct Block {
    period: String,
    starting: Option<Dec>,
    ending: Option<Dec>,
    components: [Option<Dec>; COMPONENTS.len()],
}

pub struct NavSeries {
    period: String,
    blocks: Vec<Block>,
    records: Vec<Value>,
    unparsed: usize,
    bad_identity: usize,
    bad_period: usize,
}

impl NavSeries {
    pub fn new() -> Self {
        NavSeries {
            period: String::new(),
            blocks: Vec::new(),
            records: Vec::new(),
            unparsed: 0,
            bad_identity: 0,
            bad_period: 0,
        }
    }
}

impl Target for NavSeries {
    fn name(&self) -> &'static str {
        "nav-series"
    }

    fn on_file_begin(&mut self) {
        self.blocks.clear();
    }

    fn on_meta(&mut self, key: &str, value: &str) {
        if key == "Period" {
            self.period = value.to_string();
        }
    }

    fn on_section(&mut self, title: &str, _fields: &[String]) {
        if title == SECTION {
            self.blocks.push(Block {
                period: self.period.clone(),
                ..Default::default()
            });
        }
    }

    fn on_row(&mut self, row: &RowRef) {
        if row.title != SECTION {
            return;
        }
        let Some(field) = row.cell("Field") else {
            return;
        };
        let Some(raw) = row.cell("Value") else { return };
        let Some(block) = self.blocks.last_mut() else {
            return;
        };
        let parsed = match Dec::parse(raw) {
            Ok(v) => Some(v),
            Err(_) => {
                self.unparsed += 1;
                None
            }
        };
        match field {
            "Starting Value" => block.starting = parsed,
            "Ending Value" => block.ending = parsed,
            _ => {
                if let Some(i) = COMPONENTS.iter().position(|k| *k == field) {
                    block.components[i] = parsed;
                }
            }
        }
    }

    fn on_file_end(&mut self) {
        for block in self.blocks.drain(..) {
            self.records.push(record_json(
                block,
                &mut self.bad_identity,
                &mut self.bad_period,
            ));
        }
    }

    fn report(&self) -> Value {
        let mut notes = Vec::new();
        notes.push(Value::Str(
            "absent component keys are null and count as 0 in the identity".to_string(),
        ));
        if self.unparsed > 0 {
            notes.push(Value::Str(format!(
                "EAV values that failed to parse: {}",
                self.unparsed
            )));
        }
        if self.bad_identity > 0 {
            notes.push(Value::Str(format!(
                "blocks failing the End-Start = sum(keys) identity: {}",
                self.bad_identity
            )));
        }
        if self.bad_period > 0 {
            notes.push(Value::Str(format!(
                "blocks with an unparsable statement Period: {}",
                self.bad_period
            )));
        }
        Value::Obj(vec![
            ("records".to_string(), Value::Arr(self.records.clone())),
            ("notes".to_string(), Value::Arr(notes)),
        ])
    }
}

fn record_json(block: Block, bad_identity: &mut usize, bad_period: &mut usize) -> Value {
    let period = dates::stated_period(&block.period);
    let (from, to, is_range) = match &period {
        Some((f, t, r)) => (f.clone(), t.clone(), *r),
        None => {
            *bad_period += 1;
            (None, None, false)
        }
    };
    let dw = || {
        block.components[COMPONENTS
            .iter()
            .position(|k| *k == "Deposits & Withdrawals")
            .unwrap()]
    };
    let identity_ok = match (block.starting, block.ending) {
        (Some(s), Some(e)) => {
            let mut sum = Dec::ZERO;
            for c in block.components.iter().flatten() {
                sum = sum + *c;
            }
            s + sum == e
        }
        _ => false,
    };
    if !identity_ok {
        *bad_identity += 1;
    }
    let day_pnl = match (block.starting, block.ending) {
        (Some(s), Some(e)) => Some(e - s - dw().unwrap_or(Dec::ZERO)),
        _ => None,
    };
    let mut comps = Vec::new();
    for (i, name) in COMPONENTS.iter().enumerate() {
        comps.push((
            (*name).to_string(),
            match block.components[i] {
                Some(v) => Value::num(&v),
                None => Value::Null,
            },
        ));
    }
    Value::Obj(vec![
        ("period".to_string(), Value::Str(block.period.clone())),
        (
            "period_from".to_string(),
            match from {
                Some(v) => Value::Str(v),
                None => Value::Null,
            },
        ),
        (
            "period_to".to_string(),
            match to {
                Some(v) => Value::Str(v),
                None => Value::Null,
            },
        ),
        (
            "grain".to_string(),
            Value::Str(
                if period.is_none() {
                    "unknown"
                } else if is_range {
                    "range"
                } else {
                    "day"
                }
                .to_string(),
            ),
        ),
        (
            "starting_value".to_string(),
            match block.starting {
                Some(v) => Value::num(&v),
                None => Value::Null,
            },
        ),
        (
            "ending_value".to_string(),
            match block.ending {
                Some(v) => Value::num(&v),
                None => Value::Null,
            },
        ),
        (
            "day_pnl".to_string(),
            match day_pnl {
                Some(v) => Value::num(&v),
                None => Value::Null,
            },
        ),
        ("components".to_string(), Value::Obj(comps)),
        ("identity_exact".to_string(), Value::Bool(identity_ok)),
    ])
}
