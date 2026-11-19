//! `risk-series` target tests.

use statementkit::Session;

fn report_files(names: &[&str], chunk: usize) -> String {
    let mut s = Session::open(&["risk-series"]).expect("target exists");
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

const ALL: [&str; 6] = [
    "risk_d1.csv",
    "risk_d2.csv",
    "risk_d3.csv",
    "risk_d4.csv",
    "risk_m1.csv",
    "risk_m2.csv",
];

#[test]
fn daily_curve_de_flows_the_funded_day() {
    let report = report_files(&ALL[..4], 7);
    let want = "\"equity\":[{\"date\":\"2026-01-05\",\"equity\":1000000},\
{\"date\":\"2026-01-06\",\"equity\":900000},\
{\"date\":\"2026-01-07\",\"equity\":950000},\
{\"date\":\"2026-01-08\",\"equity\":980000}]";
    assert!(report.contains(want), "{report}");
}

#[test]
fn daily_drawdown_no_recovery_in_window() {
    let report = report_files(&ALL[..4], 4096);
    assert!(report.contains("\"max_drawdown\":-0.1"), "{report}");
    assert!(report.contains("\"peak_date\":\"2026-01-05\""), "{report}");
    assert!(
        report.contains("\"trough_date\":\"2026-01-06\""),
        "{report}"
    );
    assert!(report.contains("\"recovery_date\":null"), "{report}");
    assert!(
        report.contains("\"drawdown_duration_days\":null"),
        "{report}"
    );
}

#[test]
fn grains_never_mix_and_range_grain_is_flagged() {
    let report = report_files(&ALL, 13);
    assert_eq!(report.match_indices("\"grain\":").count(), 2, "{report}");
    assert!(
        report.contains("\"grain\":\"day\",\"points\":4"),
        "{report}"
    );
    assert!(
        report.contains("\"grain\":\"range\",\"points\":2"),
        "{report}"
    );
    assert!(
        report.contains("range grain cannot see intra-period drawdown"),
        "{report}"
    );
}

#[test]
fn monthly_drawdown_from_de_flowaged_withdrawal_month() {
    // m2: ending 1,800,000 with a 250,000 deposit removed = 1,550,000
    // vs peak 2,000,000 -> -0.225, trough Feb 28, no recovery.
    let report = report_files(&[ALL[4], ALL[5]], 5);
    assert!(report.contains("\"max_drawdown\":-0.225"), "{report}");
    assert!(
        report.contains("\"trough_date\":\"2026-02-28\""),
        "{report}"
    );
    assert!(report.contains("\"recovery_date\":null"), "{report}");
}

#[test]
fn volatility_annualizes_by_grain() {
    let report = report_files(&ALL, usize::MAX);
    // daily returns -0.1, 1/18, 6/190; sample stdev * sqrt(252);
    // the 2-point range series has one return, below the stdev floor
    let want = std::fs::read_to_string(format!(
        "{}/../../tests/vectors/risk_series.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("vector missing");
    for frag in want.lines() {
        assert!(report.contains(frag), "missing {frag} in {report}");
    }
}

#[test]
fn files_without_ending_value_are_skipped() {
    let mut s = Session::open(&["risk-series"]).expect("open");
    s.feed(b"Statement,Data,Period,\"January 9, 2026\"\n");
    s.end_file();
    let report = s.report_text();
    assert!(report.contains("\"series\":[]"), "{report}");
    assert!(report.contains("files skipped"), "{report}");
}

#[test]
fn chunk_invariance_and_out_of_order_files() {
    let whole = report_files(&ALL, usize::MAX);
    let shuffled = report_files(
        &[
            "risk_m2.csv",
            "risk_d3.csv",
            "risk_d1.csv",
            "risk_m1.csv",
            "risk_d4.csv",
            "risk_d2.csv",
        ],
        9,
    );
    assert_eq!(shuffled, whole, "file order must not change the report");
    for size in [1usize, 64, 4096] {
        assert_eq!(report_files(&ALL, size), whole, "chunk {size}");
    }
}
