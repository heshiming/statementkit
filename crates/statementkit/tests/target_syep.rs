//! `syep` target tests.

use statementkit::Session;

fn report_csvs(items: &[&[u8]], chunk: usize) -> String {
    let mut s = Session::open(&["syep"]).expect("target exists");
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

// docs 05 §2 synthetic example: one loan, 30 days, collateral
// 100,000 -> interest 328.77 + fee 41.10 = 369.87.
#[test]
fn two_cash_lines_kept_separate_and_labeled() {
    let report = report_csvs(&[&fixture("syep_m1.csv")], 7);
    assert!(
        report.contains(
            "{\"symbol\":\"XYZ\",\"currency\":\"USD\",\"interest_paid\":328.77,\
             \"fee_earned\":41.1,\"total_lending_income\":369.87,\
             \"annualized_yield_on_collateral\":0.045001}"
        ),
        "{report}"
    );
}

#[test]
fn loan_windows_join_both_details_tables() {
    let report = report_csvs(&[&fixture("syep_m1.csv")], 5);
    assert!(
        report.contains(
            "{\"symbol\":\"XYZ\",\"start\":\"2026-04-01\",\
             \"value_date\":\"2026-05-01\",\"collateral\":100000,\
             \"syep_rate_pct\":\"0.50\",\"interest\":328.77,\"fee\":41.1}"
        ),
        "{report}"
    );
}

#[test]
fn control_total_ties_with_zero_residual() {
    let report = report_csvs(&[&fixture("syep_m1.csv")], 9);
    assert!(
        report.contains(
            "{\"currency\":\"USD\",\"detail_cash_total\":369.87,\
             \"interest_postings_total\":369.87,\"residual\":0}"
        ),
        "{report}"
    );
}

#[test]
fn control_residual_reported_not_absorbed() {
    let extra = b"Statement,Data,Currency,USD\n\
                  Interest,Header,Currency,Account,Symbol,Date,Description,Amount\n\
                  Interest,Data,USD,AAA0000001,,\"2026, May\",SYEP Interest,10\n";
    let report = report_csvs(&[&fixture("syep_m1.csv"), extra], 6);
    assert!(
        report.contains("\"interest_postings_total\":379.87"),
        "{report}"
    );
    assert!(report.contains("\"residual\":10"), "{report}");
    // detail side untouched
    assert!(report.contains("\"detail_cash_total\":369.87"), "{report}");
}

#[test]
fn absent_without_tables_or_bucket() {
    let report = report_csvs(&[&fixture("quality_m1.csv")], 8);
    assert!(report.contains("\"present\":\"false\""), "{report}");
}

#[test]
fn fixtures_match_vector() {
    let report = report_csvs(&[&fixture("syep_m1.csv")], 7);
    let want = std::fs::read_to_string(format!(
        "{}/../../tests/vectors/syep.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("vector missing");
    let prefix = "{\"files\":1,\"targets\":{\"syep\":";
    let rest = report.strip_prefix(prefix).expect("report shape");
    assert_eq!(&rest[..rest.len() - 2], want.trim_end());
}

#[test]
fn chunk_invariance() {
    let one = fixture("syep_m1.csv");
    let whole = report_csvs(&[&one], usize::MAX);
    for size in [1usize, 31, 4096] {
        assert_eq!(report_csvs(&[&one], size), whole, "chunk {size}");
    }
}
