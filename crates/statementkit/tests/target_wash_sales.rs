//! `wash-sales` target tests.

use statementkit::Session;

fn report_csvs(items: &[&[u8]], chunk: usize) -> String {
    let mut s = Session::open(&["wash-sales"]).expect("target exists");
    for bytes in items {
        for part in bytes.chunks(chunk.max(1)) {
            s.feed(part);
        }
        s.end_file();
    }
    s.report_text()
}

fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/../../tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(path).expect("fixture missing")
}

// docs 05 §3 example: sell 100 at -1,200 Mar 5; buy 60 Mar 20 and
// 60 Feb 10 (23 days before, still in the +/-30 window) -> the
// whole 100 matched -> flag 1,200.
#[test]
fn pro_rated_flag_within_window() {
    let report = report_csvs(&[&fixture("wash_m1.csv")], 7);
    assert!(
        report.contains("{\"symbol\":\"WSA\",\"loss\":1200,\"flagged\":1200,\"share\":1}"),
        "{report}"
    );
}

#[test]
fn outside_window_buy_not_matched() {
    let report = report_csvs(&[&fixture("wash_m1.csv")], 5);
    // WSB: 40 shares Apr 4 inside 30d, 50 shares Apr 20 outside -> 400
    assert!(
        report.contains("{\"symbol\":\"WSB\",\"loss\":1000,\"flagged\":400,\"share\":0.4}"),
        "{report}"
    );
}

#[test]
fn assignment_marker_is_not_a_buy() {
    let report = report_csvs(&[&fixture("wash_m1.csv")], 9);
    // WSD's only nearby 30d+ row is an `A` bookkeeping marker
    assert!(
        report.contains("{\"symbol\":\"WSD\",\"loss\":500,\"flagged\":0,\"share\":0}"),
        "{report}"
    );
    assert!(report.contains("\"symbols_flagged\":2"), "{report}");
}

#[test]
fn profit_closes_are_not_candidates() {
    let report = report_csvs(&[&fixture("wash_m1.csv")], 4);
    assert!(!report.contains("WSC"), "{report}");
    assert!(report.contains("\"loss_symbols\":3"), "{report}");
}

#[test]
fn totals_labeled_upper_bound() {
    let report = report_csvs(&[&fixture("wash_m1.csv")], 6);
    assert!(
        report.contains("\"totals\":{\"loss\":2700,\"flagged_upper_bound\":1600}"),
        "{report}"
    );
    assert!(
        report.contains("upper bound on probable disallowed loss"),
        "{report}"
    );
}

#[test]
fn fixtures_match_vector() {
    let report = report_csvs(&[&fixture("wash_m1.csv")], 7);
    let want = std::fs::read_to_string(format!(
        "{}/../../tests/vectors/wash_sales.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("vector missing");
    let prefix = "{\"files\":1,\"targets\":{\"wash-sales\":";
    let rest = report.strip_prefix(prefix).expect("report shape");
    assert_eq!(&rest[..rest.len() - 2], want.trim_end());
}

#[test]
fn chunk_invariance() {
    let one = fixture("wash_m1.csv");
    let whole = report_csvs(&[&one], usize::MAX);
    for size in [1usize, 31, 4096] {
        assert_eq!(report_csvs(&[&one], size), whole, "chunk {size}");
    }
}
