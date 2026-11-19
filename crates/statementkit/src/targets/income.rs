//! `income`: income statement by posting, account-year.
//!
//! Rows are IB's posted USD at posting grain; product buckets live in
//! `Description`. The accrual grain (`Change in Dividend Accruals`,
//! `Interest Accruals`) is a separate record set, reported beside the
//! postings and labeled, never merged into them.
//!
//! - `Dividends` / `Withholding Tax` sections: `Amount` rows.
//! - `Interest` rows bucketed by description: credit interest, debit
//!   interest, borrow fees, SYEP, insured deposit credit.
//! - `Trades` commissions (`Comm in USD`, else `Comm/Fee`).
//! - `Borrow Fee Details` rows counted: rows exist only where fees
//!   were charged; the fee itself stays in the borrow ledger.
//! - SYEP customer amounts captured wherever posted.
//!
//! Yield-on-cost (dividends ÷ `Cost Basis`) is not computed: dividends
//! are flows over the period while Cost Basis is one snapshot date;
//! the ratio is indicative at best.

use std::collections::BTreeMap;

use crate::dates;
use crate::dec::Dec;
use crate::json::Value;
use crate::target::{RowRef, Target};

#[derive(Default)]
struct Book {
    dividends: Dec,
    credit: Dec,
    debit: Dec,
    borrow_fees: Dec,
    syep: Dec,
    insured: Dec,
    borrow_rows: usize,
    withholding: Dec,
    commissions: Dec,
}

#[derive(Default)]
struct Accruals {
    div_gross: Dec,
    div_tax: Dec,
    div_net: Dec,
    int_paid: Dec,
    int_received: Dec,
}

pub struct Income {
    books: BTreeMap<(String, String), Book>,
    accruals: Accruals,
    file_account: String,
}

impl Income {
    pub fn new() -> Self {
        Income {
            books: BTreeMap::new(),
            accruals: Accruals::default(),
            file_account: String::new(),
        }
    }
}

fn dec(raw: Option<&str>) -> Option<Dec> {
    raw.and_then(|r| Dec::parse(r).ok())
}

fn bucket(desc: &str) -> fn(&mut Book, Dec) {
    let d = desc.to_ascii_lowercase();
    if d.contains("borrow") {
        |b: &mut Book, v| b.borrow_fees = b.borrow_fees + v
    } else if d.contains("syep") || d.contains("yield enhancement") {
        |b: &mut Book, v| b.syep = b.syep + v
    } else if d.contains("insured") {
        |b: &mut Book, v| b.insured = b.insured + v
    } else if d.contains("debit") {
        |b: &mut Book, v| b.debit = b.debit + v
    } else {
        |b: &mut Book, v| b.credit = b.credit + v
    }
}

