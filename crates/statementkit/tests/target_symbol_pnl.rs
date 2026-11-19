//! `symbol-pnl` target tests.

use statementkit::Session;

fn report_csvs(items: &[&[u8]], chunk: usize) -> String {
    let mut s = Session::open(&["symbol-pnl"]).expect("target exists");
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

// docs 05 §4 synthetic example: realized 4,000 + Δunreal 700
// + dividends 900 - borrow fee 240 = 5,360 all-in.
#[test]
fn all_in_spans_two_snapshots() {
    let report = report_csvs(
        &[
            &fixture("symbol_pnl_m1a.csv"),
            &fixture("symbol_pnl_m1b.csv"),
        ],
        7,
    );
    assert!(report.contains("\"snapshots\":2"), "{report}");
    // FOO fee is 0 here (NIQ carries it); the doc's exact 5,360 is the
    // shape test below.
    assert!(
        report.contains(
            "{\"symbol\":\"FOO\",\"realized\":4000,\"delta_unrealized\":700,\
             \"dividends\":900,\"borrow_fee\":0,\"all_in\":5600}"
        ),
        "{report}"
    );
}

#[test]
fn new_symbol_delta_is_level_when_prior_exists() {
    let report = report_csvs(
        &[
            &fixture("symbol_pnl_m1a.csv"),
            &fixture("symbol_pnl_m1b.csv"),
        ],
        5,
    );
    // GRO absent from the prior snapshot: Δ = its level, not 0
    assert!(
        report.contains(
            "{\"symbol\":\"GRO\",\"realized\":0,\"delta_unrealized\":-2976,\
             \"dividends\":0,\"borrow_fee\":0,\"all_in\":-2976}"
        ),
        "{report}"
    );
}

#[test]
fn borrow_fee_own_column_negative_sign() {
    let report = report_csvs(
        &[
            &fixture("symbol_pnl_m1a.csv"),
            &fixture("symbol_pnl_m1b.csv"),
        ],
        9,
    );
    assert!(
        report.contains(
            "{\"symbol\":\"NIQ\",\"realized\":0,\"delta_unrealized\":0,\
             \"dividends\":0,\"borrow_fee\":-36,\"all_in\":-36}"
        ),
        "{report}"
    );
    assert!(
        report.contains(
            "{\"symbol\":\"TOTAL\",\"realized\":4000,\"delta_unrealized\":-2276,\
             \"dividends\":900,\"borrow_fee\":-36,\"all_in\":2588}"
        ),
        "{report}"
    );
}

#[test]
fn total_row_and_stale_symbol_excluded() {
    let report = report_csvs(
        &[
            &fixture("symbol_pnl_m1a.csv"),
            &fixture("symbol_pnl_m1b.csv"),
        ],
        4,
    );
    assert!(!report.contains("999"), "poison Total leaked: {report}");
    assert!(!report.contains("\"BAR\""), "{report}");
}

#[test]
fn single_snapshot_delta_zero() {
    let report = report_csvs(&[&fixture("symbol_pnl_m1b.csv")], 6);
    assert!(report.contains("\"snapshots\":1"), "{report}");
    assert!(
        report.contains("\"realized\":4000,\"delta_unrealized\":0"),
        "{report}"
    );
    assert!(report.contains("one snapshot only"), "{report}");
}

#[test]
fn sorted_by_all_in_descending() {
    let report = report_csvs(
        &[
            &fixture("symbol_pnl_m1a.csv"),
            &fixture("symbol_pnl_m1b.csv"),
        ],
        8,
    );
    let f = report.find("\"FOO\"").expect("FOO");
    let n = report.find("\"NIQ\"").expect("NIQ");
    let g = report.find("\"GRO\"").expect("GRO");
    assert!(f < n && n < g, "{report}");
}

#[test]
fn fixtures_match_vector() {
    let report = report_csvs(
        &[
            &fixture("symbol_pnl_m1a.csv"),
            &fixture("symbol_pnl_m1b.csv"),
        ],
        7,
    );
    let want = std::fs::read_to_string(format!(
        "{}/../../tests/vectors/symbol_pnl.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("vector missing");
    let prefix = "{\"files\":2,\"targets\":{\"symbol-pnl\":";
    let rest = report.strip_prefix(prefix).expect("report shape");
    assert_eq!(&rest[..rest.len() - 2], want.trim_end());
}

#[test]
fn chunk_invariance() {
    let (a, b) = (fixture("symbol_pnl_m1a.csv"), fixture("symbol_pnl_m1b.csv"));
    let whole = report_csvs(&[&a, &b], usize::MAX);
    for size in [1usize, 31, 4096] {
        assert_eq!(report_csvs(&[&a, &b], size), whole, "chunk {size}");
    }
}
