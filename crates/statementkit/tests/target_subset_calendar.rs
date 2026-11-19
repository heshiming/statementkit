//! `subset-calendar` target tests.

use statementkit::Session;

fn report(tickers: &[&str], chunk: usize) -> String {
    let names = ["subc_m1.csv", "subc_m2.csv", "subc_m3.csv"];
    let mut s = Session::open_selected(&["subset-calendar"], tickers).expect("target");
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
fn weekly_rollup_and_day_counts() {
    let report = report(&[], 7);
    assert!(
        report.contains("{\"week_of\":\"2026-10-12\",\"total\":-104,\"days_with_money\":2}"),
        "{report}"
    );
    // the EUR fill is counted, not summed
    assert!(report.contains("\"non_usd_rows\":1"), "{report}");
}

#[test]
fn daily_stats_annualize_by_252() {
    let report = report(&[], 6);
    // series [98, -202]: maxDD -202, PF 98/202, Sharpe mean/sd x sqrt(252)
    assert!(report.contains("\"max_drawdown\":-202"), "{report}");
    assert!(report.contains("\"profit_factor\":0.485149"), "{report}");
    assert!(
        report.contains("\"sharpe_annualized\":-5.503163"),
        "{report}"
    );
    assert!(report.contains("small-number arithmetic"), "{report}");
}

#[test]
fn marks_walk_back_to_the_last_open_day() {
    // selection "ABC" sees no ABC position on 10-15: the machinery
    // walks back and says which day it used
    let report = report(&["ABC"], 5);
    assert!(report.contains("\"day_used\":\"2026-10-14\""), "{report}");
    assert!(report.contains("\"nav\":950000"), "{report}");
}

#[test]
fn marks_are_signed_and_converted_at_the_files_own_rate() {
    let report = report(&["ABC"], 4);
    // short EUR option leg: -500 x 1.10
    assert!(
        report.contains("{\"root\":\"ABC\",\"legs\":1,\"mark_usd\":-550,\"pct_of_nav\":-0.000579}"),
        "{report}"
    );
    assert!(report.contains("premium notional"), "{report}");
}

#[test]
fn fixtures_match_vector() {
    let report = report(&[], 7);
    let want = std::fs::read_to_string(format!(
        "{}/../../tests/vectors/subset_calendar.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("vector missing");
    let prefix = "{\"files\":3,\"targets\":{\"subset-calendar\":";
    let rest = report.strip_prefix(prefix).expect("report shape");
    assert_eq!(&rest[..rest.len() - 2], want.trim_end());
}

#[test]
fn chunk_invariance() {
    let whole = report(&[], usize::MAX);
    for size in [1usize, 31, 4096] {
        assert_eq!(report(&[], size), whole, "chunk {size}");
    }
}
