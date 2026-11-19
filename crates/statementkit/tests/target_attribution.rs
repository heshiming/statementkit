//! `attribution` target tests.

use statementkit::Session;

fn report_csvs(csvs: &[&[u8]], chunk: usize) -> String {
    let mut s = Session::open(&["attribution"]).expect("target exists");
    for bytes in csvs {
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
fn fixtures_match_vector() {
    let report = report_csvs(
        &[&fixture("daily_sample.csv"), &fixture("monthly_sample.csv")],
        7,
    );
    let want = std::fs::read_to_string(format!(
        "{}/../../tests/vectors/attribution.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("vector missing");
    let prefix = "{\"files\":2,\"targets\":{\"attribution\":";
    let rest = report.strip_prefix(prefix).expect("report shape");
    assert_eq!(&rest[..rest.len() - 2], want.trim_end());
}

#[test]
fn buckets_and_residual_from_fixtures() {
    let report = report_csvs(
        &[&fixture("daily_sample.csv"), &fixture("monthly_sample.csv")],
        7,
    );
    // daily: income 100 + 0 - 25 + 0 = 75; fees -50; tax null; fx -25;
    // flows 0; mtm 12100; residual 12100 - (75 - 50 - 25 + 0 + 12100) = 0
    assert!(report.contains(
        "\"buckets\":{\"income\":75,\"fees\":-50,\"tax\":null,\"fx\":-25,\"flows\":0,\"mtm\":12100},\"residual\":0"
    ), "{report}");
    // monthly: income 5000+2000-100-500 = 6400; fees -3000; tax -200;
    // fx -300; flows -50000 + 0; mtm 199200; residual 0
    assert!(report.contains(
        "\"buckets\":{\"income\":6400,\"fees\":-3000,\"tax\":-200,\"fx\":-300,\"flows\":-50000,\"mtm\":199200},\"residual\":0"
    ), "{report}");
    assert_eq!(report.match_indices("\"residual\":0").count(), 2);
    assert!(!report.contains("not 0"), "no nonzero residuals expected");
}

#[test]
fn missing_bucket_is_null_and_residual_still_computed() {
    let csv = b"Statement,Data,Period,\"May 5, 2026\"\n\
                Change in Combined NAV,Header,Field,Value\n\
                Change in NAV,Data,Starting Value,100\n\
                Change in NAV,Data,Ending Value,130\n\
                Change in NAV,Data,Mark-to-Market,30\n";
    let report = report_csvs(&[csv], 3);
    assert!(report.contains("\"tax\":null"), "{report}");
    assert!(report.contains("\"residual\":0"), "{report}");
}

#[test]
fn dropped_key_produces_nonzero_residual_note() {
    let csv = b"Statement,Data,Period,\"May 5, 2026\"\n\
                Change in Combined NAV,Header,Field,Value\n\
                Change in NAV,Data,Starting Value,100\n\
                Change in NAV,Data,Ending Value,150\n\
                Change in NAV,Data,Mark-to-Market,30\n";
    let report = report_csvs(&[csv], 3);
    assert!(report.contains("\"residual\":20"), "{report}");
    assert!(
        report.contains("residual (End-Start minus summed keys) is not 0: 1"),
        "{report}"
    );
}

#[test]
fn unparsable_value_excluded_and_counted() {
    let csv = b"Statement,Data,Period,\"May 5, 2026\"\n\
                Change in Combined NAV,Header,Field,Value\n\
                Change in NAV,Data,Starting Value,100\n\
                Change in NAV,Data,Ending Value,130\n\
                Change in NAV,Data,Mark-to-Market,NaN\n";
    let report = report_csvs(&[csv], usize::MAX);
    assert!(report.contains("\"mtm\":null"), "{report}");
    assert!(
        report.contains("EAV values that failed to parse: 1"),
        "{report}"
    );
    assert!(report.contains("\"residual\":30"), "{report}");
}

#[test]
fn chunk_invariance() {
    let (d, m) = (fixture("daily_sample.csv"), fixture("monthly_sample.csv"));
    let whole = report_csvs(&[&d, &m], usize::MAX);
    for size in [1usize, 3, 4096] {
        assert_eq!(report_csvs(&[&d, &m], size), whole, "chunk {size}");
    }
}
