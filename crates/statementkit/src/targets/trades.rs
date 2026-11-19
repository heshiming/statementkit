//! `trades`: execution structure from `Trades` blocks.
//!
//! The `Trades` section repeats as one block per asset category with
//! header variants: Futures rename `Proceeds` to `Notional Value`,
//! Forex replace `Comm/Fee`/`MTM P/L` with `Comm in USD`/`MTM in
//! USD` and carry no realized P&L, annuals drop `Account`. Columns
//! are read through alias lists so no block variant is dropped.
//! Rows are deduplicated across overlapping files by
//! (symbol, date/time, quantity, realized).
//!
//! Fills match into trades with signed-quantity FIFO per symbol;
//! equal-stamp fills keep file row order. A close with no open lot
//! is LEFT-CENSORED: hold unknown (not zero), its `Realized P/L`
//! still counts. Expiry/assignment marker codes (`Ep`, `Ex`,
//! `A;C`) are terminal closes: kept in holds, excluded from hour
//! histograms. `Realized P/L` is summed over rows whose code
//! contains C or A only: open rows can carry a bookkeeping artifact
//! P&L, counted separately, never added. `Comm/Fee` is already
//! inside `Realized P/L` and is never re-added.
//!
//! Hour buckets use the file's own stamp (zone undeclared in the
//! files; IB convention is US Eastern). Product session is only
//! split as option vs other: distinguishing futures from stocks
//! needs instrument metadata that statements do not carry.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::dates;
use crate::dec::Dec;
use crate::json::Value;
use crate::target::{RowRef, Target};

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Fill {
    symbol: String,
    ts: i64,
    iso: String,
    qty: Dec,
    proceeds: Option<Dec>,
    comm: Option<Dec>,
    basis: Option<Dec>,
    realized: Option<Dec>,
    code: String,
    forex: bool,
}

pub struct Trades {
    dedup: BTreeSet<(String, String, String, String)>,
    fills: Vec<Fill>,
    dupes: usize,
    blocks: usize,
    unparsed: usize,
}

impl Trades {
    pub fn new() -> Self {
        Trades {
            dedup: BTreeSet::new(),
            fills: Vec::new(),
            dupes: 0,
            blocks: 0,
            unparsed: 0,
        }
    }
}

/// `2026-09-02, 10:00:00` -> (epoch seconds, ISO date).
fn stamp_to_ts(stamp: &str) -> Option<(i64, String)> {
    let (date, time) = stamp.split_once(", ")?;
    let days = dates::iso_to_days(date)?;
    let mut parts = [0i64; 3];
    for (i, p) in time.split(':').enumerate().take(3) {
        parts[i] = p.parse().ok()?;
    }
    Some((
        days * 86400 + parts[0] * 3600 + parts[1] * 60 + parts[2],
        date.to_string(),
    ))
}

fn is_marker(code: &str) -> bool {
    code.contains("Ep") || code.contains("Ex") || code == "A;C"
}

fn is_transfer(code: &str) -> bool {
    code.contains("IA") || code.contains("IM")
}

fn is_close(code: &str) -> bool {
    code.contains('C') || code.contains('A')
}

fn is_option_symbol(symbol: &str) -> bool {
    let p: Vec<&str> = symbol.split_whitespace().collect();
    p.len() == 4
        && (p[3] == "C" || p[3] == "P")
        && p[1].len() == 7
        && p[1].bytes().next().is_some_and(|b| b.is_ascii_digit())
}

