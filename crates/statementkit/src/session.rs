//! Streaming session: byte chunks of vertical-layout CSVs in, per-file
//! structural summaries and target reports out. Memory is bounded by
//! the largest single record plus the section list and the selected
//! targets' accumulators; file size is not held.
//!
//! Layout: every row is `[SectionName, RowType, cells..]`. A `Header`
//! row names a section's fields; `Data`/`Row` rows join the active
//! section. `Statement` rows carry key/value metadata. The one known
//! tag alias (`Change in NAV` rows under the `Change in Combined NAV`
//! header) is retagged. Rows join the active section by position, as
//! the reference reader does.

use crate::csv::Csv;
use crate::dec::Dec;
use crate::json::Value;
use crate::target::{self, RowRef, Target};
use crate::tickers::{root_up, ticker_prefix};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Structure of one file fed to a [`Session`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FileSummary {
    /// `Statement` key/value pairs, in first-seen order, last value kept.
    pub meta: Vec<(String, String)>,
    /// Sections in order of appearance; a repeated title appears twice.
    pub sections: Vec<SectionSummary>,
    /// Data rows counted into a section.
    pub data_rows: usize,
    /// Rows with fewer than two fields.
    pub short_rows: usize,
    /// Data rows with no section opened yet.
    pub orphan_rows: usize,
    /// Rows whose row type is neither `Header`, `Data` nor `Row`.
    pub other_rows: usize,
    /// CSV records dropped as malformed.
    pub bad_rows: usize,
}

/// One section: its title, header fields and data row count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionSummary {
    pub title: String,
    pub fields: Vec<String>,
    pub rows: usize,
}

/// Active ticker selection: rows keyed to a selected root are
/// dispatched, rows keyed to any other root are dropped, rows
/// with no key pass through as a named ticker-less residual.
/// Per section the dollars follow the same three-way partition,
/// exact by construction.
#[derive(Debug, Default)]
struct SecStat {
    matched: usize,
    unattributed: usize,
    dropped: usize,
    matched_usd: Dec,
    unattributed_usd: Dec,
    dropped_usd: Dec,
    non_usd: usize,
}

#[derive(Debug, Default)]
struct Selection {
    roots: BTreeSet<String>,
    /// Selected roots that have not matched a row yet.
    unmatched: BTreeSet<String>,
    stats: BTreeMap<String, SecStat>,
    /// Current file's base currency, from its `Statement` meta.
    base_ccy: String,
}

/// Money columns per section, for the partition dollars.
fn money_cols(title: &str) -> &'static [&'static str] {
    match title {
        "Trades" => &["Realized P/L", "Comm/Fee"],
        "Dividends"
        | "Interest"
        | "Withholding Tax"
        | "Deposits & Withdrawals"
        | "Option Cash Settlements"
        | "Corporate Actions" => &["Amount"],
        "Borrow Fee Details" => &["Borrow Fee"],
        "Change in Dividend Accruals" => &["Accrual Amount"],
        _ => &[],
    }
}

/// The row's money in base-currency units, or `None` when the
/// section carries none; `Some(None)` marks a non-base row.
fn row_money(
    title: &str,
    fields: &[String],
    cells: &[String],
    base_ccy: &str,
) -> Option<Option<Dec>> {
    let cols = money_cols(title);
    if cols.is_empty() {
        return None;
    }
    let ccy = fields
        .iter()
        .position(|f| f == "Currency")
        .and_then(|i| cells.get(i))
        .map(String::as_str)
        .unwrap_or("USD");
    if !base_ccy.is_empty() && ccy != base_ccy {
        return Some(None);
    }
    let mut any = false;
    let mut sum = Dec::ZERO;
    for col in cols {
        if let Some(raw) = fields
            .iter()
            .position(|f| f == col)
            .and_then(|i| cells.get(i))
        {
            if let Ok(v) = Dec::parse(raw) {
                sum = sum + v;
                any = true;
            }
        }
    }
    if any { Some(Some(sum)) } else { None }
}

/// A row's selection key: the `Symbol` column when present and
/// non-empty, else the `TICKER(…)` description prefix in the
/// sections that carry a ticker only in `Description`. Nothing
/// else is attributed.
fn row_root(title: &str, fields: &[String], cells: &[String]) -> Option<String> {
    let cell = |name: &str| -> Option<&str> {
        fields
            .iter()
            .position(|f| f == name)
            .and_then(|i| cells.get(i))
            .map(String::as_str)
    };
    if let Some(sym) = cell("Symbol") {
        let sym = sym.trim();
        if !sym.is_empty() {
            return Some(root_up(sym));
        }
    }
    if matches!(
        title,
        "Dividends" | "Option Cash Settlements" | "Corporate Actions"
    ) {
        if let Some(t) = cell("Description").and_then(ticker_prefix) {
            return Some(root_up(&t));
        }
    }
    None
}

