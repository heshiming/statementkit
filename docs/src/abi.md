# C ABI

The C ABI is the single stable boundary: every binding rides on it, so
its rules are the library's rules. Header:
`crates/statementkit-c/include/statementkit.h`, a hand-maintained
mirror of the Rust crate.

## Status codes

Every entry point returns a status. `SK_OK` (0) means success; any
other value means the out-parameter must not be read.

| Code | Value | Meaning |
|------|-------|---------|
| `SK_NULL_INPUT` | 1 | a pointer argument was NULL |
| `SK_EMPTY` | 2 | input empty after trimming |
| `SK_INVALID` | 3 | input not a finite bare number; unknown target name; out-buffer too small |
| `SK_PANIC` | 4 | a Rust panic was caught at the boundary |

The discriminant values are contractual — they are an ABI break if
changed. Bindings map these codes onto idiomatic errors (Python
exceptions, Java exceptions, Go sentinel errors).

## Ownership and nulls

- Nothing returned by the library is to be freed by the caller.
  Version strings and report buffers are library-owned.
- Input and out-params may not be NULL; a NULL is `SK_NULL_INPUT`,
  not a crash.
- The session handle is opaque; destroy it only with sk_session_close
  (a NULL argument is a no-op `SK_OK`).

## Scalar API

sk_parse_float parses a NUL-terminated float literal into a `double`.
This is the skeleton demo — binary float; money paths move to exact
decimal. sk_version returns a static version string.

## Streaming session

A session parses caller-fed bytes for a selection of named targets;
the library opens no files itself.

```text
sk_session_open ─▶ sk_session_feed* ─▶ sk_session_end_file ─▶ (more files…) ─▶ report ─▶ sk_session_close
```

- **Open.** sk_session_open takes target names, and optionally a ticker
  selection; with tickers, the report carries a `filter` fragment. On
  success `*out` is a live session; on failure `*out` is NULL and, if
  `err != NULL`, the message is written into `err` (NUL-terminated,
  truncated to fit). An unknown target name is `SK_INVALID`.
- **Feed.** sk_session_feed takes a byte chunk of the current file; an
  incomplete trailing line carries over to the next chunk.
- **End file.** sk_session_end_file closes the current file; the next feed
  starts the next file.
- **Report.** Compact JSON with unquoted numeric literals, in two
  calls: sk_session_report_len for the byte length, then sk_session_report into a
  buffer of at least `len + 1` bytes (result is NUL-terminated). Too
  small a capacity is `SK_INVALID` with the buffer untouched.
- **Destroy.** sk_session_close releases the session.

In bindings this lifecycle becomes idiomatic objects — Python
`Session` as a context manager, Java `Session` as `AutoCloseable`, Go
`s.Close()` — always with the same semantics.
