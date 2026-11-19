//! `family-detected`: is this a family statement — five signals,
//! string level, no interpretation.
//!
//! Per fed file:
//! 1. combined marker — `Account Information` `Account` value ends
//!    with `(Custom Consolidated)`;
//! 2. legs — count of `Accounts Included` entries (1 when absent);
//! 3. hierarchy — `Master Name` present, `Account Type` value
//!    (`Institution Master` is the prop-group shape);
//! 4. NAV twin — the raw Header-row tag `Change in Combined NAV`
//!    beside data rows tagged `Change in NAV`; a single-account
//!    file carries only the latter. The session aliases NAV data
//!    rows into the combined block, so only the raw Header-row tag
//!    (delivered to `on_section`) shows the distinction;
//! 5. attribution — an `Account` column exists on activity
//!    sections.
//!
//! Plus `member_rows` (activity rows naming a listed member —
//! zero across IB's consolidated exports) and `combine_reason`,
//! the Disclaimer's own framing, classified not quoted.
//!
//! Detection sees the group, never the arrangement: `legs=2,
//! member_rows=0` is a combined report whose member never trades,
//! and a funded trading sub would print the same shape with
//! member rows > 0. The word "family" appears nowhere in any file.

use crate::json::Value;
use crate::target::{RowRef, Target};

const TAGGED: &[&str] = &[
    "Trades",
    "Interest",
    "Dividends",
    "Borrow Fee Details",
    "Change in Dividend Accruals",
    "Deposits & Withdrawals",
    "Transfers",
    "Corporate Actions",
    "Option Cash Settlements",
    "Withholding Tax",
];

#[derive(Default)]
struct FileDet {
    account: String,
    marker: bool,
    legs: usize,
    included: Vec<String>,
    master_name: bool,
    account_type: Option<String>,
    combined_tag: bool,
    nav_tag: bool,
    account_col_sections: usize,
    tagged_accounts: Vec<String>,
    combined_stated: bool,
    digital_asset: bool,
}

fn bare_id(raw: &str) -> String {
    raw.split_whitespace()
        .next()
        .unwrap_or(raw)
        .trim_end_matches(',')
        .to_string()
}

pub struct FamilyDetected {
    file: FileDet,
    files: Vec<FileDet>,
}

impl FamilyDetected {
    pub fn new() -> Self {
        FamilyDetected {
            file: FileDet::default(),
            files: Vec::new(),
        }
    }
}

