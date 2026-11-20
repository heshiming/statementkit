# statementkit

Parse brokerage statement content into actionable knowledge and numbers.
A library for upper applications: a pure Rust core, a stable C ABI, and
thin bindings for other languages on top.

All parsing and derivation logic lives in the Rust core and nowhere else.
A binding that re-implements logic, or behaves differently from another
binding, is a bug — every binding runs the same shared test vectors.

## At a glance

- Version 0.2.0: numeric parsing plus a streaming session over named
  statement targets, with optional ticker selection.
- Published: crates.io (Rust), PyPI (Python), npm (Node, raw WASM),
  Maven Central (Java, natives embedded in the jar). Go via cgo source.
- No network I/O, no credentials, no telemetry — checkable in a network
  namespace.
- Every fixture and example is synthetic. The library has never seen a
  real statement and ships none.

## Layout

```text
crates/statementkit      core: pure functions, no I/O, no FFI types
crates/statementkit-c    C ABI: status codes, out-params, panic-proofed
  include/statementkit.h hand-kept header; discriminants contractual
bindings/python          ctypes shim over the C ABI
bindings/node            raw-WASM npm package, TypeScript types included
bindings/java            Panama FFM, JDK 22+, no JNI
bindings/go              cgo over the static lib
tests/vectors/           shared ground-truth vectors every binding runs
tests/fixtures/          synthetic mock statement CSVs
```

## Status

The API is `0.x`: expect additive changes. The C ABI discriminant
values and the status-code contract are stable; see [C ABI](abi.md).
Numeric handling is moving to an exact decimal type — see
[Guarantees](guarantees.md).

- Start with [Architecture](architecture.md) to see where logic lives.
- Pick your language under **Language guides**.
- [API references](references.md) links the per-ecosystem reference
  docs; this site covers concepts and guides, not signatures.
