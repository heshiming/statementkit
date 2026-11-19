//! `income` target tests.

use statementkit::Session;

fn report_csvs(items: &[&[u8]], chunk: usize) -> String {
    let mut s = Session::open(&["income"]).expect("target exists");
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
fn account_year_book_from_postings() {
    let report = report_csvs(&[&fixture("income_m1.csv")], 7);
    assert!(
        report.contains(
            "{\"account\":\"AAA0000001\",\"year\":\"2026\",\"dividends\":60,\
             \"credit_interest\":12,\"debit_interest\":-30,\"borrow_fees\":-4,\
             \"syep\":14,\"insured_deposit\":7,\"borrow_fee_rows\":1,\
             \"withholding\":-4,\"commissions\":-5}"
        ),
        "{report}"
    );
}

#[test]
fn row_account_column_splits_books() {
    let report = report_csvs(&[&fixture("income_m1.csv")], 5);
    assert!(
        report.contains("{\"account\":\"BBB0000002\",\"year\":\"2025\",\"dividends\":11"),
        "{report}"
    );
}

#[test]
fn syep_captured_from_interest_row_and_details() {
    let report = report_csvs(&[&fixture("income_m1.csv")], 9);
    // Interest description 5 + details Interest Paid to Customer 9
    assert!(report.contains("\"syep\":14"), "{report}");
}

#[test]
fn accruals_stay_separate_from_postings() {
    let report = report_csvs(&[&fixture("income_m1.csv")], 4);
    assert!(
        report.contains(
            "\"accruals\":{\"dividend_gross\":100,\"dividend_tax\":-10,\
             \"dividend_net\":90,\"interest_accrued_paid\":-1,\
             \"interest_accrued_received\":2}"
        ),
        "{report}"
    );
    // dividends posting stays 60, unaffected by the 100 accrual gross
    assert!(report.contains("\"dividends\":60"), "{report}");
}

#[test]
fn year_forms_all_parse() {
    use statementkit::dates::year;
    assert_eq!(year("20260315").as_deref(), Some("2026"));
    assert_eq!(year("2026-03-15, 10:00:00").as_deref(), Some("2026"));
    assert_eq!(year("March 15, 2026").as_deref(), Some("2026"));
    assert_eq!(year("2026, March").as_deref(), Some("2026"));
    assert_eq!(year("n/a"), None);
}

#[test]
fn fixtures_match_vector() {
    let report = report_csvs(&[&fixture("income_m1.csv")], 7);
    let want = std::fs::read_to_string(format!(
        "{}/../../tests/vectors/income.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("vector missing");
    let prefix = "{\"files\":1,\"targets\":{\"income\":";
    let rest = report.strip_prefix(prefix).expect("report shape");
    assert_eq!(&rest[..rest.len() - 2], want.trim_end());
}

#[test]
fn chunk_invariance() {
    let one = fixture("income_m1.csv");
    let whole = report_csvs(&[&one], usize::MAX);
    for size in [1usize, 31, 4096] {
        assert_eq!(report_csvs(&[&one], size), whole, "chunk {size}");
    }
}
