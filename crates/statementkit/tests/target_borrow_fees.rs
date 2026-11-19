//! `borrow-fees` target tests.

use statementkit::Session;

fn report_csvs(items: &[&[u8]], chunk: usize) -> String {
    let mut s = Session::open(&["borrow-fees"]).expect("target exists");
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
fn block_total_rows_tie_to_raw_detail() {
    let three = [
        fixture("borrow_m1.csv"),
        fixture("borrow_m2.csv"),
        fixture("borrow_a1.csv"),
    ];
    let refs: Vec<&[u8]> = three.iter().map(|v| v.as_slice()).collect();
    let report = report_csvs(&refs, 7);
    assert!(
        report.contains("\"checked\":3,\"exact_ties\":3,\"mismatched\":0,\"worst_rel_diff\":0"),
        "{report}"
    );
}

#[test]
fn per_symbol_fee_days_peak_and_blended_rates() {
    let report = report_csvs(&[&fixture("borrow_m1.csv")], 5);
    // SHORT1: 2 days, peak 1200, fee 2.20, 2.20/2200 = 0.001, 2.20/44000 = 0.00005
    assert!(
        report.contains(
            "\"symbol\":\"SHORT1\",\"days\":2,\"peak_shares\":1200,\"fee\":2.2,\
             \"fee_per_share_day\":0.001,\"fee_pct_of_value_day\":0.00005"
        ),
        "{report}"
    );
}

#[test]
fn overlapping_files_dedup_to_one_leg_but_control_stays_per_file() {
    let two = [fixture("borrow_m1.csv"), fixture("borrow_m2.csv")];
    let refs: Vec<&[u8]> = two.iter().map(|v| v.as_slice()).collect();
    let report = report_csvs(&refs, 9);
    assert!(
        report.contains("duplicate fee rows dropped (overlapping files): 1"),
        "{report}"
    );
    // SHORT1 counted once: fee 2.2 not 3.2
    assert!(
        report.contains("\"fee\":2.2,\"fee_per_share_day\":0.001"),
        "{report}"
    );
    // m2's own block still ties its Total (raw detail per file)
    assert!(report.contains("\"exact_ties\":2"), "{report}");
}

#[test]
fn annual_postings_are_the_control_and_reported_not_added() {
    let report = report_csvs(&[&fixture("borrow_a1.csv")], 6);
    assert!(report.contains("\"posting_control_files\":1"), "{report}");
    assert!(
        report.contains("Interest posting total -120 is the annual control"),
        "{report}"
    );
    // fee total is the DETAIL 20, postings (-120) are never summed into it
    assert!(report.contains("\"fee\":20"), "{report}");
    assert!(!report.contains("\"fee\":-100"), "{report}");
}

#[test]
fn non_base_fee_rows_excluded_and_named() {
    let report = report_csvs(&[&fixture("borrow_m1.csv")], 4);
    assert!(
        report.contains("non-base-currency fee rows excluded (no FX derivation on fee detail): 1"),
        "{report}"
    );
    assert!(!report.contains("\"symbol\":\"EURO\""), "{report}");
}

#[test]
fn percent_rate_column_is_not_parsed() {
    let report = report_csvs(&[&fixture("borrow_m1.csv")], 8);
    assert!(
        report.contains("percent form and is not parsed"),
        "{report}"
    );
}

#[test]
fn fixtures_match_vector() {
    let three = [
        fixture("borrow_a1.csv"),
        fixture("borrow_m1.csv"),
        fixture("borrow_m2.csv"),
    ];
    let refs: Vec<&[u8]> = three.iter().map(|v| v.as_slice()).collect();
    let report = report_csvs(&refs, 7);
    let want = std::fs::read_to_string(format!(
        "{}/../../tests/vectors/borrow_fees.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("vector missing");
    let prefix = "{\"files\":3,\"targets\":{\"borrow-fees\":";
    let rest = report.strip_prefix(prefix).expect("report shape");
    assert_eq!(&rest[..rest.len() - 2], want.trim_end());
}

#[test]
fn chunk_invariance() {
    let three = [
        fixture("borrow_m1.csv"),
        fixture("borrow_m2.csv"),
        fixture("borrow_a1.csv"),
    ];
    let refs: Vec<&[u8]> = three.iter().map(|v| v.as_slice()).collect();
    let whole = report_csvs(&refs, usize::MAX);
    for size in [1usize, 31, 4096] {
        assert_eq!(report_csvs(&refs, size), whole, "chunk {size}");
    }
}
