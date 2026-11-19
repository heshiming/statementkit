//! Session ticker-selection tests (root-up matching, filter report).

use statementkit::Session;

const CSV: &str = "Statement,Header,Account ID,Currency,Period,When Generated
Statement,Data,Account ID,AAA0000001
Statement,Data,Currency,USD
Statement,Data,Period,\"January 1, 2026 - January 31, 2026\"
Statement,Data,When Generated,\"February 2, 2026, 04:02:11 EST\"
Trades,Header,Symbol,Date/Time,Quantity,Trade Price,Realized P/L,Comm/Fee,Code
Trades,Data,ABC,\"2026-01-05, 10:00:00\",100,50,0,-1,O
Trades,Data,\"ABC 20Mar26 50 C\",\"2026-01-06, 10:00:00\",-10,3,-15,-1,C
Trades,Data,\"XYZ 20Mar26 50 C\",\"2026-01-06, 10:00:00\",-10,3,-15,-1,C
Trades,Data,MNEH6,\"2026-01-07, 10:00:00\",2,4800,0,-1,O
Dividends,Header,Currency,Ex Date,Pay Date,Amount,Description
Dividends,Data,USD,20260115,20260210,50,\"ABC(US00000X1060) CASH DIVIDEND\"
Dividends,Data,USD,20260115,20260210,70,\"XYZ(US00000Y1060) CASH DIVIDEND\"
Interest,Header,Currency,Account,Date,Description,Amount
Interest,Data,USD,AAA0000001,20260131,CR INT,40
";

fn selected(tickers: &[&str]) -> String {
    let mut s = Session::open_selected(&["income"], tickers).expect("session");
    s.feed(CSV.as_bytes());
    s.end_file();
    s.report_text()
}

#[test]
fn root_up_takes_option_legs_and_strips_futures() {
    let report = selected(&["ABC", "MNE"]);
    // stock row + option leg + the MNE future survive; the XYZ leg dies
    assert!(
        report.contains("\"selected\":[\"ABC\",\"MNE\"]"),
        "{report}"
    );
    assert!(
        report.contains("\"Trades\":{\"matched\":3,\"unattributed\":0,\"dropped\":1}"),
        "{report}"
    );
}

#[test]
fn description_prefix_selects_the_dividend() {
    let report = selected(&["ABC"]);
    assert!(
        report.contains("\"Dividends\":{\"matched\":1,\"unattributed\":0,\"dropped\":1}"),
        "{report}"
    );
}

#[test]
fn keyless_money_passes_as_the_residual() {
    let report = selected(&["ABC"]);
    // Interest carries no ticker: it always reaches the targets.
    assert!(
        report.contains("\"Interest\":{\"matched\":0,\"unattributed\":1,\"dropped\":0}"),
        "{report}"
    );
    assert!(report.contains("ticker-less residual"), "{report}");
}

#[test]
fn typo_guard_lists_unmatched_roots() {
    let report = selected(&["ABC", "MNQ"]);
    assert!(
        report.contains("\"unmatched_selection\":[\"MNQ\"]"),
        "{report}"
    );
}

#[test]
fn no_selection_has_no_filter_fragment() {
    let mut s = Session::open(&["income"]).expect("session");
    s.feed(CSV.as_bytes());
    s.end_file();
    let report = s.report_text();
    assert!(!report.contains("\"filter\""), "{report}");
}

#[test]
fn selection_keeps_targets_honest_about_residual() {
    // income's credit-interest bucket sees the keyless 40 even under
    // a selection that names no interest row.
    let report = selected(&["ABC"]);
    assert!(report.contains("\"credit_interest\":40"), "{report}");
}
