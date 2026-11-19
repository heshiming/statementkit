//! `xirr` target tests.

use statementkit::Session;

fn report_csvs(items: &[&[u8]], chunk: usize) -> String {
    let mut s = Session::open(&["xirr"]).expect("target exists");
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
fn dated_flows_solve_and_annualize() {
    let report = report_csvs(&[&fixture("xirr_annual.csv")], 7);
    // 1,000,000(1+r) + 200,000(1+r)^(292/364) - 100,000(1+r)^(152/364)
    //   = 1,300,000 -> r = 0.178888 per period, 0.179554 annualized
    assert!(report.contains("\"flow_source\":\"dated\""), "{report}");
    assert!(report.contains("\"xirr_period\":0.178888"), "{report}");
    assert!(report.contains("\"xirr_annualized\":0.179554"), "{report}");
    assert!(report.contains("\"date\":\"2026-03-14\""), "{report}");
    // same P&L (200,000) over the three denominators
    assert!(report.contains("\"twr_style\":0.2"), "{report}");
    assert!(report.contains("\"roi_style\":0.153846"), "{report}");
    assert!(report.contains("\"roai_style\":0.178782"), "{report}");
}

#[test]
fn non_base_currency_flow_dropped_and_counted() {
    let report = report_csvs(&[&fixture("xirr_annual.csv")], 3);
    assert!(!report.contains("\"date\":\"2026-06-02\""), "{report}");
    assert!(
        report.contains("dated flows in a currency other than the base, dropped: 1"),
        "{report}"
    );
}

#[test]
fn net_key_becomes_one_midpoint_flow() {
    // monthly: -50,000 at Sept 16 (d0 + round(29/2)), exponent 14/29
    let report = report_csvs(&[&fixture("monthly_sample.csv")], 9);
    assert!(report.contains("\"flow_source\":\"net-key\""), "{report}");
    assert!(report.contains("\"date\":\"2026-09-16\""), "{report}");
    assert!(report.contains("\"xirr_period\":0.031697"), "{report}");
    assert!(report.contains("\"xirr_annualized\":0.481445"), "{report}");
    assert!(!report.contains("xirr_note"), "{report}");
}

#[test]
fn no_flow_month_still_gets_ratios() {
    let csv = b"Statement,Data,Currency,USD\n\
                Statement,Data,Period,\"April 1, 2026 - April 30, 2026\"\n\
                Change in Combined NAV,Header,Field,Value\n\
                Change in NAV,Data,Deposits & Withdrawals,0\n\
                Net Asset Value,Header,Asset Class,Prior Total,Current Long,Current Short,Current Total,Change\n\
                Net Asset Value,Data,Total,1000,1100,0,1100,100\n";
    let report = report_csvs(&[csv], 5);
    assert!(report.contains("\"flow_source\":\"none\""), "{report}");
    assert!(report.contains("\"xirr_note\":\"no flow\""), "{report}");
    assert!(report.contains("\"xirr_period\":null"), "{report}");
    assert!(report.contains("\"twr_style\":0.1"), "{report}");
}

#[test]
fn dated_net_mismatch_counted_in_notes() {
    let csv = b"Statement,Data,Currency,USD\n\
                Statement,Data,Period,\"January 1, 2026 - December 31, 2026\"\n\
                Change in Combined NAV,Header,Field,Value\n\
                Change in NAV,Data,Deposits & Withdrawals,999\n\
                Deposits & Withdrawals,Header,Currency,Account,Settle Date,Description,Amount\n\
                Deposits & Withdrawals,Data,USD,A1,\"March 1, 2026\",Deposit,1000\n\
                Net Asset Value,Header,Asset Class,Prior Total,Current Long,Current Short,Current Total,Change\n\
                Net Asset Value,Data,Total,1000,2000,0,2000,1000\n";
    let report = report_csvs(&[csv], 4);
    assert!(
        report.contains("dated flows do not sum to the USD net key: 1"),
        "{report}"
    );
}

#[test]
fn flow_outside_period_dropped_and_counted() {
    let csv = b"Statement,Data,Currency,USD\n\
                Statement,Data,Period,\"June 1, 2026 - June 30, 2026\"\n\
                Change in Combined NAV,Header,Field,Value\n\
                Change in NAV,Data,Deposits & Withdrawals,500\n\
                Deposits & Withdrawals,Header,Currency,Account,Settle Date,Description,Amount\n\
                Deposits & Withdrawals,Data,USD,A1,\"May 5, 2026\",Deposit,500\n\
                Net Asset Value,Header,Asset Class,Prior Total,Current Long,Current Short,Current Total,Change\n\
                Net Asset Value,Data,Total,1000,1500,0,1500,500\n";
    let report = report_csvs(&[csv], 6);
    assert!(
        report.contains("dated flows outside the statement period, dropped: 1"),
        "{report}"
    );
    // dated list becomes empty -> net key leg takes over at midpoint
    assert!(report.contains("\"flow_source\":\"net-key\""), "{report}");
}

#[test]
fn no_solve_note_on_zero_current_total() {
    let csv = b"Statement,Data,Currency,USD\n\
                Statement,Data,Period,\"June 1, 2026 - June 30, 2026\"\n\
                Change in Combined NAV,Header,Field,Value\n\
                Change in NAV,Data,Deposits & Withdrawals,-100\n\
                Net Asset Value,Header,Asset Class,Prior Total,Current Long,Current Short,Current Total,Change\n\
                Net Asset Value,Data,Total,1000,0,0,0,-1000\n";
    let report = report_csvs(&[csv], 8);
    assert!(report.contains("\"xirr_note\":\"no solve\""), "{report}");
    assert!(report.contains("\"xirr_period\":null"), "{report}");
    assert!(report.contains("\"roi_style\":null"), "{report}");
}

#[test]
fn fixtures_match_vector() {
    let report = report_csvs(
        &[&fixture("monthly_sample.csv"), &fixture("xirr_annual.csv")],
        7,
    );
    let want = std::fs::read_to_string(format!(
        "{}/../../tests/vectors/xirr.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("vector missing");
    let prefix = "{\"files\":2,\"targets\":{\"xirr\":";
    let rest = report.strip_prefix(prefix).expect("report shape");
    assert_eq!(&rest[..rest.len() - 2], want.trim_end());
}

#[test]
fn chunk_invariance() {
    let (a, m) = (fixture("xirr_annual.csv"), fixture("monthly_sample.csv"));
    let whole = report_csvs(&[&a, &m], usize::MAX);
    for size in [1usize, 17, 4096] {
        assert_eq!(report_csvs(&[&a, &m], size), whole, "chunk {size}");
    }
}
