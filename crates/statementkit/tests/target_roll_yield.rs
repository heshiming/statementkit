//! `roll-yield` target tests.

use statementkit::Session;

fn report_csvs(items: &[&[u8]], chunk: usize) -> String {
    let mut s = Session::open(&["roll-yield"]).expect("target exists");
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

// docs 05 §6 example: long 5 MES sell 4,800 buy next 4,850, mult 5
// -> (4,850-4,800) x 5 x 5 = 1,250; gap 1.0417% x 12 = 12.5%/yr.
#[test]
fn long_roll_cost_docs_example() {
    let report = report_csvs(&[&fixture("roll_m1.csv")], 7);
    assert!(report.contains("\"closing_fill\":\"MESU6\""), "{report}");
    assert!(report.contains("\"roll_cost\":1250"), "{report}");
    assert!(report.contains("\"gap_annualized\":0.125"), "{report}");
}

#[test]
fn short_roll_flips_direction() {
    let report = report_csvs(&[&fixture("roll_m1.csv")], 5);
    // short CL roll: old 70 new 69 -> (new-old) flipped -> cost +2000
    assert!(report.contains("\"closing_fill\":\"CLU6\""), "{report}");
    assert!(report.contains("\"roll_cost\":2000"), "{report}");
}

#[test]
fn multiplier_from_notional_identity() {
    let report = report_csvs(&[&fixture("roll_m1.csv")], 6);
    assert!(report.contains("\"multiplier\":5"), "{report}");
    assert!(report.contains("\"multiplier\":1000"), "{report}");
}

#[test]
fn unmatched_close_stays_out() {
    let report = report_csvs(&[&fixture("roll_m1.csv")], 4);
    // second MESU6 close of 3 has no next-month open left
    assert!(report.contains("\"unmatched_closes\":1"), "{report}");
    assert!(report.contains("\"total_roll_cost\":3250"), "{report}");
}

#[test]
fn cost_shares_of_nav() {
    let report = report_csvs(&[&fixture("roll_m1.csv")], 8);
    assert!(report.contains("\"cost_pct_of_nav\":0.01"), "{report}");
    assert!(report.contains("\"cost_pct_of_nav\":0.00625"), "{report}");
}

#[test]
fn equities_are_not_contracts() {
    let report = report_csvs(&[&fixture("roll_m1.csv")], 3);
    assert!(!report.contains("ABC"), "{report}");
    assert!(report.contains("rolls are inferred"), "{report}");
}

#[test]
fn fixtures_match_vector() {
    let report = report_csvs(&[&fixture("roll_m1.csv")], 7);
    let want = std::fs::read_to_string(format!(
        "{}/../../tests/vectors/roll_yield.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("vector missing");
    let prefix = "{\"files\":1,\"targets\":{\"roll-yield\":";
    let rest = report.strip_prefix(prefix).expect("report shape");
    assert_eq!(&rest[..rest.len() - 2], want.trim_end());
}

#[test]
fn chunk_invariance() {
    let one = fixture("roll_m1.csv");
    let whole = report_csvs(&[&one], usize::MAX);
    for size in [1usize, 31, 4096] {
        assert_eq!(report_csvs(&[&one], size), whole, "chunk {size}");
    }
}
