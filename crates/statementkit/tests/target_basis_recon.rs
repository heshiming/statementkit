//! `basis-recon` target tests.

use statementkit::Session;

fn report_csvs(items: &[&[u8]], chunk: usize) -> String {
    let mut s = Session::open(&["basis-recon"]).expect("target exists");
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
fn weighted_avg_fill_vs_ib_cost_price() {
    let report = report_csvs(&[&fixture("basis_m1.csv")], 7);
    // AAA: (10*100+12*100)/200 = 11 vs IB 11 -> 0
    assert!(
        report.contains(
            "\"symbol\":\"AAA\",\"ib_cost_price\":11,\"in_window_avg_fill\":11,\
             \"rel_diff_vs_ib\":0"
        ),
        "{report}"
    );
    // BBB: in-window 10 vs IB 12 (pre-window lot) -> 2/12
    assert!(
        report.contains(
            "\"symbol\":\"BBB\",\"ib_cost_price\":12,\"in_window_avg_fill\":10,\
             \"rel_diff_vs_ib\":0.166667"
        ),
        "{report}"
    );
}

#[test]
fn sorted_by_rel_diff_descending() {
    let report = report_csvs(&[&fixture("basis_m1.csv")], 5);
    let b = report.find("\"BBB\"").expect("BBB");
    let a = report.find("\"AAA\"").expect("AAA");
    assert!(b < a, "{report}");
}

#[test]
fn sells_do_not_weight_the_average() {
    let report = report_csvs(&[&fixture("basis_m1.csv")], 9);
    // BBB average fill is the BUY 10.00, not blended toward the 15.00 sell
    assert!(report.contains("\"in_window_avg_fill\":10"), "{report}");
}

#[test]
fn one_sided_symbols_counted_not_compared() {
    let report = report_csvs(&[&fixture("basis_m1.csv")], 4);
    assert!(report.contains("\"compared\":2"), "{report}");
    assert!(
        report.contains("no in-window buy fills (pre-window or transfer opens): 1"),
        "{report}"
    );
    assert!(
        report.contains("in-window buys but no Open Positions cost price: 1"),
        "{report}"
    );
    assert!(!report.contains("\"CCC\""), "{report}");
    assert!(!report.contains("\"DDD\""), "{report}");
}

#[test]
fn latest_snapshot_wins_for_ib_cost_price() {
    let later = b"Statement,Data,Period,\"April 1, 2026 - April 30, 2026\"\n\
                  Statement,Data,Currency,USD\n\
                  Open Positions,Header,Symbol,Quantity,Close Price,Cost Price,Mult,Value,Cost Basis,Unrealized P/L,Asset Category,Currency,DataDiscriminator\n\
                  Open Positions,Data,BBB,100,14.00,14.00,1,1400,-1400,0,Stock,USD,Summary\n";
    let report = report_csvs(&[&fixture("basis_m1.csv"), later], 8);
    // April snapshot 14.00 replaces March 12.00: rel = 4/14 = 0.285714
    assert!(report.contains("\"ib_cost_price\":14"), "{report}");
    assert!(report.contains("\"rel_diff_vs_ib\":0.285714"), "{report}");
}

#[test]
fn fixtures_match_vector() {
    let report = report_csvs(&[&fixture("basis_m1.csv")], 7);
    let want = std::fs::read_to_string(format!(
        "{}/../../tests/vectors/basis_recon.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("vector missing");
    let prefix = "{\"files\":1,\"targets\":{\"basis-recon\":";
    let rest = report.strip_prefix(prefix).expect("report shape");
    assert_eq!(&rest[..rest.len() - 2], want.trim_end());
}

#[test]
fn chunk_invariance() {
    let one = fixture("basis_m1.csv");
    let whole = report_csvs(&[&one], usize::MAX);
    for size in [1usize, 31, 4096] {
        assert_eq!(report_csvs(&[&one], size), whole, "chunk {size}");
    }
}
