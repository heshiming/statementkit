//! `quality`: data-quality notes as first-class output.
//!
//! A figure without its check is a clean-looking lie. This target
//! runs the corpus's own documented identities per file and reports
//! pass/fail counts beside the figures they discipline:
//!
//! - NAV P&L identity: `Ending − Starting − Deposits & Withdrawals`
//!   vs the summed change keys (exact by verification on dailies);
//! - `Value == Close Price × Mult × Quantity` and sign agreement on
//!   every `Open Positions` row;
//! - `Unrealized P/L == Value − Cost Basis` on every row;
//! - close-row identity `Realized P/L == Proceeds + Basis + Comm/Fee`
//!   reported as a RATE, never rebuilt from — the miss is
//!   expiry/roll codes mixing closes with opens;
//! - open rows carrying a non-zero `Realized P/L` (average-cost
//!   artifact) reported as a share of open rows;
//! - `Borrow Fee Details` row counts per file: rows exist only where
//!   fees were charged; absence is not absence of fees.
//!
//! The MTM-plug vs Σ per-symbol `P/L Total` check is NOT run: the
//! per-symbol table misses cash-side MTM and carries embedded Total
//! rows (verified 0/54 equal); the plug is the number.

use std::collections::BTreeMap;

use crate::dec::Dec;
use crate::json::Value;
use crate::target::{RowRef, Target};

#[derive(Default)]
struct Counters {
    nav_ok: usize,
    nav_fail: usize,
    value_ok: usize,
    value_bad: usize,
    sign_bad: usize,
    unreal_ok: usize,
    unreal_bad: usize,
    close_ok: usize,
    close_bad: usize,
    open_rows: usize,
    open_artifacts: usize,
    borrow_rows: usize,
}

pub struct Quality {
    cur: Counters,
    files: Vec<(usize, Counters)>,
    seq: usize,
    nav_keys: BTreeMap<String, Dec>,
    nav_keys_seen: bool,
}

impl Quality {
    pub fn new() -> Self {
        Quality {
            cur: Counters::default(),
            files: Vec::new(),
            seq: 0,
            nav_keys: BTreeMap::new(),
            nav_keys_seen: false,
        }
    }
}

fn dec(raw: Option<&str>) -> Option<Dec> {
    raw.and_then(|r| Dec::parse(r).ok())
}

fn is_close(code: &str) -> bool {
    code.contains('C') || code.contains('A')
}

