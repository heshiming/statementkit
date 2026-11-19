//! `trades` target tests.

use statementkit::Session;

fn report_csvs(items: &[&[u8]], chunk: usize) -> String {
    let mut s = Session::open(&["trades"]).expect("target exists");
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
fn fifo_holds_percentiles_and_censoring() {
    let report = report_csvs(&[&fixture("trades_m1.csv")], 7);
    assert!(report.contains("\"blocks\":4"), "{report}");
    assert!(report.contains("\"transfers\":1"), "{report}");
    assert!(report.contains("\"marker_closes\":1"), "{report}");
    assert!(report.contains("\"closes_matched\":4"), "{report}");
    assert!(report.contains("\"closes_censored\":1"), "{report}");
    // holds: EUR 30m, SPY 4h, ES 15h05, expiry 11d6h40
    assert!(
        report.contains(
            "\"p25\":1800,\"p50\":14400,\"p75\":54300,\"p90\":974400,\"same_day_share\":0.5"
        ),
        "{report}"
    );
    assert!(report.contains("left-censored closes: 1 of 5"), "{report}");
}

#[test]
fn block_variants_keep_forex_and_futures_amounts() {
    let report = report_csvs(&[&fixture("trades_m1.csv")], 5);
    // ES close matched from the Notional Value block; EUR close from the
    // Comm in USD block: both must be in the matched count
    assert!(report.contains("\"closes_matched\":4"), "{report}");
    // forex carries no Realized P/L: only the four realized closes count
    assert!(
        report.contains("\"close_rows\":4,\"total\":-1185"),
        "{report}"
    );
    assert!(
        report.contains("\"identity_ok\":4,\"identity_bad\":0"),
        "{report}"
    );
}

#[test]
fn open_row_pnl_artifact_counted_and_excluded() {
    let report = report_csvs(&[&fixture("trades_m1.csv")], 9);
    assert!(report.contains("\"open_rows_with_pnl\":1"), "{report}");
    assert!(
        report.contains("artifact: 1 (total 12, excluded)"),
        "{report}"
    );
    // total -1185 = 275+40+500-2000, the +12 artifact is not in it
    assert!(report.contains("\"total\":-1185"), "{report}");
}

#[test]
fn expiry_marker_kept_in_holds_excluded_from_hours() {
    let report = report_csvs(&[&fixture("trades_m1.csv")], 4096);
    assert!(report.contains("\"marker_closes\":1"), "{report}");
    // no hour-16 bucket at all: the 16:20 expiry print is excluded
    assert!(!report.contains("\"hour\":16"), "{report}");
}

#[test]
fn duplicates_across_overlapping_files_drop_to_one_leg() {
    let one = fixture("trades_m1.csv");
    let twice = report_csvs(&[&one, &one], 13);
    assert!(twice.contains("\"fills\":12"), "{twice}");
    assert!(
        twice.contains("duplicate fills dropped (overlapping files): 12"),
        "{twice}"
    );
    let once = report_csvs(&[&one], 13);
    let frag = |s: &str| {
        let a = s.find("\"targets\":").expect("targets");
        s[a..].to_string()
    };
    assert_eq!(
        frag(&twice)
            .replace("\"blocks\":8", "\"blocks\":4",)
            .replace(",\"duplicate fills dropped (overlapping files): 12\"]", "]",),
        frag(&once)
    );
}

#[test]
fn fifo_lots_span_across_files() {
    let open =
        b"Trades,Header,Symbol,Date/Time,Quantity,Proceeds,Comm/Fee,Basis,Realized P/L,Code\n\
                 Trades,Data,XYZ,\"2026-10-30, 10:00:00\",10,-1000,-1,1000,0,O\n";
    let close =
        b"Trades,Header,Symbol,Date/Time,Quantity,Proceeds,Comm/Fee,Basis,Realized P/L,Code\n\
                  Trades,Data,XYZ,\"2026-11-02, 10:00:00\",-10,1100,-1,-1000,99,C\n";
    let report = report_csvs(&[open, close], 6);
    // hold = 3 days exactly
    assert!(report.contains("\"p25\":259200"), "{report}");
    assert!(report.contains("\"total\":99"), "{report}");
    assert!(report.contains("\"identity_ok\":1"), "{report}");
}

#[test]
fn fixtures_match_vector() {
    let report = report_csvs(&[&fixture("trades_m1.csv")], 7);
    let want = std::fs::read_to_string(format!(
        "{}/../../tests/vectors/trades.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("vector missing");
    let prefix = "{\"files\":1,\"targets\":{\"trades\":";
    let rest = report.strip_prefix(prefix).expect("report shape");
    assert_eq!(&rest[..rest.len() - 2], want.trim_end());
}

#[test]
fn chunk_invariance() {
    let one = fixture("trades_m1.csv");
    let whole = report_csvs(&[&one], usize::MAX);
    for size in [1usize, 31, 4096] {
        assert_eq!(report_csvs(&[&one], size), whole, "chunk {size}");
    }
}
