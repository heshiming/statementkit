//! `family-stated`: the family as the statements state it.
//!
//! Reads `Account Information` (`Account`, `Account Type`,
//! `Account Capabilities`, `Accounts Included`), counts
//! `Account`-tagged activity rows per section and per named
//! account, and records where the family-only blocks appear
//! (`Net Asset Value`, `Change in Combined NAV`, each
//! `Time Weighted Rate of Return` cell). No derivation — this is
//! the ground map of which numbers carry an `Account` column and
//! which exist only at family level.
//!
//! The lone `Time Weighted Rate of Return` cell is the family's;
//! the NAV blocks carry no `Account` column at all. Activity rows
//! naming a listed member (rather than the primary) are counted —
//! in the corpus they number zero, and because member funding
//! transfers are not posted either, nothing in a family statement
//! reveals how much capital stands behind a member.
//!
//! Section titles and header fields are also scanned for
//! margin/liquidity/buying-power vocabulary; those sections
//! simply do not exist in Flex exports, and the scan says so.

use std::collections::BTreeMap;

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
struct Family {
    files: usize,
    account_type: Option<String>,
    capabilities: Option<String>,
    included: Option<Vec<String>>,
    account_marker: Option<String>,
    tagged: BTreeMap<String, BTreeMap<String, usize>>,
    nav_files: usize,
    combined_nav_files: usize,
    twr_cells: usize,
}

pub struct FamilyStated {
    account_meta: Option<String>,
    info: BTreeMap<String, String>,
    families: BTreeMap<String, Family>,
    file_has_nav: bool,
    file_has_combined: bool,
    margin_like: BTreeMap<String, usize>,
    sections_seen: BTreeMap<String, usize>,
}

impl FamilyStated {
    pub fn new() -> Self {
        FamilyStated {
            account_meta: None,
            info: BTreeMap::new(),
            families: BTreeMap::new(),
            file_has_nav: false,
            file_has_combined: false,
            margin_like: BTreeMap::new(),
            sections_seen: BTreeMap::new(),
        }
    }
}

/// Strip the `(Custom Consolidated)`-style marker IB appends.
fn bare_id(raw: &str) -> String {
    raw.split_whitespace()
        .next()
        .unwrap_or(raw)
        .trim_end_matches(',')
        .to_string()
}

fn marker_of(raw: &str) -> Option<String> {
    let start = raw.find('(')?;
    let end = raw.rfind(')')?;
    if end > start {
        Some(raw[start..=end].to_string())
    } else {
        None
    }
}

fn margin_like_word(text: &str) -> bool {
    let t = text.to_lowercase();
    t.contains("margin")
        || t.contains("liquidity")
        || t.replace([' ', '_'], "-").contains("buying-power")
}

impl FamilyStated {
    fn family_for(&mut self, id: &str) -> &mut Family {
        self.families.entry(id.to_string()).or_default()
    }
}

