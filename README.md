# statementkit

Parse brokerage statement content into actionable knowledge and numbers.
A library for upper applications: a pure Rust core, a stable C ABI, and
thin bindings for other languages on top.

Independent tool. Not affiliated with or endorsed by Interactive Brokers
LLC.

## Status

Skeleton. The demo API is a single float-literal parser proving every
layer: Rust core → Rust tests → C ABI → header → linked C consumer.
Semantics follow the documented characteristics of real statement
exports; expect additive APIs at `0.x`.

## Layout

```
crates/statementkit      core: pure functions, no I/O, no FFI types
crates/statementkit-c    C ABI: status codes, out-params, panic-proofed
  include/statementkit.h hand-kept header; discriminants contractual
bindings/python          reserved: pyo3+maturin crate (ctypes interim)
bindings/node            reserved: wasm-bindgen / napi, generated .d.ts
bindings/java            reserved: Panama FFM (java.lang.foreign), JNI fallback
scripts/c-smoke.sh       links tests/smoke.c against the static lib, runs it
```

All parsing logic lives in the core crate only. A binding that
re-implements logic, or behaves differently from another binding, is a bug.

## Quickstart

```sh
cargo test --workspace        # core unit + integration + FFI tests
scripts/c-smoke.sh            # builds release static lib, compiles and runs C smoke
```

C usage:

```c
#include "statementkit.h"
double v;
if (sk_parse_float("12.25", &v) == SK_OK) { /* v == 12.25 */ }
```

## Guarantees

- No network I/O, no credentials, no telemetry — checkable in a network
  namespace.
- No Rust panic ever crosses the FFI boundary (`catch_unwind` at every
  entry point; `SK_PANIC` as the last-resort status).
- All test fixtures are synthetic. The library has never seen a real
  statement and ships none.
- Binary float is for demo only; money handling will be exact decimal.

## License

Apache-2.0. See [LICENSE](LICENSE).
