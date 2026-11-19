//! `financing-legs` target tests.

use statementkit::Session;

fn report_fixture_set(chunk: usize) -> String {
    let names = ["fin_m1.csv", "fin_m2.csv"];
    let mut s = Session::open(&["financing-legs"]).expect("target exists");
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
fn buckets_by_description_not_sign() {
    let report = report_fixture_set(7);
    // the +150 INT CHG reversal stays on the debit side: 2 rows, -650
    assert!(
        report.contains("{\"bucket\":\"debit\",\"rows\":2,\"usd\":-650}"),
        "{report}"
    );
    assert!(
        report.contains("{\"bucket\":\"syep\",\"rows\":1,\"usd\":120}"),
        "{report}"
    );
    assert!(
        report.contains("{\"bucket\":\"credit\",\"rows\":1,\"usd\":500}"),
        "{report}"
    );
}

#[test]
fn member_without_interest_flagged() {
    let report = report_fixture_set(5);
    assert!(
        report.contains("\"members_without_interest\":[\"U10000025\"]"),
        "{report}"
    );
    assert!(report.contains("books no funding cost"), "{report}");
}

#[test]
fn non_usd_counted_not_summed() {
    let report = report_fixture_set(6);
    assert!(report.contains("\"non_usd_rows\":1"), "{report}");
    // the EUR row's 70 never reaches any bucket
    assert!(!report.contains("\"usd\":570"), "{report}");
}

#[test]
fn charge_not_invented() {
    let report = report_fixture_set(4);
    assert!(report.contains("amounts, not rates"), "{report}");
}

#[test]
fn fixtures_match_vector() {
    let report = report_fixture_set(7);
    let want = std::fs::read_to_string(format!(
        "{}/../../tests/vectors/financing_legs.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("vector missing");
    let prefix = "{\"files\":2,\"targets\":{\"financing-legs\":";
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
