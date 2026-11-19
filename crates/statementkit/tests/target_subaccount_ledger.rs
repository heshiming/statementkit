//! `subaccount-ledger` target tests.

use statementkit::Session;

fn report_fixture_set(chunk: usize) -> String {
    let names = ["ledger_m1.csv", "ledger_m2.csv", "ledger_m3.csv"];
    let mut s = Session::open(&["subaccount-ledger"]).expect("target exists");
    for name in names {
        let path = format!("{}/../../tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        let bytes = std::fs::read(path).expect("fixture missing");
        for part in bytes.chunks(chunk.max(1)) {
            s.feed(part);
        }
        s.end_file();
    }
    s.report_text()
}

#[test]
fn realized_plus_carry_sums() {
    let report = report_fixture_set(7);
    // day1: 1000-10 realized+comm +100 div +50 int -40 fee = 1100
    assert!(report.contains("\"pnl\":1100"), "{report}");
    assert!(report.contains("\"pnl\":-310"), "{report}");
}

#[test]
fn metrics_without_capital_base() {
    let report = report_fixture_set(5);
    assert!(report.contains("\"max_drawdown\":-2010"), "{report}");
    assert!(report.contains("\"profit_factor\":0.845771"), "{report}");
    assert!(report.contains("\"recovery\":-0.154229"), "{report}");
    assert!(
        report.contains("\"sharpe_annualized\":-1.448286"),
        "{report}"
    );
}

#[test]
fn marker_variant_shares_the_bare_leg() {
    let report = report_fixture_set(6);
    // m2's Trades row names the same account with the marker
    assert!(report.contains("\"pnl\":-2010"), "{report}");
}

#[test]
fn member_leg_gets_its_own_ledger() {
    let report = report_fixture_set(4);
    assert!(report.contains("\"account\":\"U10000025\""), "{report}");
    // single period: no spread metrics
    assert!(report.contains("\"sharpe_annualized\":null"), "{report}");
}

#[test]
fn non_usd_rows_counted_and_dropped() {
    let report = report_fixture_set(3);
    assert!(
        report.contains("\"reason\":\"Interest: non-USD\""),
        "{report}"
    );
    assert!(report.contains("never mixed"), "{report}");
    assert!(
        report.contains("reconcile with Change in Combined NAV"),
        "{report}"
    );
}

#[test]
fn fixtures_match_vector() {
    let report = report_fixture_set(7);
    let want = std::fs::read_to_string(format!(
        "{}/../../tests/vectors/subaccount_ledger.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("vector missing");
    let prefix = "{\"files\":3,\"targets\":{\"subaccount-ledger\":";
    let rest = report.strip_prefix(prefix).expect("report shape");
    assert_eq!(&rest[..rest.len() - 2], want.trim_end());
}

#[test]
fn chunk_invariance() {
    let whole = report_fixture_set(usize::MAX);
    for size in [1usize, 31, 4096] {
        assert_eq!(report_fixture_set(size), whole, "chunk {size}");
    }
}
