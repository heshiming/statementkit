//! `allocation` target tests.

use statementkit::Session;

fn report_csvs(items: &[&[u8]], chunk: usize) -> String {
    let mut s = Session::open(&["allocation"]).expect("target exists");
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
fn nav_table1_is_the_allocation_with_long_short() {
    let report = report_csvs(&[&fixture("alloc_m1.csv")], 7);
    assert!(report.contains("\"nav_total\":753631"), "{report}");
    // 643147/753631 = 0.853398; 113180/753631 = 0.15018
    assert!(
        report.contains(
            "\"class\":\"Cash\",\"long\":663482,\"short\":-20335,\"total\":643147,\
             \"share\":0.853398"
        ),
        "{report}"
    );
    assert!(report.contains("\"share\":0.001056"), "{report}");
    assert!(report.contains("\"share\":-0.004632"), "{report}");
}

#[test]
fn indented_subrows_are_not_classes() {
    let report = report_csvs(&[&fixture("alloc_m1.csv")], 5);
    assert!(!report.contains("Stock Sub"), "{report}");
}

#[test]
fn currency_shares_convert_at_same_file_rate() {
    let report = report_csvs(&[&fixture("alloc_m1.csv")], 9);
    // EUR 50000 at 1.10 = 55000; 55000/753631 = 0.07298
    assert!(
        report.contains("\"currency\":\"EUR\",\"value_usd\":55000,\"share\":0.07298"),
        "{report}"
    );
}

#[test]
fn optional_long_short_columns_report_null() {
    let report = report_csvs(&[&fixture("weight_d1.csv")], 6);
    assert!(report.contains("\"long\":null,\"short\":null"), "{report}");
}

#[test]
fn futures_notional_excluded_from_currency_shares() {
    let report = report_csvs(&[&fixture("weight_d1.csv")], 8);
    // USD rows without the -420000 futures leg: 210000
    assert!(
        report.contains("\"currency\":\"USD\",\"value_usd\":210000"),
        "{report}"
    );
    assert!(!report.contains("-210000"), "{report}");
}

#[test]
fn latest_snapshot_per_account_is_ranked() {
    let report = report_csvs(
        &[
            &fixture("weight_d1.csv"),
            &fixture("weight_d2.csv"),
            &fixture("alloc_m1.csv"),
        ],
        7,
    );
    // account AAA0000001 latest = 2026-09-09 (from weight_d2)
    let i_0908 = report.find("\"2026-09-08\"").expect("snapshot date");
    let i_latest = report
        .find("\"latest_by_account\"")
        .expect("latest section");
    assert!(report[i_latest..].contains("2026-09-09"), "{report}");
    assert!(!report[i_latest..].contains("2026-09-08"), "{report}");
    assert!(i_0908 < i_latest);
    // top class Cash for both accounts
    assert!(
        report.contains("\"top_classes\":[{\"class\":\"Cash\""),
        "{report}"
    );
}

#[test]
fn unrateable_rows_excluded_and_counted() {
    let report = report_csvs(&[&fixture("weight_d2.csv")], 4);
    assert!(
        report.contains("position rows excluded from currency shares (no rate): 1"),
        "{report}"
    );
}

#[test]
fn fixtures_match_vector() {
    let report = report_csvs(
        &[
            &fixture("alloc_m1.csv"),
            &fixture("weight_d1.csv"),
            &fixture("weight_d2.csv"),
        ],
        7,
    );
    let want = std::fs::read_to_string(format!(
        "{}/../../tests/vectors/allocation.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("vector missing");
    let prefix = "{\"files\":3,\"targets\":{\"allocation\":";
    let rest = report.strip_prefix(prefix).expect("report shape");
    assert_eq!(&rest[..rest.len() - 2], want.trim_end());
}

#[test]
fn chunk_invariance() {
    let items = [
        fixture("alloc_m1.csv"),
        fixture("weight_d1.csv"),
        fixture("weight_d2.csv"),
    ];
    let refs: Vec<&[u8]> = items.iter().map(|v| v.as_slice()).collect();
    let whole = report_csvs(&refs, usize::MAX);
    for size in [1usize, 31, 4096] {
        assert_eq!(report_csvs(&refs, size), whole, "chunk {size}");
    }
}
