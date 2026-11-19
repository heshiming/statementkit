//! Decimal cell-form vectors: tests/vectors/dec.tsv, consumed here and
//! by every binding suite. Columns: input, kind (ok|empty|invalid),
//! expected exact text for ok.

use statementkit::{Dec, DecError};

fn vector_path() -> String {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/vectors/dec.tsv").to_string()
}

#[test]
fn dec_vectors() {
    let data = std::fs::read_to_string(vector_path()).expect("vector file missing");
    for (i, line) in data.split('\n').enumerate() {
        if line.is_empty() {
            continue;
        }
        let mut cols = line.split('\t');
        let (input, kind) = (cols.next().unwrap(), cols.next().unwrap());
        let want = cols.next().unwrap_or("");
        let got = Dec::parse(input);
        match kind {
            "ok" => assert_eq!(
                got.map(|d| d.to_text()),
                Ok(want.to_string()),
                "line {}",
                i + 1
            ),
            "empty" => assert_eq!(got.err(), Some(DecError::Empty), "line {}", i + 1),
            "invalid" => assert_eq!(got.err(), Some(DecError::Invalid), "line {}", i + 1),
            other => panic!("bad vector kind {other:?} on line {}", i + 1),
        }
    }
}

#[test]
fn arithmetic_is_exact() {
    let a = Dec::parse("0.1").unwrap();
    let mut sum = Dec::ZERO;
    for _ in 0..10 {
        sum = sum + a;
    }
    assert_eq!(sum.to_text(), "1");
    let big = Dec::parse("9,223,372,036.85477580").unwrap();
    assert_eq!(big.to_scaled(), 922_337_203_685_477_580);
}
