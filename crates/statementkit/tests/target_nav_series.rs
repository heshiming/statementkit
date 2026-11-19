//! `nav-series` target tests.

use statementkit::Session;

fn report_csv(csvs: &[&[u8]]) -> String {
    let mut s = Session::open(&["nav-series"]).expect("target exists");
    for bytes in csvs {
        for chunk in bytes.chunks(7) {
            s.feed(chunk);
        }
        s.end_file();
    }
    s.report_text()
}

fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/../../tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(path).expect("fixture missing")
}

fn fragment(report: &str) -> &str {
    let prefix = "{\"files\":2,\"targets\":{\"nav-series\":";
    let Some(rest) = report.strip_prefix(prefix) else {
        panic!("unexpected report shape: {report}")
    };
    &rest[..rest.len() - 2]
}

#[test]
fn fixtures_match_vector() {
    let report = report_csv(&[&fixture("daily_sample.csv"), &fixture("monthly_sample.csv")]);
    let want = std::fs::read_to_string(format!(
        "{}/../../tests/vectors/nav_series.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("vector missing");
    assert_eq!(fragment(&report), want.trim_end());
}

#[test]
fn fixture_report_is_chunk_invariant() {
    let (d, m) = (fixture("daily_sample.csv"), fixture("monthly_sample.csv"));
    let mut slow = Session::open(&["nav-series"]).unwrap();
    for bytes in [&d, &m] {
        for b in bytes.iter() {
            slow.feed(std::slice::from_ref(b));
        }
        slow.end_file();
    }
    assert_eq!(slow.report_text(), report_csv(&[&d, &m]));
}

#[test]
fn single_day_record_exact_text() {
    let csv = b"Statement,Data,Period,\"May 5, 2026\"\n\
                Change in Combined NAV,Header,Field,Value\n\
                Change in NAV,Data,Starting Value,100\n\
                Change in NAV,Data,Ending Value,115\n\
                Change in NAV,Data,Mark-to-Market,20\n\
                Change in NAV,Data,Deposits & Withdrawals,-5\n";
    let report = report_csv(&[csv]);
    let want = "{\"files\":1,\"targets\":{\"nav-series\":{\"records\":[{\
        \"period\":\"May 5, 2026\",\"period_from\":\"2026-05-05\",\"period_to\":\"2026-05-05\",\
        \"grain\":\"day\",\"starting_value\":100,\"ending_value\":115,\"day_pnl\":20,\
        \"components\":{\"Mark-to-Market\":20,\"Interest\":null,\"Dividends\":null,\
        \"Commissions\":null,\"Withholding Tax\":null,\"Change in Interest Accruals\":null,\
        \"Change in Dividend Accruals\":null,\"Other FX Translations\":null,\
        \"Position Transfers\":null,\"Deposits & Withdrawals\":-5},\
        \"identity_exact\":true}],\
        \"notes\":[\"absent component keys are null and count as 0 in the identity\"]}}}";
    assert_eq!(report, want);
}

#[test]
fn broken_identity_is_reported() {
    let csv = b"Statement,Data,Period,\"May 5, 2026\"\n\
                Change in Combined NAV,Header,Field,Value\n\
                Change in NAV,Data,Starting Value,100\n\
                Change in NAV,Data,Ending Value,115\n\
                Change in NAV,Data,Mark-to-Market,20\n";
    let report = report_csv(&[csv]);
    assert!(report.contains("\"identity_exact\":false"));
    assert!(report.contains("failing the End-Start = sum(keys) identity: 1"));
}

#[test]
fn unparsable_value_is_counted_and_null() {
    let csv = b"Statement,Data,Period,\"May 5, 2026\"\n\
                Change in Combined NAV,Header,Field,Value\n\
                Change in NAV,Data,Starting Value,100\n\
                Change in NAV,Data,Ending Value,115\n\
                Change in NAV,Data,Mark-to-Market,NaN\n\
                Change in NAV,Data,Deposits & Withdrawals,5\n";
    let report = report_csv(&[csv]);
    assert!(report.contains("\"Mark-to-Market\":null"));
    assert!(report.contains("EAV values that failed to parse: 1"));
    // NaN counted as 0: 100 + 0 - 5 == 95 != 115.
    assert!(report.contains("\"identity_exact\":false"));
}

#[test]
fn two_blocks_in_one_file_make_two_records() {
    let block = "Change in Combined NAV,Header,Field,Value\n\
                 Change in NAV,Data,Starting Value,1\n\
                 Change in NAV,Data,Ending Value,2\n\
                 Change in NAV,Data,Mark-to-Market,1\n";
    let csv = format!("Statement,Data,Period,\"May 5, 2026\"\n{block}{block}");
    let report = report_csv(&[csv.as_bytes()]);
    assert_eq!(report.match_indices("\"grain\":\"day\"").count(), 2);
}

#[test]
fn missing_period_gives_unknown_grain() {
    let csv = b"Change in Combined NAV,Header,Field,Value\n\
                Change in NAV,Data,Starting Value,1\n\
                Change in NAV,Data,Ending Value,1\n";
    let report = report_csv(&[csv]);
    assert!(report.contains("\"grain\":\"unknown\""));
    assert!(report.contains("unparsable statement Period: 1"));
}

#[test]
fn unknown_target_names_are_rejected_with_the_list() {
    match Session::open(&["nope"]) {
        Ok(_) => panic!("must not open"),
        Err(e) => {
            let msg = e.to_string();
            assert!(msg.contains("nope"), "{msg}");
            assert!(msg.contains("nav-series"), "{msg}");
        }
    }
}
