//! `family-stated` target tests.

use statementkit::Session;

fn report_fixture_set(chunk: usize) -> String {
    let names = ["family_m1.csv", "family_m2.csv"];
    let mut s = Session::open(&["family-stated"]).expect("target exists");
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
fn stated_membership_and_types() {
    let report = report_fixture_set(7);
    assert!(
        report.contains("\"marker\":\"(Custom Consolidated)\""),
        "{report}"
    );
    assert!(report.contains("\"members\":[\"U10000025\"]"), "{report}");
    assert!(
        report.contains("\"account_type\":\"Institution Master\""),
        "{report}"
    );
    assert!(report.contains("\"members\":null"), "{report}");
}

#[test]
fn tagged_rows_counted_per_section() {
    let report = report_fixture_set(5);
    assert!(
        report.contains("{\"section\":\"Trades\",\"rows\":2}"),
        "{report}"
    );
    assert!(
        report.contains("{\"section\":\"Dividends\",\"rows\":1}"),
        "{report}"
    );
}

// marker variant and bare id of one account land under one key
#[test]
fn account_marker_does_not_split_counts() {
    let report = report_fixture_set(6);
    let first = report
        .find("{\"account\":\"AAA0000001\",\"sections\"")
        .expect("primary sections block");
    let second = report[first + 1..]
        .find("{\"account\":\"AAA0000001\",\"sections\"")
        .is_none();
    assert!(second, "{report}");
}

#[test]
fn member_named_rows_counted() {
    let report = report_fixture_set(4);
    // the fixture posts one D&W row naming the listed member
    assert!(report.contains("\"member_named_rows\":1"), "{report}");
}

#[test]
fn family_blocks_and_twr_cells_recorded() {
    let report = report_fixture_set(3);
    assert!(report.contains("\"nav_files\":1"), "{report}");
    assert!(report.contains("\"combined_nav_files\":1"), "{report}");
    assert!(report.contains("\"twr_cells\":1"), "{report}");
    assert!(report.contains("cells are the family's"), "{report}");
}

#[test]
fn no_margin_sections_found() {
    let report = report_fixture_set(8);
    assert!(report.contains("\"margin_like_sections\":[]"), "{report}");
}

#[test]
fn fixtures_match_vector() {
    let report = report_fixture_set(7);
    let want = std::fs::read_to_string(format!(
        "{}/../../tests/vectors/family_stated.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("vector missing");
    let prefix = "{\"files\":2,\"targets\":{\"family-stated\":";
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
