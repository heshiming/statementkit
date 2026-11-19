//! Streaming session: byte chunks of vertical-layout CSVs in, a
//! per-file structural summary out. Memory is bounded by the largest
//! single record plus the section list; file size is not held.
//!
//! Layout: every row is `[SectionName, RowType, cells..]`. A `Header`
//! row names a section's fields; `Data`/`Row` rows join the active
//! section. `Statement` rows carry key/value metadata. The one known
//! tag alias (`Change in NAV` rows under the `Change in Combined NAV`
//! header) is retagged. Row handling mirrors the reference reader in
//! behavior, including rows joining the active section by position.

use crate::csv::Csv;

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

/// Default session: feed chunks of one file, call [`Session::end_file`],
/// repeat for the next file.
#[derive(Default)]
pub struct Session {
    csv: Csv,
    file: FileSummary,
    /// Index of the active section in `file.sections`.
    cur: Option<usize>,
    files: Vec<FileSummary>,
}

impl Session {
    pub fn new() -> Self {
        Self::default()
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
        self.files.push(std::mem::take(&mut self.file));
    }

    /// Summaries of all closed files, in feed order.
    pub fn files(&self) -> &[FileSummary] {
        &self.files
    }

    fn row(&mut self, rec: Vec<String>) {
        if rec.len() < 2 {
            self.file.short_rows += 1;
            return;
        }
        let (title, rtype, cells) = (&rec[0], &rec[1], &rec[2..]);
        if rtype == "Header" {
            self.file.sections.push(SectionSummary {
                title: title.clone(),
                fields: cells.to_vec(),
                rows: 0,
            });
            self.cur = Some(self.file.sections.len() - 1);
            return;
        }
        if title == "Statement" && cells.len() >= 2 {
            match self.file.meta.iter_mut().find(|e| e.0 == cells[0]) {
                Some(e) => e.1 = cells[1].clone(),
                None => self.file.meta.push((cells[0].clone(), cells[1].clone())),
            }
            return;
        }
        if rtype != "Data" && rtype != "Row" {
            self.file.other_rows += 1;
            return;
        }
        if title == "Change in NAV" {
            let want = "Change in Combined NAV";
            match self.file.sections.iter().rposition(|s| s.title == want) {
                Some(i) => self.cur = Some(i),
                None => {
                    // Rows arrive before any header: the first row names fields.
                    let n = cells.len().min(2);
                    self.file.sections.push(SectionSummary {
                        title: want.to_string(),
                        fields: cells[..n].to_vec(),
                        rows: 0,
                    });
                    self.cur = Some(self.file.sections.len() - 1);
                    return;
                }
            }
        }
        match self.cur {
            Some(i) => {
                self.file.sections[i].rows += 1;
                self.file.data_rows += 1;
            }
            None => self.file.orphan_rows += 1,
        }
    }
}
