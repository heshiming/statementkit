/* Hand-maintained mirror of crates/statementkit-c/src/lib.rs.
   Discriminant values are contractual — keep in sync.
   Nothing returned by this library is to be freed by the caller. */
#ifndef STATEMENTKIT_H
#define STATEMENTKIT_H

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

#ifdef __cplusplus
}
#endif

#endif /* STATEMENTKIT_H */
