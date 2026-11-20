# Architecture

```text
            Rust core            C ABI                 bindings
        ┌───────────────┐   ┌──────────────┐   ┌─────────────────────┐
        │ statementkit  │──▶│ statementkit-c│──▶│ python   (ctypes)   │
        │ pure functions│   │ repr(C) status│   │ node     (raw WASM) │
        │ no I/O        │   │ out-params    │   │ java     (FFM)      │
        │ no FFI types  │   │ catch_unwind  │   │ go       (cgo)      │
        └───────────────┘   └──────────────┘   │ C      (header)     │
                                               └─────────────────────┘
```

## The core (`crates/statementkit`)

Pure functions only — no I/O, no FFI types, no thread state. Everything
that decides what a number or a statement row *means* lives here and
nowhere else. Streaming entry points take caller-fed byte chunks; the
crate never opens a file itself.

## The C ABI (`crates/statementkit-c`)

The single stable boundary every binding rides on:

- `#[repr(C)]` status enums as the only error channel — see
  [C ABI](abi.md).
- Out-parameters for results; nothing returned is freed by the caller.
- `catch_unwind` at every entry point, so a Rust panic surfaces as
  `SK_PANIC` and never unwinds into C.
- `include/statementkit.h` is a hand-maintained mirror of the Rust
  crate. Its discriminant values are contractual: changing one is an
  ABI break.

## The bindings (`bindings/*`)

Thin shells. A binding marshals data across the boundary and maps
status codes onto idiomatic errors for its language. It never
re-implements parsing or derivation. Behavioral difference between two
bindings is a bug, and the shared vectors in `tests/vectors/` are the
drill that catches it — see [Test vectors](vectors.md).

| Binding | Mechanism | Why |
|---------|-----------|-----|
| C       | header + static/shared lib | the baseline every other layer rides on |
| Python  | ctypes | zero dependencies; native lib bundled in wheels |
| Node    | raw WASM | zero npm deps, zero native toolchain for users |
| Java    | Panama FFM (JDK 22+) | no JNI, no codegen; natives embedded in the jar |
| Go      | cgo over the static lib | plain `go test ./...` from the repo |

## Test layout

Mock statement CSVs and shared vectors live only in the root `tests/`
tree. Bindings never hold copies; binding tests reference the root
files by relative path. A fixture copy inside a binding is a bug.
