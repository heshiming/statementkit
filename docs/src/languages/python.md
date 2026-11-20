# Python

```sh
pip install statementkit
```

Zero-dependency ctypes shim over the C ABI. Binary wheels bundle the
native library for Linux (x86_64, aarch64) and Windows (win_amd64); on
macOS the same install builds from the sdist and needs a Rust toolchain
and Xcode command line tools on PATH.

## Scalars

```python
import statementkit
statementkit.parse_float("12.25")   # 12.25
statementkit.parse_float("1,2")     # raises InvalidInputError
statementkit.version()              # "0.2.0"
```

Errors mirror the ABI status codes: `NullInputError`, `EmptyInputError`,
`InvalidInputError`, `PanicError` — all `StatementKitError` subclasses
with a `.code`.

## Streaming session

```python
from statementkit import Session

with Session(["income", "trades"], tickers=["ABC"]) as s:
    s.feed(open("statement.csv", "rb").read())
    s.end_file()
    text = s.report()        # compact JSON, numeric literals unquoted
    data = s.report_json()   # floats as Decimal; ints stay int
```

Ticker selection is optional; without it the report carries no `filter`
fragment.

## In-repo development

`cargo build --release -p statementkit-c` puts the library in
`target/release` where the shim auto-finds it (`$STATEMENTKIT_LIB`
overrides). The suite runs the shared vectors: `python3
test_parse_float.py` against `tests/vectors/`.
