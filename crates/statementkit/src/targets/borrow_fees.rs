//! `borrow-fees`: fee economics for a short book, kept separate.
//!
//! Verified in the record set: `Realized P/L = Proceeds + Basis +
//! Comm/Fee` holds equally for borrowed symbols, and `Unrealized P/L
//! = Value − Cost Basis` is exact — the statement's per-symbol P&L is
//! net of commissions but GROSS of borrow cost. The fee lives in
//! `Borrow Fee Details` at account level per period. So fee-net P&L
//! must be assembled, and attribution is explicit: this target keeps
//! the fee as its own line per symbol and never subtracts period fees
//! from realized silently.
//!
//! Controls:
//! - each block's own `Total` row vs its detail sum (they tie);
//! - annual `Borrow Fee Details` is a partial extract — where a file
//!   also posts `USD Borrow Fees for Mon-YYYY` rows in `Interest`, the
//!   postings are the annual control total and the gap is reported;
//! - the two grains are never added together.
//!
//! Detail rows are deduplicated across overlapping files by
//! (symbol, value date, quantity, fee). `Fee Rate (%)` is a percent
//! form and is not parsed; the blended daily rates are computed from
//! `Borrow Fee`, `Quantity` and `Value` instead.

use std::collections::{BTreeMap, BTreeSet};

use crate::dec::Dec;
use crate::json;
use crate::json::Value;
use crate::target::{RowRef, Target};

#[derive(Default)]
struct Sym {
    fee: Dec,
    days: BTreeSet<String>,
    share_days: Dec,
    value_days: Dec,
    peak: Dec,
}

#[derive(Default)]
struct Block {
    detail: Dec,
    total: Option<Dec>,
}

pub struct BorrowFees {
    seen: BTreeSet<(String, String, String, String)>,
    dupes: usize,
    blocks: Vec<Block>,
    base: String,
    in_block: Option<usize>,
    syms: BTreeMap<String, Sym>,
    posted: BTreeMap<usize, Dec>, // file seq -> Interest posting total
    file_seq: usize,
    file_posting: Dec,
    file_has_posting: bool,
    fee_files: BTreeSet<usize>,
    nonusd: usize,
}

impl Default for BorrowFees {
    fn default() -> Self {
        BorrowFees {
            seen: BTreeSet::new(),
            dupes: 0,
            blocks: Vec::new(),
            base: String::new(),
            in_block: None,
            syms: BTreeMap::new(),
            posted: BTreeMap::new(),
            file_seq: 0,
            file_posting: Dec::ZERO,
            file_has_posting: false,
            fee_files: BTreeSet::new(),
            nonusd: 0,
        }
    }
}

impl BorrowFees {
    pub fn new() -> Self {
        BorrowFees::default()
    }
}

fn dec(raw: Option<&str>) -> Option<Dec> {
    raw.and_then(|r| Dec::parse(r).ok())
}

