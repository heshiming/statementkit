//! `realized-stlt` target tests.

use statementkit::Session;

fn report_csvs(items: &[&[u8]], chunk: usize) -> String {
    let mut s = Session::open(&["realized-stlt"]).expect("target exists");
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
fn sums_symbol_rows_excluding_total() {
    let report = report_csvs(&[&fixture("realized_stlt_m1.csv")], 7);
    assert!(
        report.contains(
            "{\"account\":\"AAA0000001\",\"year\":\"2026\",\"st_profit\":12100,\
             \"st_loss\":-3000,\"lt_profit\":40200,\"lt_loss\":-50,\
             \"realized_total\":49250"
        ),
        "{report}"
    );
    assert!(report.contains("\"symbol_rows\":2"), "{report}");
    assert!(!report.contains("999999"), "Total row leaked: {report}");
}

#[test]
fn unrealized_columns_included_when_present() {
    let report = report_csvs(&[&fixture("realized_stlt_m1.csv")], 5);
    assert!(
        report.contains(
            "\"unreal_st_profit\":500,\"unreal_st_loss\":-100,\
             \"unreal_lt_profit\":10,\"unreal_lt_loss\":-5"
        ),
        "{report}"
    );
}

#[test]
fn same_account_year_files_sum_together() {
    let report = report_csvs(
        &[
            &fixture("realized_stlt_m1.csv"),
            &fixture("realized_stlt_m1.csv"),
        ],
        6,
    );
    assert!(report.contains("\"st_profit\":24200"), "{report}");
    assert!(report.contains("\"symbol_rows\":4"), "{report}");
}

#[test]
fn no_summary_section_yields_empty() {
    let report = report_csvs(&[&fixture("quality_m1.csv")], 8);
    assert!(report.contains("\"account_years\":[]"), "{report}");
}

#[test]
fn third_file_returns_to_earlier_account_year() {
    let mk = |acct: &str, year: &str| {
        format!(
            "Statement,Data,Account ID,{acct}\nStatement,Data,Currency,USD\n\
             Statement,Data,Period,\"January 1, {year} - December 31, {year}\"\n\
             Realized & Unrealized Performance Summary,Header,Symbol,Realized S/T Profit,Realized S/T Loss,Realized L/T Profit,Realized L/T Loss,Asset Category,Currency\n\
             Realized & Unrealized Performance Summary,Data,FOO,10,0,0,0,Stock,USD\n"
        )
    };
    let (a, b) = (mk("AAA", "2025"), mk("BBB", "2026"));
    let c = mk("AAA", "2025");
    let report = report_csvs(&[a.as_bytes(), b.as_bytes(), c.as_bytes()], 9);
    assert!(
        report.contains("{\"account\":\"AAA\",\"year\":\"2025\",\"st_profit\":20"),
        "{report}"
    );
    assert!(
        report.contains("{\"account\":\"BBB\",\"year\":\"2026\",\"st_profit\":10"),
        "{report}"
    );
}

#[test]
fn fixtures_match_vector() {
    let report = report_csvs(&[&fixture("realized_stlt_m1.csv")], 7);
    let want = std::fs::read_to_string(format!(
        "{}/../../tests/vectors/realized_stlt.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("vector missing");
    let prefix = "{\"files\":1,\"targets\":{\"realized-stlt\":";
    let rest = report.strip_prefix(prefix).expect("report shape");
    assert_eq!(&rest[..rest.len() - 2], want.trim_end());
}

#[test]
fn chunk_invariance() {
    let one = fixture("realized_stlt_m1.csv");
    let whole = report_csvs(&[&one], usize::MAX);
    for size in [1usize, 31, 4096] {
        assert_eq!(report_csvs(&[&one], size), whole, "chunk {size}");
    }
}
