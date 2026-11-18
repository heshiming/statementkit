# Python binding (reserved)

Plan, in order of arrival:

1. Interim, zero build tooling: `ctypes` over the shared library produced
   by `cargo build -p statementkit-c` (`libstatementkit_c.so` / `.dylib` /
   `.dll`), mirroring `include/statementkit.h`.
2. Real package: `crates/statementkit-python` (pyo3 + maturin), wheels on
   PyPI. Exceptions map from `SkStatus`.

Rules: thin shell only — no parsing logic here; every core test vector is
re-run through the binding so behavior cannot drift.