impl Target for FamilyStated {
    fn name(&self) -> &'static str {
        "family-stated"
    }

    fn on_file_begin(&mut self) {
        self.account_meta = None;
        self.info.clear();
        self.file_has_nav = false;
        self.file_has_combined = false;
    }

    fn on_meta(&mut self, key: &str, value: &str) {
        if key == "Account ID" {
            self.account_meta = Some(bare_id(value));
        }
    }

    fn on_section(&mut self, title: &str, fields: &[String]) {
        *self.sections_seen.entry(title.to_string()).or_default() += 1;
        if margin_like_word(title) || fields.iter().any(|f| margin_like_word(f)) {
            *self.margin_like.entry(title.to_string()).or_default() += 1;
        }
    }

    fn on_row(&mut self, row: &RowRef) {
        match row.title {
            "Account Information" => {
                if let (Some(field), Some(value)) = (row.cell("Field"), row.cell("Value")) {
                    self.info.insert(field.to_string(), value.to_string());
                }
            }
            "Net Asset Value" => self.file_has_nav = true,
            "Change in Combined NAV" => self.file_has_combined = true,
            _ => {}
        }
        if row
            .fields
            .iter()
            .any(|f| f == "Time Weighted Rate of Return")
            && row
                .cell("Time Weighted Rate of Return")
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false)
        {
            let f = self
                .account_meta
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            self.family_for(&f).twr_cells += 1;
        }
        if TAGGED.contains(&row.title) {
            if let Some(acct) = row.cell("Account") {
                let f = self
                    .account_meta
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string());
                let fam = self.family_for(&f);
                *fam.tagged
                    .entry(bare_id(acct))
                    .or_default()
                    .entry(row.title.to_string())
                    .or_default() += 1;
            }
        }
    }

    fn on_file_end(&mut self) {
        let stated = self.info.get("Account").cloned();
        let id = stated
            .as_deref()
            .map(bare_id)
            .or_else(|| self.account_meta.clone())
            .unwrap_or_else(|| "unknown".to_string());
        let acc_type = self.info.get("Account Type").cloned();
        let caps = self.info.get("Account Capabilities").cloned();
        let included = self.info.get("Accounts Included").cloned();
        let has_nav = self.file_has_nav;
        let has_combined = self.file_has_combined;
        let fam = self.family_for(&id);
        fam.files += 1;
        if let Some(v) = acc_type {
            fam.account_type = Some(v);
        }
        if let Some(v) = caps {
            fam.capabilities = Some(v);
        }
        if let Some(v) = &stated {
            fam.account_marker = marker_of(v);
        }
        if let Some(v) = included {
            let list: Vec<String> = v
                .split(',')
                .map(|s| bare_id(s.trim()))
                .filter(|s| !s.is_empty())
                .collect();
            match &fam.included {
                Some(cur) => {
                    if cur.len() < list.len() {
                        fam.included = Some(list);
                    }
                }
                None => fam.included = Some(list),
            }
        }
        if has_nav {
            fam.nav_files += 1;
        }
        if has_combined {
            fam.combined_nav_files += 1;
        }
    }

    fn report(&self) -> Value {
        let mut list: Vec<Value> = Vec::new();
        for (id, fam) in &self.families {
            let members: Option<Vec<String>> = fam
                .included
                .as_ref()
                .map(|inc| inc.iter().filter(|a| *a != id).cloned().collect());
            let mut member_named = 0usize;
            if let Some(m) = &members {
                for (acct, secs) in &fam.tagged {
                    if m.contains(&bare_id(acct)) {
                        member_named += secs.values().sum::<usize>();
                    }
                }
            }
            let mut section_totals: BTreeMap<String, usize> = BTreeMap::new();
            let by_account: Vec<Value> = fam
                .tagged
                .iter()
                .map(|(acct, secs)| {
                    let secs_v: Vec<Value> = secs
                        .iter()
                        .map(|(s, c)| {
                            *section_totals.entry(s.clone()).or_default() += c;
                            (s.clone(), Value::Num(c.to_string()))
                        })
                        .map(|(s, n)| {
                            Value::Obj(vec![
                                ("section".to_string(), Value::Str(s)),
                                ("rows".to_string(), n),
                            ])
                        })
                        .collect();
                    Value::Obj(vec![
                        ("account".to_string(), Value::Str(acct.clone())),
                        ("sections".to_string(), Value::Arr(secs_v)),
                    ])
                })
                .collect();
            let totals: Vec<Value> = section_totals
                .iter()
                .map(|(s, c)| {
                    Value::Obj(vec![
                        ("section".to_string(), Value::Str(s.clone())),
                        ("rows".to_string(), Value::Num(c.to_string())),
                    ])
                })
                .collect();
            list.push(Value::Obj(vec![
                ("account".to_string(), Value::Str(id.clone())),
                (
                    "marker".to_string(),
                    fam.account_marker.clone().map_or(Value::Null, Value::Str),
                ),
                (
                    "account_type".to_string(),
                    fam.account_type.clone().map_or(Value::Null, Value::Str),
                ),
                (
                    "capabilities".to_string(),
                    fam.capabilities.clone().map_or(Value::Null, Value::Str),
                ),
                (
                    "members".to_string(),
                    members
                        .map(|m| Value::Arr(m.into_iter().map(Value::Str).collect()))
                        .unwrap_or(Value::Null),
                ),
                ("files".to_string(), Value::Num(fam.files.to_string())),
                (
                    "nav_files".to_string(),
                    Value::Num(fam.nav_files.to_string()),
                ),
                (
                    "combined_nav_files".to_string(),
                    Value::Num(fam.combined_nav_files.to_string()),
                ),
                (
                    "twr_cells".to_string(),
                    Value::Num(fam.twr_cells.to_string()),
                ),
                ("tagged_rows".to_string(), Value::Arr(totals)),
                ("tagged_by_account".to_string(), Value::Arr(by_account)),
                (
                    "member_named_rows".to_string(),
                    Value::Num(member_named.to_string()),
                ),
            ]));
        }
        let margin_sections: Vec<Value> =
            self.margin_like.keys().cloned().map(Value::Str).collect();
        let mut notes = vec![
            Value::Str("Time Weighted Rate of Return cells are the family's: Net Asset Value and Change in Combined NAV carry no Account column".to_string()),
            Value::Str("member funding transfers are not posted in family statements: nothing here shows capital behind a member".to_string()),
            Value::Str("row counts sum every fed file; overlapping periods double-count rows".to_string()),
        ];
        if self.families.values().all(|f| f.included.is_none()) {
            notes.push(Value::Str(
                "no Accounts Included line in any fed file".to_string(),
            ));
        }
        Value::Obj(vec![
            ("families".to_string(), Value::Arr(list)),
            (
                "sections_scanned".to_string(),
                Value::Num(self.sections_seen.values().sum::<usize>().to_string()),
            ),
            (
                "margin_like_sections".to_string(),
                Value::Arr(margin_sections),
            ),
            ("notes".to_string(), Value::Arr(notes)),
        ])
    }
}
