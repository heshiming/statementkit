//! `attribution`: map the `Change in Combined NAV` EAV keys of each
//! block into buckets and report the residual of IB's ledger identity.
//!
//! Buckets: income (interest, dividends and their accrual changes),
//! fees (commissions), tax (withholding), fx (Other FX Translations),
//! flows (Deposits & Withdrawals, Position Transfers), mtm
//! (Mark-to-Market). Residual = `Ending - Starting` minus the sum of
//! all bucketed keys; structurally 0 in well-formed blocks. A bucket
//! is null when none of its keys appear in the block.

use crate::dates;
use crate::dec::Dec;
use crate::json::Value;
use crate::target::{RowRef, Target};

const SECTION: &str = "Change in Combined NAV";

const BUCKETS: [(&str, &[&str]); 6] = [
    (
        "income",
        &[
            "Interest",
            "Dividends",
            "Change in Interest Accruals",
            "Change in Dividend Accruals",
        ],
    ),
    ("fees", &["Commissions"]),
    ("tax", &["Withholding Tax"]),
    ("fx", &["Other FX Translations"]),
    ("flows", &["Deposits & Withdrawals", "Position Transfers"]),
    ("mtm", &["Mark-to-Market"]),
];

#[derive(Default)]
struct Block {
    period: String,
    starting: Option<Dec>,
    ending: Option<Dec>,
    keys: Vec<(&'static str, Dec)>,
}

pub struct Attribution {
    period: String,
    blocks: Vec<Block>,
    records: Vec<Value>,
    unparsed: usize,
    nonzero_residual: usize,
    bad_period: usize,
}

impl Attribution {
    pub fn new() -> Self {
        Attribution {
            period: String::new(),
            blocks: Vec::new(),
            records: Vec::new(),
            unparsed: 0,
            nonzero_residual: 0,
            bad_period: 0,
        }
    }
}

impl Target for Attribution {
    fn name(&self) -> &'static str {
        "attribution"
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
        let Ok(v) = Dec::parse(raw) else {
            self.unparsed += 1;
            return;
        };
        match field {
            "Starting Value" => block.starting = Some(v),
            "Ending Value" => block.ending = Some(v),
            _ => {
                if let Some(bucket) = bucket_of(field) {
                    block.keys.push((bucket, v));
                }
            }
        }
    }

    fn on_file_end(&mut self) {
        for block in self.blocks.drain(..) {
            let (record, nonzero, bad_period) = record_json(block);
            if bad_period {
                self.bad_period += 1;
            }
            if nonzero {
                self.nonzero_residual += 1;
            }
            self.records.push(record);
        }
    }

    fn report(&self) -> Value {
        let mut notes = vec![Value::Str(
            "buckets are null when none of their keys appear in the block".to_string(),
        )];
        if self.unparsed > 0 {
            notes.push(Value::Str(format!(
                "EAV values that failed to parse: {}",
                self.unparsed
            )));
        }
        if self.nonzero_residual > 0 {
            notes.push(Value::Str(format!(
                "blocks whose residual (End-Start minus summed keys) is not 0: {}",
                self.nonzero_residual
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

fn bucket_of(field: &str) -> Option<&'static str> {
    BUCKETS
        .iter()
        .find(|(_, keys)| keys.contains(&field))
        .map(|(name, _)| *name)
}

fn record_json(block: Block) -> (Value, bool, bool) {
    let period = dates::stated_period(&block.period);
    let mut buckets = Vec::new();
    let mut sum = Dec::ZERO;
    for (name, _) in BUCKETS {
        let total = block
            .keys
            .iter()
            .filter(|(b, _)| *b == name)
            .map(|(_, v)| *v)
            .reduce(|a, b| a + b);
        for (_, v) in block.keys.iter().filter(|(b, _)| *b == name) {
            sum = sum + *v;
        }
        buckets.push((
            (*name).to_string(),
            match total {
                Some(v) => Value::num(&v),
                None => Value::Null,
            },
        ));
    }
    let residual = match (block.starting, block.ending) {
        (Some(s), Some(e)) => Some(e - s - sum),
        _ => None,
    };
    let (from, to) = match &period {
        Some((f, t, _)) => (f.clone(), t.clone()),
        None => (None, None),
    };
    let grain = match &period {
        None => "unknown",
        Some((_, _, true)) => "range",
        Some((_, _, false)) => "day",
    };
    let nonzero = residual.map(|r| r != Dec::ZERO).unwrap_or(true);
    let record = Value::Obj(vec![
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
        ("grain".to_string(), Value::Str(grain.to_string())),
        ("buckets".to_string(), Value::Obj(buckets)),
        (
            "residual".to_string(),
            match residual {
                Some(v) => Value::num(&v),
                None => Value::Null,
            },
        ),
    ]);
    (record, nonzero, period.is_none())
}
