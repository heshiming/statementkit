//! `leverage` target tests.

use statementkit::Session;

fn report_csvs(items: &[&[u8]], chunk: usize) -> String {
    let mut s = Session::open(&["leverage"]).expect("target exists");
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
fn gross_net_check_with_cash_legs() {
    let report = report_csvs(&[&fixture("lev_m1.csv")], 7);
    assert!(report.contains("\"position_sum\":720500"), "{report}");
    assert!(report.contains("\"position_abs\":840500"), "{report}");
    assert!(report.contains("\"cash_total\":279500"), "{report}");
    assert!(report.contains("\"gross_over_nav\":1.12"), "{report}");
    assert!(report.contains("\"net_over_nav\":1"), "{report}");
    assert!(
        report.contains("\"check_pos_plus_cash_minus_nav\":0"),
        "{report}"
    );
}

#[test]
fn eur_position_converts_at_file_rate() {
    let report = report_csvs(&[&fixture("lev_m1.csv")], 5);
    // 5000 EUR at 1.10 = 5500 USD inside sum/abs
    assert!(report.contains("\"position_sum\":720500"), "{report}");
}

#[test]
fn cash_leg_choice_is_labeled() {
    let report = report_csvs(&[&fixture("lev_m1.csv")], 9);
    assert!(report.contains("\"cash_in_numerator\":true"), "{report}");
    assert!(report.contains("numerator choice labeled"), "{report}");
}

#[test]
fn missing_cash_block_counts_with_zero_leg() {
    let report = report_csvs(&[&fixture("weight_d1.csv")], 6);
    assert!(report.contains("\"cash_total\":0"), "{report}");
    assert!(
        report.contains("no cash blocks (cash leg = 0): 1"),
        "{report}"
    );
}

#[test]
fn fixtures_match_vector() {
    let report = report_csvs(&[&fixture("lev_m1.csv"), &fixture("weight_d1.csv")], 7);
    let want = std::fs::read_to_string(format!(
        "{}/../../tests/vectors/leverage.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("vector missing");
    let prefix = "{\"files\":2,\"targets\":{\"leverage\":";
    let rest = report.strip_prefix(prefix).expect("report shape");
    assert_eq!(&rest[..rest.len() - 2], want.trim_end());
}

#[test]
fn chunk_invariance() {
    let items = [fixture("lev_m1.csv"), fixture("weight_d1.csv")];
    let refs: Vec<&[u8]> = items.iter().map(|v| v.as_slice()).collect();
    let whole = report_csvs(&refs, usize::MAX);
    for size in [1usize, 31, 4096] {
        assert_eq!(report_csvs(&refs, size), whole, "chunk {size}");
    }
}
