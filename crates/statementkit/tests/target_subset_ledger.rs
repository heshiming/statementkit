//! `subset-ledger` target tests.

use statementkit::Session;

fn report_fixture_set(chunk: usize) -> String {
    let names = ["sel_m1.csv", "sel_m2.csv"];
    let mut s = Session::open(&["subset-ledger"]).expect("target exists");
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
fn roots_carry_options_and_futures_expiries() {
    let report = report_fixture_set(7);
    // ABC = stock + its option leg (commissions ride every fill);
    // MNE = two expiries of one root.
    assert!(
        report.contains(
            "{\"root\":\"ABC\",\"realized\":-100,\"commissions\":-5,\"dividends\":50,\"borrow_fee\":-4,\"total\":-59,"
        ),
        "{report}"
    );
    assert!(
        report.contains(
            "{\"root\":\"MNE\",\"realized\":800,\"commissions\":-6,\"dividends\":0,\"borrow_fee\":0,\"total\":794,"
        ),
        "{report}"
    );
}

#[test]
fn booking_period_is_the_files_period() {
    let report = report_fixture_set(6);
    // the -200 close carries a January fill stamp but sits in the
    // February file: it books to February.
    assert!(
        report
            .contains("{\"period_from\":\"2026-02-01\",\"period_to\":\"2026-02-28\",\"total\":95}"),
        "{report}"
    );
    assert!(report.contains("\"grand_total\":735"), "{report}");
}

#[test]
fn capital_free_metrics_only() {
    let report = report_fixture_set(5);
    assert!(report.contains("\"max_drawdown\":-202"), "{report}");
    assert!(report.contains("\"profit_factor\":0.707921"), "{report}");
    assert!(report.contains("\"recovery\":-0.292079"), "{report}");
    assert!(report.contains("no %-return, no subset TWR"), "{report}");
}

#[test]
fn non_usd_counted_not_mixed() {
    let report = report_fixture_set(4);
    // the EUR fill and the EUR dividend never enter a bucket
    assert!(report.contains("\"non_usd_rows\":2"), "{report}");
    assert!(!report.contains("\"dividends\":57"), "{report}");
}

#[test]
fn declared_bases_move_ratios_do_not() {
    let report = report_fixture_set(6);
    // subset stream [640, 95]: cum_pct of the declared K, labeled
    // percent-of-capital, never CAGR
    assert!(
        report.contains("{\"kind\":\"notional\",\"k\":100000,\"ann_return\":0.0433,\"ann_vol\":0.009354,\"cum_pct\":0.00735}"),
        "{report}"
    );
    assert!(
        report.contains("\"kind\":\"family-nav-avg\",\"k\":1010000"),
        "{report}"
    );
    assert!(
        report.contains("\"nav_min\":1000000,\"nav_max\":1020000"),
        "{report}"
    );
    assert!(report.contains("never CAGR"), "{report}");
}

#[test]
fn fixtures_match_vector() {
    let report = report_fixture_set(7);
    let want = std::fs::read_to_string(format!(
        "{}/../../tests/vectors/subset_ledger.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("vector missing");
    let prefix = "{\"files\":2,\"targets\":{\"subset-ledger\":";
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
