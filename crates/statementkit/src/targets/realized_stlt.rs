//! `realized-stlt`: short-term vs long-term realized, IB's shipped
//! answer.
//!
//! `Realized & Unrealized Performance Summary` columns `Realized S/T
//! Profit | S/T Loss | L/T Profit | L/T Loss` and the `Unrealized
//! S/T * | L/T *` columns, summed per file's account-year. The
//! holding-period grain is IB's own: lot granularity is NOT in the
//! file and IB's holding-period convention governs, not the reader's
//! tax jurisdiction. Never rebuild these from `Trades` dates — FIFO
//! holds are approximations. The embedded `Total` row is excluded
//! before summing. Annual templates additionally carry per-symbol
//! `Month & Year to Date Performance Summary` for the same read at
//! month grain; not parsed here.

use crate::dates;
use crate::dec::Dec;
use crate::json::Value;
use crate::target::{RowRef, Target};

#[derive(Default)]
struct Year {
    st_profit: Dec,
    st_loss: Dec,
    lt_profit: Dec,
    lt_loss: Dec,
    u_st_profit: Dec,
    u_st_loss: Dec,
    u_lt_profit: Dec,
    u_lt_loss: Dec,
    symbol_rows: usize,
}

pub struct RealizedStlt {
    years: Vec<(String, String, Year)>,
    file_account: String,
    file_year: String,
}

impl RealizedStlt {
    pub fn new() -> Self {
        RealizedStlt {
            years: Vec::new(),
            file_account: String::new(),
            file_year: String::from("????"),
        }
    }
}

fn dec(raw: Option<&str>) -> Option<Dec> {
    raw.and_then(|r| Dec::parse(r).ok())
}

impl Target for RealizedStlt {
    fn name(&self) -> &'static str {
        "realized-stlt"
    }

    fn on_meta(&mut self, key: &str, value: &str) {
        match key {
            "Account ID" => self.file_account = value.trim().to_string(),
            "Period" => {
                self.file_year = dates::stated_period(value)
                    .and_then(|(_, end, _)| end)
                    .and_then(|end| dates::year(&end))
                    .unwrap_or_else(|| String::from("????"));
            }
            _ => {}
        }
    }

    fn on_section(&mut self, title: &str, _fields: &[String]) {
        if title == "Realized & Unrealized Performance Summary" {
            let key = (self.file_account.clone(), self.file_year.clone());
            if !self
                .years
                .iter()
                .any(|(a, y, _)| a == &key.0 && y == &key.1)
            {
                self.years.push((key.0, key.1, Year::default()));
            }
        }
    }

    fn on_row(&mut self, row: &RowRef) {
        if row.title != "Realized & Unrealized Performance Summary" {
            return;
        }
        if row.cell("Symbol").map(|s| s.trim() == "Total") != Some(false) {
            return;
        }
        let Some((_, _, y)) = self.years.iter_mut().find(|(a, b, _)| {
            a.as_str() == self.file_account.as_str() && b.as_str() == self.file_year.as_str()
        }) else {
            return;
        };
        let add = |slot: &mut Dec, col: &str| {
            if let Some(v) = dec(row.cell(col)) {
                *slot = *slot + v;
            }
        };
        add(&mut y.st_profit, "Realized S/T Profit");
        add(&mut y.st_loss, "Realized S/T Loss");
        add(&mut y.lt_profit, "Realized L/T Profit");
        add(&mut y.lt_loss, "Realized L/T Loss");
        add(&mut y.u_st_profit, "Unrealized S/T Profit");
        add(&mut y.u_st_loss, "Unrealized S/T Loss");
        add(&mut y.u_lt_profit, "Unrealized L/T Profit");
        add(&mut y.u_lt_loss, "Unrealized L/T Loss");
        y.symbol_rows += 1;
    }

    fn report(&self) -> Value {
        let rows: Vec<Value> = self
            .years
            .iter()
            .map(|(account, year, y)| {
                let realized_total = y.st_profit + y.st_loss + y.lt_profit + y.lt_loss;
                let mut obj = vec![
                    ("account".to_string(), Value::Str(account.clone())),
                    ("year".to_string(), Value::Str(year.clone())),
                    ("st_profit".to_string(), Value::num(&y.st_profit)),
                    ("st_loss".to_string(), Value::num(&y.st_loss)),
                    ("lt_profit".to_string(), Value::num(&y.lt_profit)),
                    ("lt_loss".to_string(), Value::num(&y.lt_loss)),
                    ("realized_total".to_string(), Value::num(&realized_total)),
                ];
                if y.u_st_profit != Dec::ZERO
                    || y.u_st_loss != Dec::ZERO
                    || y.u_lt_profit != Dec::ZERO
                    || y.u_lt_loss != Dec::ZERO
                {
                    obj.extend([
                        ("unreal_st_profit".to_string(), Value::num(&y.u_st_profit)),
                        ("unreal_st_loss".to_string(), Value::num(&y.u_st_loss)),
                        ("unreal_lt_profit".to_string(), Value::num(&y.u_lt_profit)),
                        ("unreal_lt_loss".to_string(), Value::num(&y.u_lt_loss)),
                    ]);
                }
                obj.push((
                    "symbol_rows".to_string(),
                    Value::Num(y.symbol_rows.to_string()),
                ));
                Value::Obj(obj)
            })
            .collect();
        Value::Obj(vec![
            ("account_years".to_string(), Value::Arr(rows)),
            (
                "notes".to_string(),
                Value::Arr(vec![
                    Value::Str("IB's shipped S/T-L/T grain, symbol-aggregated, under IB's holding-period convention; lot granularity is not in the file".to_string()),
                    Value::Str("not recomputed from Trades dates: FIFO holds are approximations".to_string()),
                    Value::Str("annual templates also carry per-symbol Month & Year to Date Performance Summary for the same read at month grain".to_string()),
                ]),
            ),
        ])
    }
}