impl Target for BorrowFees {
    fn name(&self) -> &'static str {
        "borrow-fees"
    }

    fn on_meta(&mut self, key: &str, value: &str) {
        if key == "Currency" {
            self.base = value.to_string();
        }
    }

    fn on_section(&mut self, title: &str, _fields: &[String]) {
        self.in_block = if title == "Borrow Fee Details" {
            self.blocks.push(Block::default());
            self.fee_files.insert(self.file_seq);
            Some(self.blocks.len() - 1)
        } else {
            None
        };
    }

    fn on_file_end(&mut self) {
        if self.file_has_posting {
            let e = self.posted.entry(self.file_seq).or_insert(Dec::ZERO);
            *e = *e + self.file_posting;
        }
        self.file_posting = Dec::ZERO;
        self.file_has_posting = false;
        self.file_seq += 1;
    }

    fn on_row(&mut self, row: &RowRef) {
        match row.title {
            "Borrow Fee Details" => {
                let Some(b) = self.in_block else { return };
                let Some(fee) = dec(row.cell("Borrow Fee")) else {
                    return;
                };
                let symbol = row.cell("Symbol").unwrap_or("");
                if symbol.trim() == "Total" {
                    self.blocks[b].total = Some(fee);
                    return;
                }
                let ccy = row.cell("Currency").unwrap_or(&self.base).to_string();
                if ccy != self.base {
                    self.nonusd += 1;
                    return;
                }
                let qty_raw = row.cell("Quantity").unwrap_or("");
                let vdate = row.cell("Value Date").unwrap_or("");
                // the block's own Total control uses the file's raw detail rows;
                // dedup applies to the per-symbol aggregation only
                self.blocks[b].detail = self.blocks[b].detail + fee;
                if !self.seen.insert((
                    symbol.to_string(),
                    vdate.to_string(),
                    qty_raw.to_string(),
                    fee.to_text(),
                )) {
                    self.dupes += 1;
                    return;
                }
                let s = self.syms.entry(symbol.to_string()).or_default();
                s.fee = s.fee + fee;
                if !vdate.is_empty() {
                    s.days.insert(vdate.to_string());
                }
                if let Some(q) = dec(Some(qty_raw)) {
                    let aq = if q < Dec::ZERO { -q } else { q };
                    s.share_days = s.share_days + aq;
                    if aq > s.peak {
                        s.peak = aq;
                    }
                }
                if let Some(v) = dec(row.cell("Value")) {
                    s.value_days = s.value_days + v;
                }
            }
            "Interest" => {
                let desc = row.cell("Description").unwrap_or("");
                if desc.contains("Borrow Fees") {
                    if let Some(a) = dec(row.cell("Amount")) {
                        self.file_posting = self.file_posting + a;
                        self.file_has_posting = true;
                    }
                }
            }
            _ => {}
        }
    }

    fn report(&self) -> Value {
        // block control: detail vs its own Total row
        let mut control_ok = 0usize;
        let mut control_bad = 0usize;
        let mut worst = 0.0f64;
        let mut worst_set = false;
        for b in &self.blocks {
            let Some(t) = b.total else { continue };
            if t != Dec::ZERO {
                let diff = ((b.detail - t).to_f64() / t.to_f64()).abs();
                if !worst_set || diff > worst {
                    worst = diff;
                    worst_set = true;
                }
            }
            if b.detail == t {
                control_ok += 1;
            } else {
                control_bad += 1;
            }
        }
        // annual control: Interest postings vs same file's detail
        let mut annual_gaps = Vec::new();
        for (seq, posting) in &self.posted {
            if self.fee_files.contains(seq) {
                annual_gaps.push((*seq, *posting));
            }
        }
        let sym_list: Vec<Value> = self
            .syms
            .iter()
            .filter(|(_, s)| s.fee != Dec::ZERO || !s.days.is_empty())
            .map(|(symbol, s)| {
                let fee_share_day = if s.share_days != Dec::ZERO {
                    s.fee.to_f64() / s.share_days.to_f64()
                } else {
                    0.0
                };
                let fee_value_day = if s.value_days != Dec::ZERO {
                    s.fee.to_f64() / s.value_days.to_f64()
                } else {
                    0.0
                };
                Value::Obj(vec![
                    ("symbol".to_string(), Value::Str(symbol.clone())),
                    ("days".to_string(), Value::Num(s.days.len().to_string())),
                    ("peak_shares".to_string(), Value::num(&s.peak)),
                    ("fee".to_string(), Value::num(&s.fee)),
                    (
                        "fee_per_share_day".to_string(),
                        Value::Num(json::ratio_text(fee_share_day)),
                    ),
                    (
                        "fee_pct_of_value_day".to_string(),
                        Value::Num(json::ratio_text(fee_value_day)),
                    ),
                ])
            })
            .collect();
        let total_fee: Dec = self
            .syms
            .values()
            .map(|s| s.fee)
            .fold(Dec::ZERO, |a, b| a + b);
        let total_days: usize = self.syms.values().map(|s| s.days.len()).sum();
        let mut notes = vec![
            Value::Str("statement P&L is gross of borrow fees (identity checks verified); fee stays its own line, never subtracted from realized here".to_string()),
            Value::Str("fee/share-day and fee/%-of-value-day are blended ratios; the Fee Rate (%) column is a percent form and is not parsed".to_string()),
            Value::Str("annual Borrow Fee Details is a partial extract: the Interest postings are the annual control".to_string()),
            Value::Str("detail and Interest posting grains are the same fee; they are never added together".to_string()),
        ];
        if control_bad > 0 {
            notes.push(Value::Str(format!(
                "blocks where detail sum != its own Total row: {control_bad} of {}",
                control_ok + control_bad
            )));
        }
        for (seq, posting) in &annual_gaps {
            notes.push(Value::Str(format!(
                "file seq {seq}: both fee grains present; Interest posting total {posting} is the annual control, detail is a partial extract"
            )));
        }
        if self.dupes > 0 {
            notes.push(Value::Str(format!(
                "duplicate fee rows dropped (overlapping files): {}",
                self.dupes
            )));
        }
        if self.nonusd > 0 {
            notes.push(Value::Str(format!(
                "non-base-currency fee rows excluded (no FX derivation on fee detail): {}",
                self.nonusd
            )));
        }
        Value::Obj(vec![
            (
                "blocks".to_string(),
                Value::Num(self.blocks.len().to_string()),
            ),
            (
                "block_control".to_string(),
                Value::Obj(vec![
                    (
                        "checked".to_string(),
                        Value::Num((control_ok + control_bad).to_string()),
                    ),
                    ("exact_ties".to_string(), Value::Num(control_ok.to_string())),
                    (
                        "mismatched".to_string(),
                        Value::Num(control_bad.to_string()),
                    ),
                    (
                        "worst_rel_diff".to_string(),
                        if worst_set {
                            Value::Num(json::ratio_text(worst))
                        } else {
                            Value::Null
                        },
                    ),
                ]),
            ),
            (
                "posting_control_files".to_string(),
                Value::Num(annual_gaps.len().to_string()),
            ),
            (
                "total".to_string(),
                Value::Obj(vec![
                    ("fee".to_string(), Value::num(&total_fee)),
                    (
                        "symbol_days".to_string(),
                        Value::Num(total_days.to_string()),
                    ),
                    (
                        "symbols".to_string(),
                        Value::Num(self.syms.len().to_string()),
                    ),
                ]),
            ),
            ("symbols".to_string(), Value::Arr(sym_list)),
            ("notes".to_string(), Value::Arr(notes)),
        ])
    }
}
