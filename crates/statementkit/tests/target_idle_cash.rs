//! `idle-cash` target tests.

use statementkit::Session;

fn report_csvs(items: &[&[u8]], chunk: usize) -> String {
    let mut s = Session::open(&["idle-cash"]).expect("target exists");
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
fn realized_view_month_end_approx() {
    let report = report_csvs(&[&fixture("idle_cash_m1.csv")], 7);
    // docs example: 4,000 / 1,200,000 over March (31 days)
    assert!(
        report.contains(
            "{\"month\":\"2026-03\",\"credit_interest\":4000,\
             \"avg_cash\":1200000,\"annualized_yield\":0.039247,\
             \"cash_basis\":\"month-end approx\"}"
        ),
        "{report}"
    );
}

#[test]
fn daily_mean_basis_with_several_snapshots() {
    let report = report_csvs(
        &[&fixture("idle_cash_m1.csv"), &fixture("idle_cash_m2.csv")],
        5,
    );
    assert!(
        report.contains(
            "\"avg_cash\":1000000,\"annualized_yield\":0.047097,\
             \"cash_basis\":\"daily mean\""
        ),
        "{report}"
    );
}

#[test]
fn net_short_cash_is_not_measured() {
    let short = b"Statement,Data,Currency,USD\n\
                  Statement,Data,Period,\"April 1, 2026 - April 30, 2026\"\n\
                  Cash Report,Header,Category,Currency,Total,Securities,Futures,Paxos\n\
                  Cash Report,Data,Base Currency Summary,USD,-367593,-367593,0,0\n";
    let report = report_csvs(&[short], 6);
    assert!(
        report.contains(
            "{\"month\":\"2026-04\",\"credit_interest\":0,\
             \"avg_cash\":-367593,\"annualized_yield\":\"n/m\",\
             \"cash_basis\":\"n/m net short cash\"}"
        ),
        "{report}"
    );
}

#[test]
fn sweep_rate_stats_and_wide_rows() {
    let report = report_csvs(&[&fixture("idle_cash_m1.csv")], 9);
    // one data row carries a 7th cell beyond the header: rates still map
    assert!(
        report.contains(
            "\"sweep_posted_rate\":{\"rows\":2,\"banks\":[\"BARBUS\",\"BEASUS\"],\
             \"min\":0.0338,\"median\":0.03865,\"max\":0.0435}"
        ),
        "{report}"
    );
}

#[test]
fn accrual_view_annualizes_over_period_span() {
    let report = report_csvs(&[&fixture("idle_cash_m1.csv")], 4);
    assert!(
        report.contains(
            "\"accrual_view\":[{\"month\":\"2026-03\",\"accrued\":4000,\
             \"annualized_yield\":0.040556,\"days\":30}]"
        ),
        "{report}"
    );
}

#[test]
fn latest_segments_reported() {
    let report = report_csvs(
        &[&fixture("idle_cash_m1.csv"), &fixture("idle_cash_m2.csv")],
        8,
    );
    assert!(
        report.contains(
            "\"latest_segments\":{\"cash_total\":800000,\"securities\":800000,\
             \"futures\":0,\"paxos\":0}"
        ),
        "{report}"
    );
}

#[test]
fn fixtures_match_vector() {
    let report = report_csvs(&[&fixture("idle_cash_m1.csv")], 7);
    let want = std::fs::read_to_string(format!(
        "{}/../../tests/vectors/idle_cash.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("vector missing");
    let prefix = "{\"files\":1,\"targets\":{\"idle-cash\":";
    let rest = report.strip_prefix(prefix).expect("report shape");
    assert_eq!(&rest[..rest.len() - 2], want.trim_end());
}

#[test]
fn chunk_invariance() {
    let (a, b) = (fixture("idle_cash_m1.csv"), fixture("idle_cash_m2.csv"));
    let whole = report_csvs(&[&a, &b], usize::MAX);
    for size in [1usize, 31, 4096] {
        assert_eq!(report_csvs(&[&a, &b], size), whole, "chunk {size}");
    }
}
