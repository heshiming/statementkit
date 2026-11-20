# Rust

```sh
cargo add statementkit
```

Full API reference: [docs.rs/statementkit](https://docs.rs/statementkit).

## Scalars

```rust
use statementkit::{parse_float, Dec};

let v = parse_float("12.25")?;          // 12.25f64 — demo only
assert_eq!(parse_float("NaN"), Err(_)); // non-finite never passes

let d = Dec::parse("12.25")?;           // money path: exact decimal
assert_eq!(d.to_text(), "12.25");
```

`parse_float` returns binary float and is a skeleton demo — do not
route money through it. `Dec` is the decimal type.

## Streaming session

```rust
use statementkit::{Session, target_names};

println!("{:?}", target_names());       // available statement targets

let mut s = Session::open_selected(&["income", "trades"], &["ABC"])?;
s.feed(chunk);                          // caller-fed bytes; no file I/O
s.end_file();                           // next feed is the next file

let text: String = s.report_text();     // compact JSON, unquoted numbers
let value = s.report();                 // structured Value
```

`Session::open(&names)` skips the ticker selection; the selected form
carries a `filter` fragment in the report. Per-file summaries are
available via `s.files()`.

## Guarantees

The crate is pure: no I/O, no FFI types, no network. See
[Guarantees](../guarantees.md).
