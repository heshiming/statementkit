# Python binding — implemented (ctypes)

`statementkit` is a zero-dependency ctypes shim over the C ABI. Published
on PyPI; `pip install statementkit`. Binary wheels ship the native library
bundled for Linux (x86_64, aarch64) and Windows (win_amd64); on macOS the
same install builds the library from the sdist, which needs a Rust
toolchain (`cargo`) and the Xcode command line tools on PATH. For in-repo
development: `cargo build --release -p statementkit-c` (auto-found in
`target/release`, override with `$STATEMENTKIT_LIB`).

Independent tool. Not affiliated with or endorsed by Interactive Brokers
LLC.

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

Packaging: the Python distribution is declared in the repo-root
`pyproject.toml`; setuptools-rust (`NoBinding`) compiles
`crates/statementkit-c` and bundles the cdylib into the wheel. Build with
`python -m build` from the repo root.

Next stage when the API is real: `crates/statementkit-python` (pyo3 +
maturin, `.pyi` stubs); this shim keeps working meanwhile.
