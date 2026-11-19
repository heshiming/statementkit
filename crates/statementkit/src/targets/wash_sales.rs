//! `wash-sales`: probable disallowed-loss flags, upper bound.
//!
//! Candidate = close row (`Code` contains `C` or `A`) with negative
//! `Realized P/L` (loss) — closes only, per the open-row artifact.
//! For each candidate, buys (`Code` contains `O`, transfers never)
//! in the same symbol within ±30 calendar days supply matched
//! quantity, consumed greedily in date order; flag = |loss| ×
//! matched ÷ closed. Output is a per-symbol list summed as an
//! **upper bound on probable disallowed loss**, never "your wash
//! sales": the authoritative amounts live on the 1099/1042-S, not
//! in statement data.
//!
//! Same-symbol only: a repurchase of the same underlying at another
//! option strike ("substantially identical") is invisible here, and
//! so are retirement-account and cross-account legs. Assignment and
//! expiry markers are bookkeeping rows — buys are the `O` codes,
//! nothing else. IB's average-cost `Realized P/L` is per row, not
//! per IRS lot; this matcher is FIFO-free by design.

use std::collections::BTreeMap;

use crate::dates;
use crate::dec::Dec;
use crate::json;
use crate::json::Value;
use crate::target::{RowRef, Target};

struct Buy {
    day: i64,
    remaining: Dec,
}

#[derive(Default)]
struct Loss {
    day: i64,
    closed: Dec,
    loss: Dec, // positive magnitude
}

#[derive(Default)]
struct SymbolState {
    buys: Vec<Buy>,
    losses: Vec<Loss>,
}

pub struct WashSales {
    syms: BTreeMap<String, SymbolState>,
    dupes: usize,
    seen: std::collections::BTreeSet<(String, String, String, String)>,
}

impl WashSales {
    pub fn new() -> Self {
        WashSales {
            syms: BTreeMap::new(),
            dupes: 0,
            seen: std::collections::BTreeSet::new(),
        }
    }
}

fn dec(raw: Option<&str>) -> Option<Dec> {
    raw.and_then(|r| Dec::parse(r).ok())
}

impl Target for WashSales {
    fn name(&self) -> &'static str {
        "wash-sales"
    }

    fn on_row(&mut self, row: &RowRef) {
        if row.title != "Trades" {
            return;
        }
        let (Some(symbol), Some(qty), Some(code)) = (
            row.cell("Symbol"),
            dec(row.cell("Quantity")),
            row.cell("Code"),
        ) else {
            return;
        };
        if code.contains("IA") || code.contains("IM") {
            return;
        }
        let key = (
            symbol.to_string(),
            row.cell("Date/Time").unwrap_or("").to_string(),
            qty.to_text(),
            row.cell("Realized P/L").unwrap_or("").to_string(),
        );
        if !self.seen.insert(key) {
            self.dupes += 1;
            return;
        }
        let Some(day) = row.cell("Date/Time").and_then(dates::date_to_days) else {
            return;
        };
        let state = self.syms.entry(symbol.to_string()).or_default();
        if code.contains('O') && qty > Dec::ZERO {
            state.buys.push(Buy {
                day,
                remaining: qty,
            });
        } else if (code.contains('C') || code.contains('A')) && qty < Dec::ZERO {
            let realized = dec(row.cell("Realized P/L"));
            if let Some(r) = realized {
                if r < Dec::ZERO {
                    state.losses.push(Loss {
                        day,
                        closed: -qty,
                        loss: -r,
                    });
                }
            }
        }
    }

    fn report(&self) -> Value {
        let mut rows: Vec<(String, Dec, Dec)> = Vec::new();
        for (symbol, st) in &self.syms {
            if st.losses.is_empty() {
                continue;
            }
            let mut buys: Vec<Buy> = st
                .buys
                .iter()
                .map(|b| Buy {
                    day: b.day,
                    remaining: b.remaining,
                })
                .collect();
            buys.sort_by_key(|b| b.day);
            let mut loss_total = Dec::ZERO;
            let mut flagged = Dec::ZERO;
            for l in &st.losses {
                loss_total = loss_total + l.loss;
                let mut need = l.closed;
                let mut matched = Dec::ZERO;
                for b in buys.iter_mut() {
                    if need <= Dec::ZERO {
                        break;
                    }
                    if (b.day - l.day).abs() > 30 || b.remaining <= Dec::ZERO {
                        continue;
                    }
                    let take = if b.remaining < need {
                        b.remaining
                    } else {
                        need
                    };
                    b.remaining = b.remaining - take;
                    need = need - take;
                    matched = matched + take;
                }
                if matched > Dec::ZERO {
                    flagged = flagged
                        + Dec::parse(&json::ratio_text(
                            l.loss.to_f64() * (matched.to_f64() / l.closed.to_f64()),
                        ))
                        .unwrap_or(Dec::ZERO);
                }
            }
            rows.push((symbol.clone(), loss_total, flagged));
        }
        rows.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
        let list: Vec<Value> = rows
            .iter()
            .map(|(symbol, loss, flagged)| {
                Value::Obj(vec![
                    ("symbol".to_string(), Value::Str(symbol.clone())),
                    ("loss".to_string(), Value::num(loss)),
                    ("flagged".to_string(), Value::num(flagged)),
                    (
                        "share".to_string(),
                        Value::Num(json::ratio_text(flagged.to_f64() / loss.to_f64())),
                    ),
                ])
            })
            .collect();
        let tot_loss: Dec = rows.iter().fold(Dec::ZERO, |a, r| a + r.1);
        let tot_flag: Dec = rows.iter().fold(Dec::ZERO, |a, r| a + r.2);
        let mut notes = vec![
            Value::Str("upper bound on probable disallowed loss, not 'your wash sales': authoritative amounts are on the 1099/1042-S".to_string()),
            Value::Str("same-symbol matching only: same-underlying other-strike repurchases are substantially-identical and invisible here".to_string()),
            Value::Str("buys are O codes only; assignment/expiry markers are bookkeeping rows; retirement and cross-account legs are out of scope".to_string()),
            Value::Str("a 100% share means continuous trading saturates the ±30d test — that is the upper bound, not a bug".to_string()),
        ];
        if self.dupes > 0 {
            notes.push(Value::Str(format!(
                "duplicate fills dropped (overlapping files): {}",
                self.dupes
            )));
        }
        Value::Obj(vec![
            (
                "symbols_flagged".to_string(),
                Value::Num(rows.iter().filter(|r| r.2 > Dec::ZERO).count().to_string()),
            ),
            (
                "loss_symbols".to_string(),
                Value::Num(rows.len().to_string()),
            ),
            ("symbols".to_string(), Value::Arr(list)),
            (
                "totals".to_string(),
                Value::Obj(vec![
                    ("loss".to_string(), Value::num(&tot_loss)),
                    ("flagged_upper_bound".to_string(), Value::num(&tot_flag)),
                ]),
            ),
            ("notes".to_string(), Value::Arr(notes)),
        ])
    }
}
