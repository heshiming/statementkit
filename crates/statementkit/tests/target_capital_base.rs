//! `capital-base` target tests.

use statementkit::Session;

fn report_fixture_set(chunk: usize) -> String {
    let names = ["cap_m1.csv", "cap_m2.csv", "cap_m3.csv"];
    let mut s = Session::open(&["capital-base"]).expect("target exists");
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
fn notional_bases_disclose_k() {
    let report = report_fixture_set(7);
    assert!(
        report.contains("{\"kind\":\"notional\",\"k\":100000"),
        "{report}"
    );
    assert!(report.contains("\"k\":2000000"), "{report}");
    // same period ledger at 100k vs 2M: level spread is 20x
    assert!(report.contains("\"ann_return\":-0.377425"), "{report}");
    assert!(report.contains("\"ann_return\":-0.018871"), "{report}");
}

#[test]
fn family_nav_base_is_time_average_disclosed() {
    let report = report_fixture_set(5);
    // mean(1.0M, 1.1M, 0.9M) = 1.0M, min/max disclosed beside it
    assert!(
        report.contains("{\"kind\":\"family-nav\",\"k\":1000000"),
        "{report}"
    );
    assert!(report.contains("\"nav_min\":900000"), "{report}");
    assert!(report.contains("\"nav_max\":1100000"), "{report}");
}

#[test]
fn vol_target_base_returns_equal_target_vol() {
    let report = report_fixture_set(6);
    // K = realized annual $-vol / 15% -> ann_vol is 0.15 by construction
    assert!(report.contains("\"kind\":\"vol-target\""), "{report}");
    assert!(report.contains("\"ann_vol\":0.15"), "{report}");
}

#[test]
fn ratios_identical_across_bases() {
    let report = report_fixture_set(4);
    // ratios printed once, base-free
    assert!(
        report.contains("\"sharpe_annualized\":-1.448286"),
        "{report}"
    );
    assert!(report.contains("\"profit_factor\":0.845771"), "{report}");
    assert!(report.contains("level is the convention"), "{report}");
}

#[test]
fn never_compounded_and_no_margin_base() {
    let report = report_fixture_set(3);
    assert!(report.contains("never CAGR"), "{report}");
    assert!(
        report.contains("margin-consumed base not offered"),
        "{report}"
    );
}

#[test]
fn fixtures_match_vector() {
    let report = report_fixture_set(7);
    let want = std::fs::read_to_string(format!(
        "{}/../../tests/vectors/capital_base.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("vector missing");
    let prefix = "{\"files\":3,\"targets\":{\"capital-base\":";
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