impl Selection {
    fn fragment(&self) -> Value {
        let keys: Vec<&String> = self.stats.keys().collect();
        let mut sel = Dec::ZERO;
        let mut other = Dec::ZERO;
        let mut keyless = Dec::ZERO;
        let mut non_usd = 0usize;
        for s in self.stats.values() {
            sel = sel + s.matched_usd;
            other = other + s.dropped_usd;
            keyless = keyless + s.unattributed_usd;
            non_usd += s.non_usd;
        }
        let grand = (sel + other) + keyless;
        let stats: Vec<Value> = self
            .stats
            .values()
            .map(|s| {
                Value::Obj(vec![
                    ("matched".to_string(), Value::Num(s.matched.to_string())),
                    (
                        "unattributed".to_string(),
                        Value::Num(s.unattributed.to_string()),
                    ),
                    ("dropped".to_string(), Value::Num(s.dropped.to_string())),
                    ("matched_usd".to_string(), Value::num(&s.matched_usd)),
                    (
                        "unattributed_usd".to_string(),
                        Value::num(&s.unattributed_usd),
                    ),
                    ("dropped_usd".to_string(), Value::num(&s.dropped_usd)),
                    (
                        "non_usd_rows".to_string(),
                        Value::Num(s.non_usd.to_string()),
                    ),
                ])
            })
            .collect();
        Value::Obj(vec![
            (
                "selected".to_string(),
                Value::Arr(self.roots.iter().cloned().map(Value::Str).collect()),
            ),
            (
                "unmatched_selection".to_string(),
                Value::Arr(self.unmatched.iter().cloned().map(Value::Str).collect()),
            ),
            (
                "partition".to_string(),
                Value::Obj(vec![
                    ("selected_roots".to_string(), Value::num(&sel)),
                    ("every_other_root".to_string(), Value::num(&other)),
                    ("ticker_less".to_string(), Value::num(&keyless)),
                    ("grand_total".to_string(), Value::num(&grand)),
                    (
                        "non_usd_rows_held_out".to_string(),
                        Value::Num(non_usd.to_string()),
                    ),
                ]),
            ),
            (
                "sections".to_string(),
                Value::Obj(keys.into_iter().cloned().zip(stats).collect()),
            ),
            (
                "notes".to_string(),
                Value::Arr(vec![
                    Value::Str("rows with no selection key pass through to every target as the ticker-less residual: financing, funding and NAV money is in it".to_string()),
                    Value::Str("selection is by root, not product: letters shared by a stock and a futures root select both".to_string()),
                    Value::Str("unmatched_selection lists typed roots that hit no row: a typo is indistinguishable from an untouched ticker otherwise".to_string()),
                    Value::Str("the partition is exact by construction: components sum to the grand total; every_other_root is the column a typed selection hides".to_string()),
                    Value::Str("Option Cash Settlements is assignment/exercise cash, not P&L: the P&L of those closings already sits in the root columns".to_string()),
                ]),
            ),
        ])
    }
}

/// Session construction errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionError {
    /// Requested target names that do not exist; carries the unknown
    /// names in request order.
    UnknownTargets(Vec<String>),
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SessionError::UnknownTargets(names) => write!(
                f,
                "unknown targets: {}; known targets: {}",
                names.join(", "),
                target::target_names().join(", ")
            ),
        }
    }
}

impl std::error::Error for SessionError {}

/// Default session: feed chunks of one file, call [`Session::end_file`],
/// repeat for the next file.
#[derive(Default)]
pub struct Session {
    csv: Csv,
    file: FileSummary,
    /// Index of the active section in `file.sections`.
    cur: Option<usize>,
    files: Vec<FileSummary>,
    targets: Vec<Box<dyn Target>>,
    sel: Option<Selection>,
}

impl Session {
    /// Structural summaries only, no targets.
    pub fn new() -> Self {
        Self::default()
    }

    /// Session with the named targets; fails listing the unknown names.
    pub fn open(names: &[&str]) -> Result<Self, SessionError> {
        let mut targets = Vec::new();
        let mut unknown = Vec::new();
        for name in names {
            match target::build(name) {
                Some(t) => targets.push(t),
                None => unknown.push((*name).to_string()),
            }
        }
        if !unknown.is_empty() {
            return Err(SessionError::UnknownTargets(unknown));
        }
        let mut s = Session {
            targets,
            ..Default::default()
        };
        for t in s.targets.iter_mut() {
            t.on_file_begin();
        }
        Ok(s)
    }

    /// Session with a ticker selection: each typed ticker is
    /// root-upped, and only rows keyed to a selected root (plus
    /// every keyless row) reach the targets. The session's own
    /// `filter` report fragment lists the selection, the roots
    /// that matched nothing, and per-section row dispositions.
    /// An empty selection behaves like [`Session::open`].
    pub fn open_selected(names: &[&str], tickers: &[&str]) -> Result<Self, SessionError> {
        let mut s = Self::open(names)?;
        if !tickers.is_empty() {
            let roots: BTreeSet<String> = tickers.iter().map(|t| root_up(t)).collect();
            s.sel = Some(Selection {
                unmatched: roots.clone(),
                roots,
                stats: BTreeMap::new(),
                base_ccy: String::new(),
            });
        }
        Ok(s)
    }

