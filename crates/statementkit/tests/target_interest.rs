//! `interest` target tests.

use statementkit::Session;

fn report_files(names: &[&str], chunk: usize) -> String {
    let mut s = Session::open(&["interest"]).expect("target exists");
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

const BOTH: [&str; 2] = ["interest_d1.csv", "interest_m1.csv"];

#[test]
fn dedup_and_sign_classification() {
    let report = report_files(&BOTH, 7);
    // 9 raw rows, one JPY row repeated across the overlapping pair
    assert!(report.contains("\"rows\":8"), "{report}");
    assert!(
        report.contains("duplicate rows dropped (daily/monthly overlap): 1"),
        "{report}"
    );
    // the `Credit Interest from <acct>` row is booked negative: sign wins
    assert!(report.contains(
        "\"bucket\":\"credit\",\"currency_group\":\"base\",\"rows\":3,\"usd_received\":170,\"usd_paid\":200"
    ), "{report}");
    assert!(report.contains("names a related account: 1"), "{report}");
}

#[test]
fn forex_split_at_statement_rates() {
    let report = report_files(&BOTH, 5);
    // JPY 4500 * 0.0068 = 30.6 received
    assert!(report.contains("\"usd_received\":30.6"), "{report}");
    // GBP 30*1.27 + CAD 5*0.73 = 41.75 paid
    assert!(report.contains(
        "\"bucket\":\"debit\",\"currency_group\":\"forex\",\"rows\":2,\"usd_received\":0,\"usd_paid\":41.75"
    ), "{report}");
    assert!(report.contains("\"usd_received\":200.6"), "{report}");
    assert!(report.contains("\"usd_paid\":253.75"), "{report}");
}

#[test]
fn unconvertible_currency_is_counted_not_guessed() {
    let report = report_files(&BOTH, 3);
    assert!(
        report.contains("currencies with no fed rate, rows excluded from the USD split: ZAR"),
        "{report}"
    );
}

#[test]
fn reconciliation_residual_against_nav_anchor() {
    let report = report_files(&BOTH, 4096);
    // anchor = Interest 108.90 + Change in Interest Accruals -47.00
    assert!(report.contains("\"net\":-53.15"), "{report}");
    assert!(report.contains("\"nav_anchor\":61.9"), "{report}");
    assert!(report.contains("\"residual\":-115.05"), "{report}");
}

#[test]
fn accrual_summary_block_totalled() {
    let report = report_files(&BOTH, 11);
    assert!(report.contains("\"Interest Accrued\":150"), "{report}");
    assert!(report.contains("\"Accrual Reversal\":-150"), "{report}");
    assert!(report.contains("\"FX Translation\":3"), "{report}");
    assert!(
        report.contains("\"Ending Accrual Balance\":503"),
        "{report}"
    );
}

#[test]
fn nearest_earlier_rate_is_a_counted_fallback() {
    let mut s = Session::open(&["interest"]).expect("open");
    let rate = b"Statement,Data,Currency,USD\n\
                 Statement,Data,Period,\"August 4, 2026\"\n\
                 Base Currency Exchange Rate,Header,Currency,Rate\n\
                 Base Currency Exchange Rate,Data,GBP,1.25\n";
    let row = b"Statement,Data,Currency,USD\n\
                Statement,Data,Period,\"August 20, 2026\"\n\
                Interest,Header,Currency,Date,Description,Amount\n\
                Interest,Data,GBP,\"August 10, 2026\",Debit Interest,(100.00)\n";
    let items: [(&[u8], usize); 2] = [(rate, 7), (row, 9)];
    for (bytes, size) in items {
        for part in bytes.chunks(size) {
            s.feed(part);
        }
        s.end_file();
    }
    let report = s.report_text();
    assert!(
        report.contains("nearest earlier statement date: 1"),
        "{report}"
    );
    assert!(report.contains("\"usd_paid\":125"), "{report}");
}

#[test]
fn fixtures_match_vector() {
    let report = report_files(&BOTH, 7);
    let want = std::fs::read_to_string(format!(
        "{}/../../tests/vectors/interest.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("vector missing");
    let prefix = "{\"files\":2,\"targets\":{\"interest\":";
    let rest = report.strip_prefix(prefix).expect("report shape");
    assert_eq!(&rest[..rest.len() - 2], want.trim_end());
}

#[test]
fn chunk_and_feed_order_invariance() {
    let (a, b) = (
        std::fs::read(format!(
            "{}/../../tests/fixtures/{}",
            env!("CARGO_MANIFEST_DIR"),
            BOTH[0]
        ))
        .unwrap(),
        std::fs::read(format!(
            "{}/../../tests/fixtures/{}",
            env!("CARGO_MANIFEST_DIR"),
            BOTH[1]
        ))
        .unwrap(),
    );
    let whole = report_files(&BOTH, usize::MAX);
    let swapped = {
        let mut s = Session::open(&["interest"]).expect("open");
        for bytes in [&b, &a] {
            s.feed(bytes);
            s.end_file();
        }
        s.report_text()
    };
    assert_eq!(swapped, whole, "file order must not change the report");
    for size in [1usize, 64, 4096] {
        assert_eq!(report_files(&BOTH, size), whole, "chunk {size}");
    }
}
