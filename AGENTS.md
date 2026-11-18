# statementkit — rules

Public open-source library (Apache-2.0). Parses brokerage statement content
into actionable knowledge and numbers for upper applications. Rust core, C
ABI boundary, language bindings layered on top.

## Data

1. Never commit a real brokerage statement or any fragment of one,
   redacted or not. No real account numbers, account-holder names,
   balances, positions, prices, amounts or trading history in any file —
   tests, examples and comments included.
2. Every fixture, test vector, doc example and snippet is synthetic.
3. The library parses content handed to it: no corpus reads, no
   credentials, no API tokens (those belong to the calling application),
   no network I/O — checkable in a network namespace.
4. If a task seems to require real statement data, stop and ask. Never
   self-redact a real artifact.

## Parsing rules

1. Every parsing rule is justified by a documented characteristic of real
   statement exports and pinned by synthetic test vectors in the same
   change.
2. Classify paid vs. received by amount sign, not by row-label wording —
   labels are unreliable; use them only to sub-categorise.
3. Never sum a raw amount column across mixed currencies; per-currency
   ledgers first, conversion is a separate explicit step.

## Money and numbers (invariants)

- Money is decimal, never binary float. `parse_float` (f64) is a skeleton
  demo only; do not route money paths through it. The decimal type is a
  dependency decision — ask first.
- Numeric boundaries reject non-finite values: real exports contain the
  literal string `NaN`, and it must never enter arithmetic.
- Thousands separators, parenthesis-negatives and percent forms are later
  milestones; until implemented they must parse as Invalid, never
  silently truncate.

## Layout and bindings

- `crates/statementkit` — core. Pure functions, no I/O, no FFI types. All
  parsing and derivation logic lives here and nowhere else.
- `crates/statementkit-c` — C ABI: `#[repr(C)]` status enums, out-params,
  null checks, `catch_unwind` at every boundary so no Rust panic crosses
  into C. `include/statementkit.h` is hand-kept in sync; the discriminant
  values are contractual. cbindgen arrives when the API grows.
- `bindings/{python,node,java,go}` — thin shells over the C ABI or a
  native bridge (python: ctypes, node: raw wasm, java: Panama FFM, go:
  cgo). A binding never re-implements parsing logic; behavioral difference
  between bindings is a bug, so every suite runs the shared vectors in
  `tests/vectors/`. New Rust binding crates go in
  `crates/statementkit-<lang>`.

## Rust rules

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --
  -D warnings`, `cargo test --workspace` clean before any change lands;
  `scripts/c-smoke.sh` proves the C link path.
- No `unwrap`/`expect` on caller input in library code.
- Standard library first. Adding a third-party dependency — ask first.
- New logic ships with tests in the same change.

## Coding style

1. No clever one-liners.
2. Conciseness and clarity; short variable and function names.
3. Keep functions small; prevent creation of unnecessary functions.
4. Prefer simplicity over technically-correct abstraction.
5. Comments explain mechanism in hard-to-understand logic; no repetitive
   or article-like comments on obvious code.
6. Do not change anything unrelated to the task; keep changes as small as
   possible.
7. Prefer pure functions; keep data immutable where practical.
8. For refactoring, generally aim to reduce more code than adding.
9. Correctness over cleverness. When uncertain, ask instead of guessing.
10. Optimize only demonstrated bottlenecks.

## Public surface

- The name "statementkit" never contains "IBKR" or "Interactive Brokers".
  Every public surface carries: "Independent tool. Not affiliated with or
  endorsed by Interactive Brokers LLC."
- No advice. Never "you should".
- Public APIs are semver; keep them backward compatible unless explicitly
  instructed.

## Repo operations

- No git remote, push, tag, publish or first commit unless instructed.
- Ask before: changing license, product name or a public API; adding a
  dependency; creating a binding crate beyond the planned set.
- Commit messages: short subject, bullet-point body when more than one
  point; no prose paragraphs, no filler.