    /// Feed bytes of the current file.
    pub fn feed(&mut self, bytes: &[u8]) {
        let mut csv = std::mem::take(&mut self.csv);
        csv.feed(bytes, &mut |rec| self.row(rec));
        self.csv = csv;
    }

    /// Close the current file and record its summary.
    pub fn end_file(&mut self) {
        let mut csv = std::mem::take(&mut self.csv);
        csv.finish(&mut |rec| self.row(rec));
        self.file.bad_rows = csv.bad;
        self.csv = Csv::default();
        self.cur = None;
        if let Some(sel) = self.sel.as_mut() {
            sel.base_ccy.clear();
        }
        for t in self.targets.iter_mut() {
            t.on_file_end();
        }
        self.files.push(std::mem::take(&mut self.file));
        for t in self.targets.iter_mut() {
            t.on_file_begin();
        }
    }

    /// Summaries of all closed files, in feed order.
    pub fn files(&self) -> &[FileSummary] {
        &self.files
    }

    /// Session report: closed file count plus each target's fragment,
    /// in open order.
    pub fn report(&self) -> Value {
        let frags = self
            .targets
            .iter()
            .map(|t| (t.name().to_string(), t.report()))
            .collect();
        let mut out: Vec<(String, Value)> = vec![(
            "files".to_string(),
            Value::Num(self.files.len().to_string()),
        )];
        if let Some(sel) = &self.sel {
            out.push(("filter".to_string(), sel.fragment()));
        }
        out.push(("targets".to_string(), Value::Obj(frags)));
        Value::Obj(out)
    }

    /// Report as compact JSON text.
    pub fn report_text(&self) -> String {
        self.report().text()
    }

    fn row(&mut self, rec: Vec<String>) {
        if rec.len() < 2 {
            self.file.short_rows += 1;
            return;
        }
        let rtype: &str = &rec[1];
        if rtype == "Header" {
            self.file.sections.push(SectionSummary {
                title: rec[0].clone(),
                fields: rec[2..].to_vec(),
                rows: 0,
            });
            self.cur = Some(self.file.sections.len() - 1);
            for t in self.targets.iter_mut() {
                t.on_section(&rec[0], &rec[2..]);
            }
            return;
        }
        if rec[0] == "Statement" && rec.len() >= 4 {
            if rec[2] == "Currency" {
                if let Some(sel) = self.sel.as_mut() {
                    sel.base_ccy = rec[3].trim().to_string();
                }
            }
            match self.file.meta.iter_mut().find(|e| e.0 == rec[2]) {
                Some(e) => e.1 = rec[3].clone(),
                None => self.file.meta.push((rec[2].clone(), rec[3].clone())),
            }
            for t in self.targets.iter_mut() {
                t.on_meta(&rec[2], &rec[3]);
            }
            return;
        }
        if rtype != "Data" && rtype != "Row" {
            self.file.other_rows += 1;
            return;
        }
        let mut title: &str = &rec[0];
        if title == "Change in NAV" {
            let want = "Change in Combined NAV";
            match self.file.sections.iter().rposition(|s| s.title == want) {
                Some(i) => self.cur = Some(i),
                None => {
                    // Rows arrive before any header: the first row names fields.
                    let n = (rec.len() - 2).min(2);
                    self.file.sections.push(SectionSummary {
                        title: want.to_string(),
                        fields: rec[2..2 + n].to_vec(),
                        rows: 0,
                    });
                    self.cur = Some(self.file.sections.len() - 1);
                    return;
                }
            }
            title = want;
        }
        let Some(i) = self.cur else {
            self.file.orphan_rows += 1;
            return;
        };
        self.file.sections[i].rows += 1;
        self.file.data_rows += 1;
        let Session {
            file, targets, sel, ..
        } = self;
        if let Some(sel) = sel.as_mut() {
            let money = row_money(title, &file.sections[i].fields, &rec[2..], &sel.base_ccy);
            let root = row_root(title, &file.sections[i].fields, &rec[2..]);
            let kept = match &root {
                Some(r) => {
                    let hit = sel.roots.contains(r);
                    if hit {
                        sel.unmatched.remove(r);
                    }
                    hit
                }
                None => true,
            };
            let d = sel.stats.entry(title.to_string()).or_default();
            let add = match money {
                Some(Some(v)) => Some(v),
                Some(None) => {
                    d.non_usd += 1;
                    None
                }
                None => None,
            };
            match (&root, kept) {
                (Some(_), true) => {
                    d.matched += 1;
                    if let Some(v) = add {
                        d.matched_usd = d.matched_usd + v;
                    }
                }
                (Some(_), false) => {
                    d.dropped += 1;
                    if let Some(v) = add {
                        d.dropped_usd = d.dropped_usd + v;
                    }
                    return;
                }
                (None, _) => {
                    d.unattributed += 1;
                    if let Some(v) = add {
                        d.unattributed_usd = d.unattributed_usd + v;
                    }
                }
            }
        }
        let row = RowRef {
            title,
            fields: &file.sections[i].fields,
            cells: &rec[2..],
        };
        for t in targets.iter_mut() {
            t.on_row(&row);
        }
    }
}
