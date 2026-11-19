//! `dividends` target tests.

use statementkit::Session;

fn report_csvs(items: &[&[u8]], chunk: usize) -> String {
    let mut s = Session::open(&["dividends"]).expect("target exists");
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
fn cash_view_calendar_by_pay_month() {
    let report = report_csvs(&[&fixture("dividends_m1.csv")], 7);
    assert!(
        report.contains(
            "{\"month\":\"2025-03\",\"net\":400},{\"month\":\"2025-04\",\
             \"net\":400},{\"month\":\"2026-03\",\"net\":1530},\
             {\"month\":\"2026-06\",\"net\":1530}"
        ),
        "{report}"
    );
}

#[test]
fn trailing_12_window_and_run_rate() {
    let report = report_csvs(&[&fixture("dividends_m1.csv")], 5);
    // last pay month 2026-06: window 2025-07..2026-06 excludes 2025 rows
    assert!(
        report.contains("\"trailing_12m\":{\"months\":2,\"net\":3060,\"run_rate_monthly\":255}"),
        "{report}"
    );
}

#[test]
fn forward_uses_same_quarter_prior_year() {
    let report = report_csvs(&[&fixture("dividends_m1.csv")], 9);
    assert!(
        report
            .contains("\"forward_same_quarter_prior_year\":{\"quarter\":\"Q2 2025\",\"net\":400}"),
        "{report}"
    );
}

#[test]
fn gross_rates_quoted_never_recomputed() {
    let report = report_csvs(&[&fixture("dividends_m1.csv")], 4);
    assert!(
        report.contains("{\"symbol\":\"XYZ\",\"net\":3060,\"quoted_gross_rates\":[2.4]}"),
        "{report}"
    );
}

#[test]
fn posting_unmatched_by_pay_month_counted_not_rendered() {
    let report = report_csvs(&[&fixture("dividends_m1.csv")], 6);
    assert!(
        report.contains("\"postings\":{\"rows\":2,\"net\":1607,\"unmatched_by_pay_month\":1}"),
        "{report}"
    );
    // free-text Description never reaches the output
    assert!(!report.contains("DIVIDEND"), "{report}");
}

#[test]
fn withholding_flag_tracks_section_presence() {
    let report = report_csvs(&[&fixture("dividends_m1.csv")], 8);
    assert!(
        report.contains("\"withholding\":{\"present\":\"true\",\"amount\":-540}"),
        "{report}"
    );
    let report = report_csvs(&[&fixture("quality_m1.csv")], 8);
    assert!(
        report.contains("\"withholding\":{\"present\":\"false\"}"),
        "{report}"
    );
}

#[test]
fn month_key_forms_all_parse() {
    use statementkit::dates::month_key;
    assert_eq!(month_key("20260315").as_deref(), Some("2026-03"));
    assert_eq!(
        month_key("2026-03-15, 10:00:00").as_deref(),
        Some("2026-03")
    );
    assert_eq!(month_key("March 15, 2026").as_deref(), Some("2026-03"));
    assert_eq!(month_key("2026, March").as_deref(), Some("2026-03"));
    assert_eq!(month_key("n/a"), None);
}

#[test]
fn fixtures_match_vector() {
    let report = report_csvs(&[&fixture("dividends_m1.csv")], 7);
    let want = std::fs::read_to_string(format!(
        "{}/../../tests/vectors/dividends.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("vector missing");
    let prefix = "{\"files\":1,\"targets\":{\"dividends\":";
    let rest = report.strip_prefix(prefix).expect("report shape");
    assert_eq!(&rest[..rest.len() - 2], want.trim_end());
}

#[test]
fn chunk_invariance() {
    let one = fixture("dividends_m1.csv");
    let whole = report_csvs(&[&one], usize::MAX);
    for size in [1usize, 31, 4096] {
        assert_eq!(report_csvs(&[&one], size), whole, "chunk {size}");
    }
}
