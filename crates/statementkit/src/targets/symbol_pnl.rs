//! `symbol-pnl`: per-symbol all-in P&L, latest snapshot view.
//!
//! Per symbol: `Realized Total` (latest `Realized & Unrealized
//! Performance Summary`) + Δ`Unrealized Total` (this period-end
//! summary minus the prior one — the file ships snapshots, so the
//! change is the period's unrealized contribution) + `Dividends`
//! (postings plus per-symbol accrual net rows) − `Borrow Fee`.
//! The embedded `Total` rows are excluded before any sum.
//!
//! Traps respected: `Unrealized Total` is a level, never a flow —
//! with one snapshot only, Δ stays 0. Δ spans opens/closes AND price
//! drift, so a closed position's whole mark move sits in Δ while
//! `realized` also books it once: only the `all_in` column is
//! additive-safe. Borrow fees are a period accrual kept in their own
//! column, never charged into realized. Commissions are already
//! inside `Realized Total` — `Comm/Fee` is never added again.
//! Option symbols roll up under their root for the x-ray, not here.

use std::collections::BTreeMap;

use crate::dec::Dec;
use crate::json::Value;
use crate::target::{RowRef, Target};

#[derive(Default)]
struct Sym {
    dividends: Dec,
    borrow_fee: Dec,
}

pub struct SymbolPnl {
    snapshots: Vec<BTreeMap<String, (Dec, Dec)>>, // per file: symbol -> (realized, unrealized)
    cur_snapshot: Option<BTreeMap<String, (Dec, Dec)>>,
    syms: BTreeMap<String, Sym>,
}

impl SymbolPnl {
    pub fn new() -> Self {
        SymbolPnl {
            snapshots: Vec::new(),
            cur_snapshot: None,
            syms: BTreeMap::new(),
        }
    }
}

fn dec(raw: Option<&str>) -> Option<Dec> {
    raw.and_then(|r| Dec::parse(r).ok())
}

impl Target for SymbolPnl {
    fn name(&self) -> &'static str {
        "symbol-pnl"
    }

    fn on_file_end(&mut self) {
        if let Some(snap) = self.cur_snapshot.take() {
            self.snapshots.push(snap);
        }
    }

    fn on_row(&mut self, row: &RowRef) {
        match row.title {
            "Realized & Unrealized Performance Summary" => {
                let Some(symbol) = row.cell("Symbol").filter(|s| s.trim() != "Total") else {
                    return;
                };
                let snap = self.cur_snapshot.get_or_insert_default();
                snap.insert(
                    symbol.to_string(),
                    (
                        dec(row.cell("Realized Total")).unwrap_or(Dec::ZERO),
                        dec(row.cell("Unrealized Total")).unwrap_or(Dec::ZERO),
                    ),
                );
            }
            "Dividends" => {
                let (Some(symbol), Some(v)) = (row.cell("Symbol"), dec(row.cell("Amount"))) else {
                    return;
                };
                let s = self.syms.entry(symbol.to_string()).or_default();
                s.dividends = s.dividends + v;
            }
            "Change in Dividend Accruals" => {
                let (Some(symbol), Some(v)) = (row.cell("Symbol"), dec(row.cell("Net Amount")))
                else {
                    return;
                };
                let s = self.syms.entry(symbol.to_string()).or_default();
                s.dividends = s.dividends + v;
            }
            "Borrow Fee Details" => {
                let (Some(symbol), Some(v)) = (
                    row.cell("Symbol").filter(|s| s.trim() != "Total"),
                    dec(row.cell("Borrow Fee")),
                ) else {
                    return;
                };
                let s = self.syms.entry(symbol.to_string()).or_default();
                s.borrow_fee = s.borrow_fee + v;
            }
            _ => {}
        }
    }

    fn report(&self) -> Value {
        let Some(latest) = self.snapshots.last() else {
            return Value::Obj(vec![
                ("symbols".to_string(), Value::Arr(vec![])),
                (
                    "notes".to_string(),
                    Value::Arr(vec![Value::Str(
                        "no Realized & Unrealized Performance Summary in any file".to_string(),
                    )]),
                ),
            ]);
        };
        let prev: Option<&BTreeMap<String, (Dec, Dec)>> = if self.snapshots.len() > 1 {
            self.snapshots.get(self.snapshots.len() - 2)
        } else {
            None
        };
        let differencing = prev.is_some();
        let mut keys: Vec<&String> = latest.keys().collect();
        for k in self.syms.keys() {
            if !latest.contains_key(k) {
                keys.push(k);
            }
        }
        let empty = Sym::default();
        let mut rows: Vec<(String, Dec, Dec, Dec, Dec, Dec)> = Vec::new();
        for symbol in keys {
            let (realized, unreal) = latest
                .get(symbol)
                .copied()
                .unwrap_or((Dec::ZERO, Dec::ZERO));
            let delta = match prev {
                Some(p) => unreal - p.get(symbol).map(|(_, u)| *u).unwrap_or(Dec::ZERO),
                None => Dec::ZERO,
            };
            let s = self.syms.get(symbol).unwrap_or(&empty);
            let fee = -s.borrow_fee;
            let all_in = realized + delta + s.dividends + fee;
            rows.push((symbol.clone(), realized, delta, s.dividends, fee, all_in));
        }
        rows.sort_by(|a, b| b.5.cmp(&a.5).then_with(|| a.0.cmp(&b.0)));
        let row = |r: &(String, Dec, Dec, Dec, Dec, Dec)| {
            Value::Obj(vec![
                ("symbol".to_string(), Value::Str(r.0.clone())),
                ("realized".to_string(), Value::num(&r.1)),
                ("delta_unrealized".to_string(), Value::num(&r.2)),
                ("dividends".to_string(), Value::num(&r.3)),
                ("borrow_fee".to_string(), Value::num(&r.4)),
                ("all_in".to_string(), Value::num(&r.5)),
            ])
        };
        let symbols: Vec<Value> = rows.iter().map(row).collect();
        let mut tot = (
            String::from("TOTAL"),
            Dec::ZERO,
            Dec::ZERO,
            Dec::ZERO,
            Dec::ZERO,
            Dec::ZERO,
        );
        for r in &rows {
            tot.1 = tot.1 + r.1;
            tot.2 = tot.2 + r.2;
            tot.3 = tot.3 + r.3;
            tot.4 = tot.4 + r.4;
            tot.5 = tot.5 + r.5;
        }
        let mut notes = vec![
            Value::Str("only the all_in column is additive-safe: delta_unrealized spans opens/closes and price drift, so a closed position's mark move sits in both columns".to_string()),
            Value::Str("borrow fee kept as its own column (period accrual), never charged into realized; commissions already inside Realized Total, never added again".to_string()),
            Value::Str("symbols absent from the latest summary contribute nothing; option roots not rolled up (x-ray view, not this table)".to_string()),
        ];
        if !differencing {
            notes.push(Value::Str(
                "one snapshot only: delta_unrealized is 0; Δ needs two period-end statements"
                    .to_string(),
            ));
        }
        Value::Obj(vec![
            (
                "snapshots".to_string(),
                Value::Num(self.snapshots.len().to_string()),
            ),
            ("symbols".to_string(), Value::Arr(symbols)),
            ("total".to_string(), row(&tot)),
            ("notes".to_string(), Value::Arr(notes)),
        ])
    }
}
