//! `weights` target tests.

use statementkit::Session;

fn report_csvs(items: &[&[u8]], chunk: usize) -> String {
    let mut s = Session::open(&["weights"]).expect("target exists");
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
fn class_stack_signed_weights_with_fx_conversion() {
    let report = report_csvs(&[&fixture("weight_d1.csv")], 9);
    assert!(report.contains("\"date\":\"2026-09-08\""), "{report}");
    assert!(report.contains("\"nav_total\":985500"), "{report}");
    // 770000/985500, 235500/985500, -20000/985500
    assert!(report.contains("\"weight\":0.781329"), "{report}");
    assert!(report.contains("\"weight\":0.238965"), "{report}");
    assert!(report.contains("\"weight\":-0.020294"), "{report}");
    // EUR 5000 at rate 1.10 = 5500 USD
    assert!(
        report.contains("\"value_usd\":5500,\"weight\":0.005581"),
        "{report}"
    );
    assert!(
        report.contains("\"closure\":{\"pass\":true,\"gap\":0}"),
        "{report}"
    );
}

#[test]
fn indented_subrows_are_not_classes() {
    let report = report_csvs(&[&fixture("weight_d1.csv")], 5);
    assert!(!report.contains("Broker Assets"), "{report}");
}

#[test]
fn option_legs_group_into_units_net_and_gross() {
    let report = report_csvs(&[&fixture("weight_d1.csv")], 7);
    assert!(
        report.contains(
            "\"unit\":\"SPY 18SEP26\",\"net_usd\":-20000,\"gross_usd\":70000,\
             \"net_weight\":-0.020294,\"gross_weight\":0.07103"
        ),
        "{report}"
    );
}

#[test]
fn futures_are_overlay_not_slice() {
    let report = report_csvs(&[&fixture("weight_d1.csv")], 11);
    assert!(!report.contains("\"class\":\"Futures\""), "{report}");
    assert!(
        report.contains("\"futures_overlay\":{\"value_usd\":-420000,\"weight\":-0.42618}"),
        "{report}"
    );
}

#[test]
fn unrateable_currency_rows_excluded_and_named() {
    let report = report_csvs(&[&fixture("weight_d2.csv")], 6);
    assert!(
        report.contains("1 rows in JPY excluded: no rate in this file"),
        "{report}"
    );
    // closing still passes: the JPY row is not part of NAV
    assert!(
        report.contains("\"closure\":{\"pass\":true,\"gap\":0}"),
        "{report}"
    );
}

#[test]
fn closure_failure_reported_as_gap() {
    let broken = b"Statement,Data,Period,\"September 8, 2026\"\n\
                   Statement,Data,Currency,USD\n\
                   Open Positions,Header,Symbol,Quantity,Close Price,Mult,Value,Cost Basis,Unrealized P/L,Asset Category,Currency,DataDiscriminator\n\
                   Open Positions,Data,STK1,1000,250.00,1,250000,-240000,10000,Stock,USD,Summary\n\
                   Net Asset Value,Header,Asset Class,Prior Total,Current Total,Change,Change%\n\
                   Net Asset Value,Data,Cash,800000.00,800000.00,0.00,0\n\
                   Net Asset Value,Data,Total,1100000.00,1100000.00,0.00,0\n";
    let report = report_csvs(&[broken], 8);
    // gap = 1100000 - 800000 - 250000 = 50000
    assert!(
        report.contains("\"closure\":{\"pass\":false,\"gap\":50000}"),
        "{report}"
    );
    assert!(report.contains("closure gap 50000"), "{report}");
    assert!(report.contains("files failing closure: 1"), "{report}");
}

#[test]
fn fixtures_match_vector() {
    let report = report_csvs(&[&fixture("weight_d1.csv"), &fixture("weight_d2.csv")], 7);
    let want = std::fs::read_to_string(format!(
        "{}/../../tests/vectors/weights.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("vector missing");
    let prefix = "{\"files\":2,\"targets\":{\"weights\":";
    let rest = report.strip_prefix(prefix).expect("report shape");
    assert_eq!(&rest[..rest.len() - 2], want.trim_end());
}

#[test]
fn chunk_invariance() {
    let items = [fixture("weight_d1.csv"), fixture("weight_d2.csv")];
    let refs: Vec<&[u8]> = items.iter().map(|v| v.as_slice()).collect();
    let whole = report_csvs(&refs, usize::MAX);
    for size in [1usize, 31, 4096] {
        assert_eq!(report_csvs(&refs, size), whole, "chunk {size}");
    }
}