impl Target for FamilyDetected {
    fn name(&self) -> &'static str {
        "family-detected"
    }

    fn on_meta(&mut self, key: &str, value: &str) {
        if key == "Account ID" {
            self.file.account = bare_id(value);
        }
    }

    fn on_section(&mut self, title: &str, fields: &[String]) {
        match title {
            "Change in Combined NAV" => self.file.combined_tag = true,
            "Change in NAV" => self.file.nav_tag = true,
            _ => {}
        }
        if TAGGED.contains(&title) && fields.iter().any(|f| f == "Account") {
            self.file.account_col_sections += 1;
        }
    }

    fn on_row(&mut self, row: &RowRef) {
        match row.title {
            "Account Information" => {
                let (Some(field), Some(value)) = (row.cell("Field"), row.cell("Value")) else {
                    return;
                };
                match field {
                    "Account" => {
                        self.file.marker = value.contains("(Custom Consolidated)");
                        if self.file.account.is_empty() {
                            self.file.account = bare_id(value);
                        }
                    }
                    "Account Type" => self.file.account_type = Some(value.to_string()),
                    "Master Name" => self.file.master_name = true,
                    "Accounts Included" => {
                        let list: Vec<String> = value
                            .split(',')
                            .map(|s| bare_id(s.trim()))
                            .filter(|s| !s.is_empty())
                            .collect();
                        if list.len() > self.file.legs {
                            self.file.legs = list.len();
                            self.file.included = list;
                        }
                    }
                    _ => {}
                }
            }
            "Disclaimer" => {
                let joined = row.cells.join(" ").to_lowercase();
                if joined.contains("combined statement") {
                    self.file.combined_stated = true;
                }
                if joined.contains("digital asset") {
                    self.file.digital_asset = true;
                }
            }
            _ => {}
        }
        if TAGGED.contains(&row.title) {
            if let Some(acct) = row.cell("Account").map(bare_id) {
                self.file.tagged_accounts.push(acct);
            }
        }
    }

    fn on_file_end(&mut self) {
        let mut f = std::mem::take(&mut self.file);
        if f.legs == 0 {
            f.legs = 1;
        }
        let members: Vec<String> = f
            .included
            .iter()
            .filter(|a| **a != f.account)
            .cloned()
            .collect();
        f.tagged_accounts.retain(|a| members.contains(a));
        self.files.push(f);
    }

    fn report(&self) -> Value {
        let mut detections: Vec<Value> = Vec::new();
        let mut marker_files = 0usize;
        let mut twin_files = 0usize;
        let mut nav_only = 0usize;
        let mut member_rows_total = 0usize;
        for f in &self.files {
            let member_rows = f.tagged_accounts.len();
            member_rows_total += member_rows;
            if f.marker {
                marker_files += 1;
            }
            if f.combined_tag {
                twin_files += 1;
            }
            if f.nav_tag && !f.combined_tag {
                nav_only += 1;
            }
            let combine_reason = if f.digital_asset {
                "securities + digital-asset accounts"
            } else if f.combined_stated {
                "combined statement (framing not classified)"
            } else {
                "no combined-statement disclaimer"
            };
            detections.push(Value::Obj(vec![
                ("account".to_string(), Value::Str(f.account.clone())),
                ("combined_marker".to_string(), Value::Bool(f.marker)),
                ("legs".to_string(), Value::Num(f.legs.to_string())),
                ("master_name".to_string(), Value::Bool(f.master_name)),
                (
                    "account_type".to_string(),
                    f.account_type.clone().map_or(Value::Null, Value::Str),
                ),
                (
                    "nav_twin".to_string(),
                    Value::Str(if f.combined_tag {
                        "twin".to_string()
                    } else if f.nav_tag {
                        "nav-only".to_string()
                    } else {
                        "no-nav-tag".to_string()
                    }),
                ),
                (
                    "account_column_sections".to_string(),
                    Value::Num(f.account_col_sections.to_string()),
                ),
                (
                    "member_rows".to_string(),
                    Value::Num(member_rows.to_string()),
                ),
                (
                    "combine_reason".to_string(),
                    Value::Str(combine_reason.to_string()),
                ),
            ]));
        }
        Value::Obj(vec![
            ("detections".to_string(), Value::Arr(detections)),
            (
                "corpus".to_string(),
                Value::Obj(vec![
                    (
                        "files_scanned".to_string(),
                        Value::Num(self.files.len().to_string()),
                    ),
                    (
                        "combined_marker_files".to_string(),
                        Value::Num(marker_files.to_string()),
                    ),
                    (
                        "nav_twin_files".to_string(),
                        Value::Num(twin_files.to_string()),
                    ),
                    (
                        "nav_only_files".to_string(),
                        Value::Num(nav_only.to_string()),
                    ),
                    (
                        "member_rows_total".to_string(),
                        Value::Num(member_rows_total.to_string()),
                    ),
                ]),
            ),
            (
                "notes".to_string(),
                Value::Arr(vec![
                    Value::Str("detection sees the group, never the arrangement: legs counted are not legs trading".to_string()),
                    Value::Str("the word family appears in no file; combine_reason is the Disclaimer's own framing, classified not quoted".to_string()),
                    Value::Str("NAV data rows are aliased into the combined block by the parser; only the raw Header-row tag shows twin vs nav-only".to_string()),
                ]),
            ),
        ])
    }
}
