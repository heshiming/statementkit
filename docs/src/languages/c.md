# C

Baseline consumer of the C ABI. Header:
`crates/statementkit-c/include/statementkit.h`.

## Build the library

```sh
cargo build --release -p statementkit-c
# -> target/release/libstatementkit_c.a and .so
```

## Use it

```c
#include "statementkit.h"
#include <stdio.h>

double v;
if (sk_parse_float("12.25", &v) == SK_OK) { /* v == 12.25 */ }

const char *ver = sk_version(void);   /* static string; do not free */
```

(Link flags as in `scripts/c-smoke.sh`:
`-I crates/statementkit-c/include target/release/libstatementkit_c.a
-lm -ldl -lpthread`.)

A streaming session:

```c
SkSession *s = NULL;
char err[256];
const char *names[] = { "income", "trades" };
const char *tickers[] = { "ABC" };
if (sk_session_open(&s, names, 2, tickers, 1, err, sizeof err) != SK_OK) {
    fprintf(stderr, "%s\n", err);   /* unknown target: SK_INVALID */
    return 1;
}
sk_session_feed(s, data, len);          /* feed byte chunks of one file */
sk_session_end_file(s);            /* next feed starts the next file */

size_t n;
sk_session_report_len(s, &n);
char *buf = malloc(n + 1);
sk_session_report(s, buf, n + 1);         /* compact JSON, NUL-terminated */
free(buf);                        /* the buffer is yours; the library
                                     owns nothing you keep here */
sk_session_close(s);
```

Rules that matter in C — full list in [C ABI](../abi.md): nothing the
library returns is to be freed by the caller, out-params are valid only
on `SK_OK`, and a Rust panic arrives as `SK_PANIC`, never as an unwind.

## Reference consumer

`scripts/c-smoke.sh` links `crates/statementkit-c/tests/smoke.c`
against the static library, runs it, and is part of CI. It is the
minimal known-good link line.
