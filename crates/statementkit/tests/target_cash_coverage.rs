//! `cash-coverage` target tests.

use statementkit::Session;

fn report_csvs(items: &[&[u8]], chunk: usize) -> String {
    let mut s = Session::open(&["cash-coverage"]).expect("target exists");
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

#[test]
fn coverage_is_base_cash_over_nav() {
    let report = report_csvs(&[&fixture("cash_m1.csv")], 7);
    assert!(report.contains("\"cash_base\":250000"), "{report}");
    assert!(report.contains("\"coverage\":0.25"), "{report}");
}

#[test]
fn non_base_cash_rows_do_not_become_coverage() {
    let report = report_csvs(&[&fixture("cash_m1.csv")], 5);
    // EUR Total Cash 55000 must not show as cash_base
    assert!(!report.contains("\"cash_base\":55000"), "{report}");
}

#[test]
fn segment_and_other_summary_rows_do_not_overwrite_total_cash() {
    let report = report_csvs(&[&fixture("cash_m1.csv")], 9);
    assert!(report.contains("\"cash_base\":250000"), "{report}");
    assert!(report.contains("segment columns"), "{report}");
}

#[test]
fn forex_balances_sum_as_usd_context() {
    let report = report_csvs(&[&fixture("cash_m1.csv")], 4);
    assert!(report.contains("\"forex_balances_usd\":110000"), "{report}");
}

#[test]
fn missing_cash_row_is_null_not_zero() {
    let report = report_csvs(&[&fixture("cash_m2.csv")], 6);
    assert!(report.contains("\"cash_base\":null"), "{report}");
    assert!(report.contains("\"coverage\":null"), "{report}");
    assert!(
        report.contains("no base-currency Total Cash row: 1"),
        "{report}"
    );
}

#[test]
fn fixtures_match_vector() {
    let report = report_csvs(&[&fixture("cash_m1.csv"), &fixture("cash_m2.csv")], 7);
    let want = std::fs::read_to_string(format!(
        "{}/../../tests/vectors/cash_coverage.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("vector missing");
    let prefix = "{\"files\":2,\"targets\":{\"cash-coverage\":";
    let rest = report.strip_prefix(prefix).expect("report shape");
    assert_eq!(&rest[..rest.len() - 2], want.trim_end());
}

#[test]
fn chunk_invariance() {
    let items = [fixture("cash_m1.csv"), fixture("cash_m2.csv")];
    let refs: Vec<&[u8]> = items.iter().map(|v| v.as_slice()).collect();
    let whole = report_csvs(&refs, usize::MAX);
    for size in [1usize, 31, 4096] {
        assert_eq!(report_csvs(&refs, size), whole, "chunk {size}");
    }
}
