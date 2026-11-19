//! `fx-exposure` target tests.

use statementkit::Session;

fn report_csvs(items: &[&[u8]], chunk: usize) -> String {
    let mut s = Session::open(&["fx-exposure"]).expect("target exists");
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

// docs 05 §8 synthetic example: NAV 1,000,000; GBP +120,000,
// JPY -45,000, HKD-equity +90,000 -> net +165,000 = 16.5%,
// gross 255,000 = 25.5%.
#[test]
fn net_and_gross_over_nav() {
    let report = report_csvs(&[&fixture("fx_exposure_m1.csv")], 7);
    assert!(
        report.contains(
            "{\"month\":\"2026-03\",\"days\":1,\"net_over_nav_avg\":0.165,\
             \"gross_over_nav_avg\":0.255,\"gross_over_nav_max\":0.255"
        ),
        "{report}"
    );
}

#[test]
fn signed_short_shows_in_net_not_gross_only() {
    let report = report_csvs(&[&fixture("fx_exposure_m1.csv")], 5);
    assert!(
        report.contains("{\"currency\":\"JPY\",\"net_over_nav_avg\":-0.045}"),
        "{report}"
    );
}

#[test]
fn base_currency_never_counts() {
    let report = report_csvs(&[&fixture("fx_exposure_m1.csv")], 9);
    assert!(!report.contains("\"USD\""), "{report}");
    assert!(report.contains("\"other_fx_translations\":250"), "{report}");
}

#[test]
fn missing_same_file_rate_counts_out_not_interpolated() {
    let no_rate = b"Statement,Data,Currency,USD\n\
                    Statement,Data,Period,\"March 31, 2026 - March 31, 2026\"\n\
                    Change in Combined NAV,Header,Section,Key,Value,Basis,Currency\n\
                    Change in Combined NAV,Data,Net Asset Value,Ending Value,1000000,,USD\n\
                    Open Positions,Header,Symbol,Quantity,Close Price,Cost Price,Mult,Value,Cost Basis,Unrealized P/L,Asset Category,Currency,DataDiscriminator\n\
                    Open Positions,Data,CHF,100,500.00,400.00,1,50000,-40000,10000,Stock,CHF,Summary\n";
    let report = report_csvs(&[no_rate], 6);
    assert!(report.contains("\"snapshots\":1"), "{report}");
    assert!(report.contains("missing same-file rate: 1"), "{report}");
    assert!(!report.contains("CHF"), "{report}");
}

#[test]
fn fixtures_match_vector() {
    let report = report_csvs(&[&fixture("fx_exposure_m1.csv")], 7);
    let want = std::fs::read_to_string(format!(
        "{}/../../tests/vectors/fx_exposure.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("vector missing");
    let prefix = "{\"files\":1,\"targets\":{\"fx-exposure\":";
    let rest = report.strip_prefix(prefix).expect("report shape");
    assert_eq!(&rest[..rest.len() - 2], want.trim_end());
}

#[test]
fn chunk_invariance() {
    let one = fixture("fx_exposure_m1.csv");
    let whole = report_csvs(&[&one], usize::MAX);
    for size in [1usize, 31, 4096] {
        assert_eq!(report_csvs(&[&one], size), whole, "chunk {size}");
    }
}
