//! `twr` target tests.

use statementkit::Session;

fn report_files(names: &[&str], chunk: usize) -> String {
    let mut s = Session::open(&["twr"]).expect("target exists");
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

const FIXTURES: [&str; 3] = ["daily_sample.csv", "monthly_sample.csv", "twr_no_t2.csv"];

#[test]
fn stated_table2_rate_wins() {
    let report = report_files(&FIXTURES, 7);
    assert!(report.contains("\"twr_stated\":0.0019"), "{report}");
    assert!(report.contains("\"source\":\"table2\""), "{report}");
}

#[test]
fn missing_table2_falls_back_to_nav_chain() {
    let report = report_files(&FIXTURES[..3], 3);
    // oct: (6,252,100 - 6,552,100 - 0) / 6,552,100
    assert!(
        report.contains("\"twr_stated\":null,\"fallback\":-0.045787,\"source\":\"nav-chain\""),
        "{report}"
    );
    assert!(
        report.contains("table 2 is absent, rate taken from the NAV chain: 1"),
        "{report}"
    );
}

#[test]
fn fallback_never_avoids_deflow() {
    // monthly: (E-S-D&W)/S = (152,100 + 50,000)/6,400,000 = 0.031578
    let report = report_files(&[FIXTURES[1]], 11);
    assert!(report.contains("\"fallback\":0.031578"), "{report}");
}

#[test]
fn chain_is_multiplicative_with_overlap_note() {
    let report = report_files(&FIXTURES, 4096);
    // (1.03)(1.0019)(1-0.045786...) computed independently:
    assert!(report.contains("\"periods\":3"), "{report}");
    assert!(
        report.contains("\"cumulative_return\":-0.015293"),
        "{report}"
    );
    assert!(report.contains("\"annualized\":-0.089549"), "{report}");
    assert!(
        report.contains("overlapping statement periods chained anyway: 1"),
        "{report}"
    );
}

#[test]
fn files_without_nav_table_are_not_records() {
    let report = report_files(&["risk_d1.csv", "activity_sample.csv"], 9);
    assert!(report.contains("\"records\":[]"), "{report}");
    assert!(report.contains("\"chain\":null"), "{report}");
}

#[test]
fn fixtures_match_vector() {
    let report = report_files(&FIXTURES, 7);
    let want = std::fs::read_to_string(format!(
        "{}/../../tests/vectors/twr.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("vector missing");
    let prefix = "{\"files\":3,\"targets\":{\"twr\":";
    let rest = report.strip_prefix(prefix).expect("report shape");
    assert_eq!(&rest[..rest.len() - 2], want.trim_end());
}

#[test]
fn chain_invariant_under_file_order() {
    let whole = report_files(&FIXTURES, usize::MAX);
    let shuffled = report_files(&[FIXTURES[2], FIXTURES[0], FIXTURES[1]], 13);
    let chain = |s: &str| {
        let a = s.find("\"chain\":").expect("chain");
        s[a..s.find("\"notes\"").expect("notes")].to_string()
    };
    assert_eq!(chain(&shuffled), chain(&whole));
    for size in [1usize, 97, 4096] {
        assert_eq!(report_files(&FIXTURES, size), whole, "chunk {size}");
    }
}
