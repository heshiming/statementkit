# Python binding — implemented (ctypes)

`statementkit.py` is a zero-dependency ctypes shim over the C ABI. Requires
the built library: `cargo build --release -p statementkit-c` (auto-found in
`target/release`, override with `$STATEMENTKIT_LIB`).

```python
import statementkit
statementkit.parse_float("12.25")   # 12.25
statementkit.parse_float("1,2")     # raises InvalidInputError
statementkit.version()              # "0.1.0"
```

Errors mirror the ABI status codes: `NullInputError`, `EmptyInputError`,
`InvalidInputError`, `PanicError` (all `StatementKitError` subclasses with
`.code`).

Test: `python3 test_parse_float.py` (shared vectors from `tests/vectors/`).

Next stage when the API is real: `crates/statementkit-python` (pyo3 +
maturin, wheels, `.pyi` stubs); this shim keeps working meanwhile.
