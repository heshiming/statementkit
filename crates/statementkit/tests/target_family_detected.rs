//! `family-detected` target tests.

use statementkit::Session;

fn report_fixture_set(chunk: usize) -> String {
    let names = ["family_m1.csv", "family_m2.csv", "family_m3.csv"];
    let mut s = Session::open(&["family-detected"]).expect("target exists");
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
fn combined_file_signals() {
    let report = report_fixture_set(7);
    assert!(report.contains("\"combined_marker\":true"), "{report}");
    assert!(report.contains("\"legs\":2"), "{report}");
    assert!(report.contains("\"master_name\":true"), "{report}");
    assert!(report.contains("\"nav_twin\":\"twin\""), "{report}");
}

#[test]
fn nav_only_file_detected() {
    let report = report_fixture_set(5);
    assert!(report.contains("\"nav_twin\":\"nav-only\""), "{report}");
    assert!(report.contains("\"nav_only_files\":1"), "{report}");
}

#[test]
fn member_rows_and_combine_reason() {
    let report = report_fixture_set(6);
    // m1 posts one D&W row naming the listed member
    assert!(report.contains("\"member_rows\":1"), "{report}");
    assert!(
        report.contains("\"combine_reason\":\"securities + digital-asset accounts\""),
        "{report}"
    );
    assert!(
        report.contains("\"combine_reason\":\"no combined-statement disclaimer\""),
        "{report}"
    );
}

#[test]
fn account_columns_counted() {
    let report = report_fixture_set(4);
    // m1: Trades + Dividends + D&W carry an Account column
    assert!(report.contains("\"account_column_sections\":3"), "{report}");
}

#[test]
fn master_shape_without_marker() {
    let report = report_fixture_set(3);
    assert!(
        report.contains("\"account_type\":\"Institution Master\""),
        "{report}"
    );
    assert!(report.contains("\"combined_marker\":false"), "{report}");
}

#[test]
fn fixtures_match_vector() {
    let report = report_fixture_set(7);
    let want = std::fs::read_to_string(format!(
        "{}/../../tests/vectors/family_detected.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("vector missing");
    let prefix = "{\"files\":3,\"targets\":{\"family-detected\":";
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
