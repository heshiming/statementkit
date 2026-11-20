# Guarantees

These hold in every binding, because they are enforced once in the core
and at the C ABI boundary:

## No network, no credentials

The library parses content handed to it. No network I/O, no API
tokens, no telemetry. This is checkable: run a consumer in a network
namespace and it keeps working. Credentials belong to the calling
application.

## No panic ever crosses the FFI boundary

Every C ABI entry point runs under `catch_unwind`. A Rust panic
surfaces as the `SK_PANIC` status code — a last resort, never an
unwind into C, never a crash of the host process.

## Synthetic data only

Every fixture, test vector, doc example and snippet is synthetic. The
library has never seen a real brokerage statement and ships none.

## Money is decimal, never binary float

`parse_float` (f64) is a skeleton demo — do not route money through
it. Exact decimal handling (`Dec`) is the money path. Numeric
boundaries reject non-finite values: real statement exports contain the
literal string `NaN`, and it must never enter arithmetic.

Thousands separators, parenthesis-negatives and percent forms are later
milestones; until implemented they parse as Invalid, never silently
truncated.

## Behavior is pinned by shared vectors

All bindings run the same ground-truth vectors from `tests/vectors/`.
If Python and Go disagree on an input, that is a bug with a failing
test, not a documentation question. See [Test vectors](vectors.md).
