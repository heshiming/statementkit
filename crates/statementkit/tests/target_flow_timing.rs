//! `flow-timing` target tests.

use statementkit::Session;

fn report_fixture_set(chunk: usize) -> String {
    let names = ["flow_m1.csv", "flow_m2.csv", "flow_m3.csv", "flow_m4.csv"];
    let mut s = Session::open(&["flow-timing"]).expect("target exists");
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

// docs 05 §9 shape: chain 1.09 x 1.0 x 0.88 x 1.0 -> -4.08% total.
#[test]
fn chain_twr_from_flow_adjusted_periods() {
    let report = report_fixture_set(7);
    assert!(report.contains("\"twr\":{\"total\":-0.0408"), "{report}");
}

#[test]
fn deposit_after_strength_classified() {
    let report = report_fixture_set(5);
    assert!(report.contains("\"prior_drift\":0.09"), "{report}");
    assert!(report.contains("\"class\":\"bought-strength\""), "{report}");
}

#[test]
fn withdrawal_after_weakness_classified() {
    let report = report_fixture_set(6);
    assert!(report.contains("\"class\":\"sold-weakness\""), "{report}");
    assert!(report.contains("\"flow_periods\":2"), "{report}");
}

#[test]
fn dated_rows_listed_beside_curve() {
    let report = report_fixture_set(4);
    assert!(
        report.contains("{\"date\":\"2026-01-05\",\"amount_usd\":300000}"),
        "{report}"
    );
    assert!(report.contains("\"amount_usd\":-200000"), "{report}");
}

#[test]
fn gap_is_xirr_minus_twr() {
    let report = report_fixture_set(3);
    let x = report
        .find("\"mwr_xirr_annualized\":")
        .map(|i| &report[i..])
        .expect("xirr field");
    assert!(x.starts_with("\"mwr_xirr_annualized\":0."), "{report}");
    assert!(report.contains("\"behavior_gap_pp_yr\":"), "{report}");
    assert!(report.contains("narrative, not statistics"), "{report}");
}

#[test]
fn fixtures_match_vector() {
    let report = report_fixture_set(7);
    let want = std::fs::read_to_string(format!(
        "{}/../../tests/vectors/flow_timing.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("vector missing");
    let prefix = "{\"files\":4,\"targets\":{\"flow-timing\":";
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