impl Target for Income {
    fn name(&self) -> &'static str {
        "income"
    }

    fn on_meta(&mut self, key: &str, value: &str) {
        if key == "Account ID" {
            self.file_account = value.trim().to_string();
        }
    }

    fn on_row(&mut self, row: &RowRef) {
        let year = row
            .cell("Date")
            .or_else(|| row.cell("Date/Time"))
            .and_then(dates::year)
            .unwrap_or_else(|| String::from("????"));
        let account = row
            .cell("Account")
            .filter(|a| !a.trim().is_empty())
            .unwrap_or(&self.file_account)
            .to_string();
        let key = (account, year);
        match row.title {
            "Dividends" => {
                if let Some(v) = dec(row.cell("Amount")) {
                    let b = self.books.entry(key.clone()).or_default();
                    b.dividends = b.dividends + v;
                }
            }
            "Withholding Tax" => {
                if let Some(v) = dec(row.cell("Amount")) {
                    let b = self.books.entry(key.clone()).or_default();
                    b.withholding = b.withholding + v;
                }
            }
            "Interest" => {
                let (Some(v), desc) = (
                    dec(row.cell("Amount")),
                    row.cell("Description").unwrap_or(""),
                ) else {
                    return;
                };
                bucket(desc)(self.books.entry(key.clone()).or_default(), v);
            }
            "Trades" => {
                if let Some(v) = dec(row.cell("Comm in USD")).or_else(|| dec(row.cell("Comm/Fee")))
                {
                    let b = self.books.entry(key.clone()).or_default();
                    b.commissions = b.commissions + v;
                }
            }
            "Borrow Fee Details" => {
                if row.cell("Symbol").map(|s| s.trim() != "Total") == Some(true) {
                    self.books.entry(key.clone()).or_default().borrow_rows += 1;
                }
            }
            "Change in Dividend Accruals" => {
                let a = &mut self.accruals;
                if let Some(v) = dec(row.cell("Gross Amount")) {
                    a.div_gross = a.div_gross + v;
                }
                if let Some(v) = dec(row.cell("Tax")) {
                    a.div_tax = a.div_tax + v;
                }
                if let Some(v) = dec(row.cell("Net Amount")) {
                    a.div_net = a.div_net + v;
                }
            }
            "Interest Accruals" => {
                let a = &mut self.accruals;
                if let Some(v) = dec(row.cell("Accrued Interest Paid")) {
                    a.int_paid = a.int_paid + v;
                }
                if let Some(v) = dec(row.cell("Accrued Interest Received")) {
                    a.int_received = a.int_received + v;
                }
            }
            _ => {
                if row.title.contains("Yield Enhancement Program") {
                    let v = dec(row.cell("Interest Paid to Customer")).unwrap_or(Dec::ZERO)
                        + dec(row.cell("Fee Earned by Customer")).unwrap_or(Dec::ZERO)
                        + dec(row.cell("SYEP Fee Earned by Customer")).unwrap_or(Dec::ZERO);
                    if v != Dec::ZERO {
                        let b = self.books.entry(key.clone()).or_default();
                        b.syep = b.syep + v;
                    }
                }
            }
        }
    }

    fn report(&self) -> Value {
        let rows: Vec<Value> = self
            .books
            .iter()
            .map(|((account, year), b)| {
                Value::Obj(vec![
                    ("account".to_string(), Value::Str(account.clone())),
                    ("year".to_string(), Value::Str(year.clone())),
                    ("dividends".to_string(), Value::num(&b.dividends)),
                    ("credit_interest".to_string(), Value::num(&b.credit)),
                    ("debit_interest".to_string(), Value::num(&b.debit)),
                    ("borrow_fees".to_string(), Value::num(&b.borrow_fees)),
                    ("syep".to_string(), Value::num(&b.syep)),
                    ("insured_deposit".to_string(), Value::num(&b.insured)),
                    (
                        "borrow_fee_rows".to_string(),
                        Value::Num(b.borrow_rows.to_string()),
                    ),
                    ("withholding".to_string(), Value::num(&b.withholding)),
                    ("commissions".to_string(), Value::num(&b.commissions)),
                ])
            })
            .collect();
        let a = &self.accruals;
        Value::Obj(vec![
            ("account_years".to_string(), Value::Arr(rows)),
            (
                "accruals".to_string(),
                Value::Obj(vec![
                    ("dividend_gross".to_string(), Value::num(&a.div_gross)),
                    ("dividend_tax".to_string(), Value::num(&a.div_tax)),
                    ("dividend_net".to_string(), Value::num(&a.div_net)),
                    (
                        "interest_accrued_paid".to_string(),
                        Value::num(&a.int_paid),
                    ),
                    (
                        "interest_accrued_received".to_string(),
                        Value::num(&a.int_received),
                    ),
                ]),
            ),
            (
                "notes".to_string(),
                Value::Arr(vec![
                    Value::Str("rows are IB's posted USD at posting grain; accruals are a separate record set, never merged".to_string()),
                    Value::Str("SYEP customer interest and fee captured wherever posted".to_string()),
                    Value::Str("yield-on-cost not computed: dividends flow over the period while Cost Basis is one snapshot date".to_string()),
                ]),
            ),
        ])
    }
}
