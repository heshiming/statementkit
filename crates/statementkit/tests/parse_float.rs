use statementkit::{Error, parse_float};

// Ground truth for the CSV extraction path, synthetic vectors only.

#[test]
fn float_vectors() {
    let ok: &[(&str, f64)] = &[
        ("12.25", 12.25),
        ("-1040.02", -1040.02),
        ("0.000001", 0.000001),
        ("176.4", 176.4),
    ];
    for (text, want) in ok {
        assert_eq!(parse_float(text), Ok(*want), "input: {text:?}");
    }

    let empty: &[&str] = &["", " \t "];
    for text in empty {
        assert_eq!(parse_float(text), Err(Error::Empty), "input: {text:?}");
    }

    let invalid: &[&str] = &["n/a", "1,234", "1..2", "NaN", "inf", "3.5x"];
    for text in invalid {
        assert_eq!(parse_float(text), Err(Error::Invalid), "input: {text:?}");
    }
}
