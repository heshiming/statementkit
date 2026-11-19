/* Hand-maintained mirror of crates/statementkit-c/src/lib.rs.
   Discriminant values are contractual — keep in sync.
   Nothing returned by this library is to be freed by the caller. */
#ifndef STATEMENTKIT_H
#define STATEMENTKIT_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef enum SkStatus {
    SK_OK = 0,
    SK_NULL_INPUT = 1,
    SK_EMPTY = 2,
    SK_INVALID = 3,
    SK_PANIC = 4
} SkStatus;

/* Parse a NUL-terminated float literal into *out.
   Returns SK_OK on success; input/out may not be NULL. */
SkStatus sk_parse_float(const char *input, double *out);

/* Library version, static string. Do not free. */
const char *sk_version(void);

/* Streaming session. Opaque handle; destroy only with sk_session_close.
   All entry points return a status; a Rust panic surfaces as SK_PANIC,
   never as unwinding into C. */
typedef struct SkSession SkSession;

/* Open a session over the named targets. With n_tickers > 0, the
   session applies that ticker selection and its report carries a
   "filter" fragment. On SK_OK *out is a live session; otherwise *out
   is NULL and, if err != NULL, the message is written into err (NUL,
   truncated to fit). An unknown target name is SK_INVALID. */
SkStatus sk_session_open(SkSession **out,
                const char *const *names, size_t n_names,
                const char *const *tickers, size_t n_tickers,
                char *err, size_t err_cap);

/* Feed a byte chunk of the current file; incomplete trailing lines
   carry to the next chunk. */
SkStatus sk_session_feed(SkSession *s, const unsigned char *data, size_t len);

/* Close the current file; the next feed starts the next file. */
SkStatus sk_session_end_file(SkSession *s);

/* Report, compact JSON with unquoted numeric literals. Two calls:
   report_len for the byte length, then report into a buffer of at
   least len + 1 bytes; the result is NUL-terminated. Too small a cap
   is SK_INVALID with buf untouched. */
SkStatus sk_session_report_len(const SkSession *s, size_t *out_len);
SkStatus sk_session_report(const SkSession *s, char *buf, size_t cap);

/* Destroy the session. NULL is a no-op SK_OK. */
SkStatus sk_session_close(SkSession *s);

#ifdef __cplusplus
}
#endif

#endif /* STATEMENTKIT_H */