impl Target for Quality {
    fn name(&self) -> &'static str {
        "quality"
    }

    fn on_section(&mut self, title: &str, _fields: &[String]) {
        if title == "Change in Combined NAV" {
            self.nav_keys.clear();
            self.nav_keys_seen = true;
        }
    }

    fn on_file_end(&mut self) {
        if self.nav_keys_seen {
            let start = self.nav_keys.get("Starting Value").copied();
            let end = self.nav_keys.get("Ending Value").copied();
            let dw = self.nav_keys.get("Deposits & Withdrawals").copied();
            if let (Some(s), Some(e)) = (start, end) {
                let dw = dw.unwrap_or(Dec::ZERO);
                let mut sum = Dec::ZERO;
                for (k, v) in &self.nav_keys {
                    match k.as_str() {
                        "Starting Value" | "Ending Value" | "Deposits & Withdrawals" => {}
                        _ => sum = sum + *v,
                    }
                }
                if e - s - dw == sum {
                    self.cur.nav_ok += 1;
                } else {
                    self.cur.nav_fail += 1;
                }
            }
            self.nav_keys_seen = false;
        }
        self.files
            .push((self.seq + 1, std::mem::take(&mut self.cur)));
        self.seq += 1;
    }

    fn on_row(&mut self, row: &RowRef) {
        match row.title {
            "Change in Combined NAV" => {
                if let (Some(k), Some(v)) = (row.cell("Key"), dec(row.cell("Value"))) {
                    self.nav_keys.insert(k.to_string(), v);
                }
            }
            "Open Positions" => {
                let Some(qty) = dec(row.cell("Quantity")) else {
                    return;
                };
                let Some(value) = dec(row.cell("Value")) else {
                    return;
                };
                if let (Some(price), Some(mult)) =
                    (dec(row.cell("Close Price")), dec(row.cell("Mult")))
                {
                    if price * mult * qty == value {
                        self.cur.value_ok += 1;
                    } else {
                        self.cur.value_bad += 1;
                    }
                }
                if (value > Dec::ZERO) != (qty > Dec::ZERO)
                    && value != Dec::ZERO
                    && qty != Dec::ZERO
                {
                    self.cur.sign_bad += 1;
                }
                if let (Some(u), Some(b)) =
                    (dec(row.cell("Unrealized P/L")), dec(row.cell("Cost Basis")))
                {
                    if u == value + b {
                        self.cur.unreal_ok += 1;
                    } else {
                        self.cur.unreal_bad += 1;
                    }
                }
            }
            "Trades" => {
                let code = row.cell("Code").unwrap_or("");
                if code.contains("IA") || code.contains("IM") {
                    return;
                }
                let realized = dec(row.cell("Realized P/L"));
                if is_close(code) {
                    let (Some(r), Some(p), Some(c), Some(b)) = (
                        realized,
                        row.cell("Proceeds")
                            .and_then(|x| dec(Some(x)))
                            .or_else(|| row.cell("Notional Value").and_then(|x| dec(Some(x)))),
                        row.cell("Comm/Fee")
                            .and_then(|x| dec(Some(x)))
                            .or_else(|| row.cell("Comm in USD").and_then(|x| dec(Some(x)))),
                        dec(row.cell("Basis")),
                    ) else {
                        return;
                    };
                    if p + c + b == r {
                        self.cur.close_ok += 1;
                    } else {
                        self.cur.close_bad += 1;
                    }
                } else if let Some(r) = realized {
                    self.cur.open_rows += 1;
                    if r != Dec::ZERO {
                        self.cur.open_artifacts += 1;
                    }
                }
            }
            "Borrow Fee Details"
                if row.cell("Symbol").map(|s| s.trim() != "Total") == Some(true) =>
            {
                self.cur.borrow_rows += 1;
            }
            _ => {}
        }
    }

    fn report(&self) -> Value {
        let mut agg = Counters::default();
        let mut borrow_by_file: Vec<Value> = Vec::new();
        for (_, c) in &self.files {
            agg.nav_ok += c.nav_ok;
            agg.nav_fail += c.nav_fail;
            agg.value_ok += c.value_ok;
            agg.value_bad += c.value_bad;
            agg.sign_bad += c.sign_bad;
            agg.unreal_ok += c.unreal_ok;
            agg.unreal_bad += c.unreal_bad;
            agg.close_ok += c.close_ok;
            agg.close_bad += c.close_bad;
            agg.open_rows += c.open_rows;
            agg.open_artifacts += c.open_artifacts;
        }
        for (seq, c) in &self.files {
            if c.borrow_rows > 0 {
                borrow_by_file.push(Value::Obj(vec![
                    ("file_seq".to_string(), Value::Num(seq.to_string())),
                    ("rows".to_string(), Value::Num(c.borrow_rows.to_string())),
                ]));
            }
        }
        let counter = |label: &str, ok: usize, bad: usize| {
            Value::Obj(vec![
                ("check".to_string(), Value::Str(label.to_string())),
                ("ok".to_string(), Value::Num(ok.to_string())),
                ("bad".to_string(), Value::Num(bad.to_string())),
            ])
        };
        Value::Obj(vec![
            (
                "checks".to_string(),
                Value::Arr(vec![
                    counter("nav_pnl_identity", agg.nav_ok, agg.nav_fail),
                    counter("position_value_identity", agg.value_ok, agg.value_bad),
                    counter("unrealized_identity", agg.unreal_ok, agg.unreal_bad),
                    counter("close_identity_rate", agg.close_ok, agg.close_bad),
                ]),
            ),
            (
                "shares".to_string(),
                Value::Obj(vec![
                    (
                        "sign_mismatches".to_string(),
                        Value::Num(agg.sign_bad.to_string()),
                    ),
                    (
                        "open_rows_with_realized".to_string(),
                        Value::Obj(vec![
                            (
                                "rows".to_string(),
                                Value::Num(agg.open_rows.to_string()),
                            ),
                            (
                                "artifacts".to_string(),
                                Value::Num(agg.open_artifacts.to_string()),
                            ),
                            (
                                "share".to_string(),
                                Value::Num(crate::json::ratio_text(
                                    agg.open_artifacts as f64 / agg.open_rows.max(1) as f64,
                                )),
                            ),
                        ]),
                    ),
                ]),
            ),
            ("borrow_detail_rows_by_file".to_string(), Value::Arr(borrow_by_file)),
            (
                "notes".to_string(),
                Value::Arr(vec![
                    Value::Str("close identity is reported as a rate and never rebuilt; trust the row's Realized P/L".to_string()),
                    Value::Str("open-row Realized P/L is an average-cost artifact; P&L aggregation stays on close codes".to_string()),
                    Value::Str("MTM-plug vs per-symbol P/L Total check not run: the per-symbol table misses cash MTM and embeds Total rows; the plug is the number".to_string()),
                    Value::Str("Borrow Fee Details rows exist only where fees were charged; absent rows are not absent fees".to_string()),
                ]),
            ),
        ])
    }
}
