//! `ticker-map` target tests.

use statementkit::Session;

fn report_fixture_set(chunk: usize) -> String {
    let names = ["ticker_m1.csv"];
    let mut s = Session::open(&["ticker-map"]).expect("target exists");
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
fn carriers_match_the_verified_shapes() {
    let report = report_fixture_set(7);
    assert!(
        report.contains("\"Trades\":{\"carrier\":\"symbol-column\",\"rows\":3,\"with_ticker\":3}"),
        "{report}"
    );
    assert!(
        report.contains(
            "\"Dividends\":{\"carrier\":\"description-prefix\",\"rows\":2,\"with_ticker\":1}"
        ),
        "{report}"
    );
    assert!(
        report.contains("\"Interest\":{\"carrier\":\"none\",\"rows\":1,\"with_ticker\":0}"),
        "{report}"
    );
}

#[test]
fn header_only_sections_are_dropped() {
    let report = report_fixture_set(5);
    assert!(!report.contains("\"Statement\":{\"carrier\""), "{report}");
}

#[test]
fn vocabulary_is_the_files_own_symbol_set() {
    let report = report_fixture_set(6);
    assert!(report.contains("\"vocabulary_symbols\":3"), "{report}");
}

#[test]
fn untitled_dividend_rows_are_counted() {
    let report = report_fixture_set(4);
    assert!(
        report.contains("empty Description (unattributable, counted): 1"),
        "{report}"
    );
}

#[test]
fn fixtures_match_vector() {
    let report = report_fixture_set(7);
    let want = std::fs::read_to_string(format!(
        "{}/../../tests/vectors/ticker_map.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("vector missing");
    let prefix = "{\"files\":1,\"targets\":{\"ticker-map\":";
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
