//! `quality` target tests.

use statementkit::Session;

fn report_csvs(items: &[&[u8]], chunk: usize) -> String {
    let mut s = Session::open(&["quality"]).expect("target exists");
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
fn nav_identity_passes_on_balanced_file() {
    let report = report_csvs(&[&fixture("quality_m1.csv")], 7);
    assert!(
        report.contains("{\"check\":\"nav_pnl_identity\",\"ok\":1,\"bad\":0}"),
        "{report}"
    );
}

#[test]
fn nav_identity_fails_on_unbalanced_file() {
    let bad = b"Statement,Data,Period,\"May 1, 2026 - May 31, 2026\"\n\
                Statement,Data,Currency,USD\n\
                Change in Combined NAV,Header,Section,Key,Value,Basis,Currency\n\
                Change in Combined NAV,Data,Net Asset Value,Starting Value,1000,,USD\n\
                Change in Combined NAV,Data,Net Asset Value,Ending Value,1100,,USD\n\
                Change in Combined NAV,Data,Net Asset Value,Mark-to-Market Performance (Price),90,,USD\n";
    let report = report_csvs(&[&fixture("quality_m1.csv"), bad], 5);
    assert!(
        report.contains("{\"check\":\"nav_pnl_identity\",\"ok\":1,\"bad\":1}"),
        "{report}"
    );
}

#[test]
fn row_level_identities_counted() {
    let report = report_csvs(&[&fixture("quality_m1.csv")], 9);
    assert!(
        report.contains("{\"check\":\"position_value_identity\",\"ok\":3,\"bad\":1}"),
        "{report}"
    );
    assert!(
        report.contains("{\"check\":\"unrealized_identity\",\"ok\":3,\"bad\":1}"),
        "{report}"
    );
    assert!(
        report.contains("{\"check\":\"close_identity_rate\",\"ok\":1,\"bad\":1}"),
        "{report}"
    );
}

#[test]
fn open_row_artifact_share() {
    let report = report_csvs(&[&fixture("quality_m1.csv")], 4);
    assert!(
        report.contains("\"open_rows_with_realized\":{\"rows\":2,\"artifacts\":1,\"share\":0.5}"),
        "{report}"
    );
    assert!(report.contains("\"sign_mismatches\":0"), "{report}");
}

#[test]
fn borrow_detail_rows_counted_per_file() {
    let report = report_csvs(&[&fixture("quality_m1.csv")], 6);
    assert!(
        report.contains("\"borrow_detail_rows_by_file\":[{\"file_seq\":1,\"rows\":2}]"),
        "{report}"
    );
}

#[test]
fn fixtures_match_vector() {
    let report = report_csvs(&[&fixture("quality_m1.csv")], 7);
    let want = std::fs::read_to_string(format!(
        "{}/../../tests/vectors/quality.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("vector missing");
    let prefix = "{\"files\":1,\"targets\":{\"quality\":";
    let rest = report.strip_prefix(prefix).expect("report shape");
    assert_eq!(&rest[..rest.len() - 2], want.trim_end());
}

#[test]
fn chunk_invariance() {
    let one = fixture("quality_m1.csv");
    let whole = report_csvs(&[&one], usize::MAX);
    for size in [1usize, 31, 4096] {
        assert_eq!(report_csvs(&[&one], size), whole, "chunk {size}");
    }
}
