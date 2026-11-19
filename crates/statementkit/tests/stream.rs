//! Stream processor tests: chunk invariance, layout quirks, counters.

use statementkit::session::{FileSummary, Session};

fn parse(data: &[u8], chunk: usize) -> Vec<FileSummary> {
    let mut s = Session::new();
    for part in data.chunks(chunk.max(1)) {
        s.feed(part);
    }
    s.end_file();
    s.files().to_vec()
}

fn fixture() -> Vec<u8> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/activity_sample.csv"
    );
    std::fs::read(path).expect("fixture missing")
}

fn titles(f: &FileSummary) -> Vec<&str> {
    f.sections.iter().map(|s| s.title.as_str()).collect()
}

#[test]
fn fixture_summary() {
    let files = parse(&fixture(), usize::MAX);
    assert_eq!(files.len(), 1);
    let f = &files[0];
    assert_eq!(
        titles(f),
        [
            "Statement",
            "Trades",
            "Change in Combined NAV",
            "Interest",
            "Open Positions",
        ]
    );
    assert_eq!(f.meta.len(), 4);
    assert_eq!(
        f.meta.iter().find(|e| e.0 == "Period").unwrap().1,
        "January 1, 2024 - December 31, 2024"
    );
    let trades = &f.sections[1];
    assert_eq!(trades.fields.len(), 10);
    assert_eq!(trades.fields[9], "");
    assert_eq!(trades.rows, 3);
    assert_eq!(f.sections[2].rows, 2);
    assert_eq!(f.sections[3].rows, 4);
    assert_eq!(f.data_rows, 10);
    assert_eq!(
        (f.short_rows, f.orphan_rows, f.other_rows, f.bad_rows),
        (0, 0, 0, 0)
    );
}

#[test]
fn chunk_invariance() {
    let data = fixture();
    let want = parse(&data, usize::MAX);
    assert!(!want.is_empty());
    for size in [1usize, 7, 4096] {
        assert_eq!(parse(&data, size), want, "chunk size {size}");
    }
}

fn crlf(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 16);
    for &b in data {
        if b == b'\n' {
            out.push(b'\r');
        }
        out.push(b);
    }
    out
}

#[test]
fn bom_and_crlf_are_transparent() {
    let data = fixture();
    let want = parse(&data, usize::MAX);
    let mut bom = b"\xEF\xBB\xBF".to_vec();
    bom.extend_from_slice(&data);
    assert_eq!(parse(&bom, usize::MAX), want);
    assert_eq!(parse(&bom, 2), want, "BOM split across chunks");
    let win = crlf(&data);
    assert_eq!(parse(&win, usize::MAX), want);
    assert_eq!(parse(&win, 1), want, "CRLF split across chunks");
}

#[test]
fn newline_inside_quotes_stays_in_record() {
    let csv = b"X,Header,A,B\nX,Data,\"line1\nline2\",2\n";
    let files = parse(csv, usize::MAX);
    let f = &files[0];
    assert_eq!(f.sections[0].rows, 1);
    assert_eq!(f.data_rows, 1);
    assert_eq!(f.bad_rows, 0);
}

#[test]
fn doubled_quotes_are_literal() {
    let csv = b"X,Header,A\nX,Data,\"say \"\"hi\"\"\"\n";
    let files = parse(csv, 1);
    assert_eq!(files[0].data_rows, 1);
    assert_eq!(files[0].bad_rows, 0);
}

#[test]
fn alias_rows_retag_to_their_header() {
    let csv = b"Change in Combined NAV,Header,Field,Value\nChange in NAV,Data,Total,1\n";
    let files = parse(csv, usize::MAX);
    assert_eq!(titles(&files[0]), ["Change in Combined NAV"]);
    assert_eq!(files[0].sections[0].rows, 1);
}

#[test]
fn alias_rows_before_any_header_open_the_section() {
    let csv = b"Change in NAV,Data,Field,Value\nChange in NAV,Data,Total,1\n";
    let files = parse(csv, usize::MAX);
    assert_eq!(titles(&files[0]), ["Change in Combined NAV"]);
    assert_eq!(files[0].sections[0].fields, ["Field", "Value"]);
    assert_eq!(files[0].sections[0].rows, 1);
}

#[test]
fn data_row_without_section_is_orphan() {
    let csv = b"Foo,Data,a,b\n";
    let files = parse(csv, 3);
    assert_eq!(files[0].orphan_rows, 1);
    assert_eq!(files[0].data_rows, 0);
}

#[test]
fn short_rows_and_foreign_row_types_are_counted() {
    let csv = b"junk\nA,Header,X\nA,Bogus,v\n";
    let files = parse(csv, usize::MAX);
    assert_eq!(files[0].short_rows, 1);
    assert_eq!(files[0].other_rows, 1);
}

#[test]
fn unterminated_quote_is_dropped_and_counted() {
    let csv = b"A,Header,X\nA,Data,\"oops\n";
    let files = parse(csv, usize::MAX);
    assert_eq!(files[0].bad_rows, 1);
    assert_eq!(files[0].data_rows, 0);
}

#[test]
fn invalid_utf8_record_is_dropped_and_counted() {
    let mut csv = b"A,Header,X\nA,Data,".to_vec();
    csv.extend_from_slice(&[0xFF, 0xFE]);
    csv.extend_from_slice(b"\nA,Data,ok\n");
    let files = parse(&csv, 1);
    assert_eq!(files[0].bad_rows, 1);
    assert_eq!(files[0].data_rows, 1);
}

#[test]
fn final_row_without_trailing_newline_is_kept() {
    let data = fixture();
    let mut trimmed = data.clone();
    while trimmed.ends_with(b"\n") {
        trimmed.pop();
    }
    assert_eq!(parse(&trimmed, 5), parse(&data, usize::MAX));
}

#[test]
fn multiple_files_keep_separate_summaries() {
    let data = fixture();
    let mut win = crlf(&data);
    win.splice(0..0, b"\xEF\xBB\xBF".iter().cloned());
    let mut s = Session::new();
    s.feed(&data);
    s.end_file();
    s.feed(&win);
    s.end_file();
    assert_eq!(s.files().len(), 2);
    assert_eq!(s.files()[0], s.files()[1]);
}