impl Target for Trades {
    fn name(&self) -> &'static str {
        "trades"
    }

    fn on_section(&mut self, title: &str, _fields: &[String]) {
        if title == "Trades" {
            self.blocks += 1;
        }
    }

    fn on_row(&mut self, row: &RowRef) {
        if row.title != "Trades" {
            return;
        }
        let (Some(symbol), Some(stamp), Some(qty_raw)) = (
            row.cell("Symbol"),
            row.cell("Date/Time"),
            row.cell("Quantity"),
        ) else {
            return;
        };
        let cell_any = |names: [&str; 2]| row.cell(names[0]).or_else(|| row.cell(names[1]));
        let forex = row.cell("Comm in USD").is_some() || row.cell("MTM in USD").is_some();
        let (Ok(qty), Some((ts, iso))) = (Dec::parse(qty_raw), stamp_to_ts(stamp)) else {
            self.unparsed += 1;
            return;
        };
        let realized_raw = row.cell("Realized P/L");
        let key = (
            symbol.to_string(),
            stamp.to_string(),
            qty.to_text(),
            realized_raw.unwrap_or("").to_string(),
        );
        if !self.dedup.insert(key) {
            self.dupes += 1;
            return;
        }
        let parse_cell = |raw: Option<&str>| raw.and_then(|r| Dec::parse(r).ok());
        self.fills.push(Fill {
            symbol: symbol.to_string(),
            ts,
            iso,
            qty,
            proceeds: parse_cell(cell_any(["Proceeds", "Notional Value"])),
            comm: parse_cell(cell_any(["Comm/Fee", "Comm in USD"])),
            basis: parse_cell(row.cell("Basis")),
            realized: parse_cell(realized_raw),
            code: row.cell("Code").unwrap_or("").to_string(),
            forex,
        });
    }

    fn report(&self) -> Value {
        // FIFO per symbol in (ts, file order); fills are pushed in file
        // order and sorted stably so equal stamps keep it.
        let mut fills: Vec<&Fill> = self.fills.iter().collect();
        fills.sort_by_key(|f| f.ts);
        let mut open: BTreeMap<&str, VecDeque<(i64, String, Dec)>> = BTreeMap::new();
        let mut holds: Vec<i64> = Vec::new();
        let mut hold_same_day = 0usize;
        let mut matched = 0usize;
        let mut censored = 0usize;
        let mut transfers = 0usize;
        let mut markers = 0usize;
        let mut hours: BTreeMap<(&str, i64), usize> = BTreeMap::new();
        let mut realized_total = Dec::ZERO;
        let mut close_rows = 0usize;
        let mut identity_ok = 0usize;
        let mut identity_bad = 0usize;
        let mut open_rows_with_pnl = 0usize;
        let mut open_pnl_total = Dec::ZERO;

        for f in fills {
            if is_transfer(&f.code) {
                transfers += 1;
                continue;
            }
            let class = if is_option_symbol(&f.symbol) {
                "option"
            } else {
                "other"
            };
            if is_close(&f.code) {
                markers += if is_marker(&f.code) { 1 } else { 0 };
            }
            if !is_marker(&f.code) {
                let hour = (f.ts.div_euclid(3600) % 24 + 24) % 24;
                *hours.entry((class, hour)).or_insert(0) += 1;
            }
            if let (Some(p), Some(c), Some(b), Some(r)) = (f.proceeds, f.comm, f.basis, f.realized)
            {
                if is_close(&f.code) && !f.forex {
                    close_rows += 1;
                    realized_total = realized_total + r;
                    if p + c + b == r {
                        identity_ok += 1;
                    } else {
                        identity_bad += 1;
                    }
                } else if !is_close(&f.code) && r != Dec::ZERO {
                    open_rows_with_pnl += 1;
                    open_pnl_total = open_pnl_total + r;
                }
            }
            let lots = open.entry(f.symbol.as_str()).or_default();
            if f.qty > Dec::ZERO {
                // buy: close shorts oldest-first, remainder opens a long lot
                let mut left = f.qty;
                while left > Dec::ZERO && lots.front().is_some_and(|l| l.2 < Dec::ZERO) {
                    let front = lots.front_mut().unwrap();
                    if -front.2 <= left {
                        left = left + front.2;
                        lots.pop_front();
                    } else {
                        front.2 = front.2 + left;
                        left = Dec::ZERO;
                    }
                }
                if left > Dec::ZERO {
                    lots.push_back((f.ts, f.iso.clone(), left));
                }
            } else if f.qty < Dec::ZERO {
                let mut left = -f.qty;
                let mut first: Option<(i64, String)> = None;
                while left > Dec::ZERO && lots.front().is_some_and(|l| l.2 > Dec::ZERO) {
                    let front = lots.front().unwrap();
                    if first.is_none() {
                        first = Some((front.0, front.1.clone()));
                    }
                    if front.2 <= left {
                        left = left - front.2;
                        lots.pop_front();
                    } else {
                        let idx = 0;
                        lots[idx].2 = lots[idx].2 - left;
                        left = Dec::ZERO;
                    }
                }
                if is_close(&f.code) {
                    match first {
                        Some((ots, oiso)) => {
                            matched += 1;
                            holds.push(f.ts - ots);
                            if f.iso == oiso {
                                hold_same_day += 1;
                            }
                        }
                        None => censored += 1,
                    }
                }
                if left > Dec::ZERO {
                    lots.push_back((f.ts, f.iso.clone(), -left));
                }
            }
        }

        holds.sort_unstable();
        let pct = |p: f64| -> Value {
            if holds.is_empty() {
                return Value::Null;
            }
            let idx = (p * holds.len() as f64).ceil() as usize - 1;
            Value::Num(holds[idx.min(holds.len() - 1)].to_string())
        };
        let hour_list: Vec<Value> = hours
            .iter()
            .map(|((c, h), n)| {
                Value::Obj(vec![
                    ("class".to_string(), Value::Str(c.to_string())),
                    ("hour".to_string(), Value::Num(h.to_string())),
                    ("fills".to_string(), Value::Num(n.to_string())),
                ])
            })
            .collect();
        let mut notes = vec![
            Value::Str(
                "holds are FIFO-matched approximations; Realized P/L is IB's own per-row number"
                    .to_string(),
            ),
            Value::Str(
                "censored closes keep their Realized P/L; their hold is unknown, not zero"
                    .to_string(),
            ),
            Value::Str(
                "stamps keep the file's zone (undeclared; IB convention US Eastern)".to_string(),
            ),
            Value::Str(
                "session class is option vs other only; futures vs stocks need instrument metadata"
                    .to_string(),
            ),
        ];
        if censored > 0 {
            notes.push(Value::Str(format!(
                "left-censored closes: {censored} of {}",
                matched + censored
            )));
        }
        if open_rows_with_pnl > 0 {
            notes.push(Value::Str(format!(
                "open rows carrying a non-zero Realized P/L artifact: {open_rows_with_pnl} (total {open_pnl_total}, excluded)"
            )));
        }
        if identity_bad > 0 {
            notes.push(Value::Str(format!(
                "close rows where Proceeds+Comm/Fee+Basis != Realized P/L: {identity_bad} (realized kept, identity not rebuilt)"
            )));
        }
        if self.dupes > 0 {
            notes.push(Value::Str(format!(
                "duplicate fills dropped (overlapping files): {}",
                self.dupes
            )));
        }
        if self.unparsed > 0 {
            notes.push(Value::Str(format!(
                "rows with unparsable stamp or quantity: {}",
                self.unparsed
            )));
        }
        Value::Obj(vec![
            ("blocks".to_string(), Value::Num(self.blocks.to_string())),
            (
                "fills".to_string(),
                Value::Num(self.fills.len().to_string()),
            ),
            ("transfers".to_string(), Value::Num(transfers.to_string())),
            ("marker_closes".to_string(), Value::Num(markers.to_string())),
            (
                "closes_matched".to_string(),
                Value::Num(matched.to_string()),
            ),
            (
                "closes_censored".to_string(),
                Value::Num(censored.to_string()),
            ),
            (
                "hold_seconds".to_string(),
                Value::Obj(vec![
                    ("p25".to_string(), pct(0.25)),
                    ("p50".to_string(), pct(0.50)),
                    ("p75".to_string(), pct(0.75)),
                    ("p90".to_string(), pct(0.90)),
                    (
                        "same_day_share".to_string(),
                        if matched > 0 {
                            Value::Num(crate::json::ratio_text(
                                hold_same_day as f64 / matched as f64,
                            ))
                        } else {
                            Value::Null
                        },
                    ),
                ]),
            ),
            ("hours".to_string(), Value::Arr(hour_list)),
            (
                "realized_pnl".to_string(),
                Value::Obj(vec![
                    ("close_rows".to_string(), Value::Num(close_rows.to_string())),
                    ("total".to_string(), Value::num(&realized_total)),
                    (
                        "identity_ok".to_string(),
                        Value::Num(identity_ok.to_string()),
                    ),
                    (
                        "identity_bad".to_string(),
                        Value::Num(identity_bad.to_string()),
                    ),
                    (
                        "open_rows_with_pnl".to_string(),
                        Value::Num(open_rows_with_pnl.to_string()),
                    ),
                ]),
            ),
            ("notes".to_string(), Value::Arr(notes)),
        ])
    }
}
