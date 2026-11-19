//! Target interface: one extraction goal per implementation file,
//! registered by name, selected at session open, mutually ignorant.

use crate::json::Value;
use crate::targets;

/// One data row plus its section title and header fields.
pub struct RowRef<'a> {
    pub title: &'a str,
    pub fields: &'a [String],
    pub cells: &'a [String],
}

impl<'a> RowRef<'a> {
    /// Cell under the named header field, if present.
    pub fn cell(&self, field: &str) -> Option<&'a str> {
        self.fields
            .iter()
            .position(|f| f == field)
            .and_then(|i| self.cells.get(i))
            .map(String::as_str)
    }
}

/// A pluggable extraction target. Callbacks fire per file in feed
/// order; `report` renders the target's JSON fragment.
pub trait Target {
    fn name(&self) -> &'static str;
    fn on_file_begin(&mut self) {}
    fn on_meta(&mut self, _key: &str, _value: &str) {}
    fn on_section(&mut self, _title: &str, _fields: &[String]) {}
    fn on_row(&mut self, _row: &RowRef) {}
    fn on_file_end(&mut self) {}
    fn report(&self) -> Value;
}

/// Every built target, by name.
type Factory = fn() -> Box<dyn Target>;

pub(crate) fn registry() -> &'static [(&'static str, Factory)] {
    &[
        ("nav-series", || {
            Box::new(targets::nav_series::NavSeries::new())
        }),
        ("attribution", || {
            Box::new(targets::attribution::Attribution::new())
        }),
        ("risk-series", || {
            Box::new(targets::risk_series::RiskSeries::new())
        }),
        ("twr", || Box::new(targets::twr::Twr::new())),
        ("xirr", || Box::new(targets::xirr::Xirr::new())),
        ("interest", || Box::new(targets::interest::Interest::new())),
        ("allocation", || Box::new(targets::alloc::Allocation::new())),
        ("weights", || Box::new(targets::weights::Weights::new())),
        ("trades", || Box::new(targets::trades::Trades::new())),
    ]
}

/// Names accepted by [`crate::Session::open`].
pub fn target_names() -> Vec<&'static str> {
    registry().iter().map(|(n, _)| *n).collect()
}

pub(crate) fn build(name: &str) -> Option<Box<dyn Target>> {
    registry()
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, f)| f())
}
