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
        ("basis-recon", || {
            Box::new(targets::basis_recon::BasisRecon::new())
        }),
        ("borrow-fees", || {
            Box::new(targets::borrow_fees::BorrowFees::new())
        }),
        ("leverage", || Box::new(targets::leverage::Leverage::new())),
        ("cash-coverage", || {
            Box::new(targets::cash_coverage::CashCoverage::new())
        }),
        ("allocation", || Box::new(targets::alloc::Allocation::new())),
        ("weights", || Box::new(targets::weights::Weights::new())),
        ("trades", || Box::new(targets::trades::Trades::new())),
        ("quality", || Box::new(targets::quality::Quality::new())),
        ("income", || Box::new(targets::income::Income::new())),
        ("realized-stlt", || {
            Box::new(targets::realized_stlt::RealizedStlt::new())
        }),
        ("symbol-pnl", || {
            Box::new(targets::symbol_pnl::SymbolPnl::new())
        }),
        ("dividends", || {
            Box::new(targets::dividends::Dividends::new())
        }),
        ("syep", || Box::new(targets::syep::Syep::new())),
        (
            "idle-cash",
            || Box::new(targets::idle_cash::IdleCash::new()),
        ),
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
