/* End-to-end C consumer of the statementkit static library. */
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "statementkit.h"

static int failures = 0;

static void expect_ok(const char *in, double want) {
    double got = NAN;
    SkStatus st = sk_parse_float(in, &got);
    if (st != SK_OK || got != want) {
        printf("FAIL sk_parse_float(%s): status=%d got=%g want=%g\n", in, st, got, want);
        failures++;
    }
}

static void expect_status(const char *in, SkStatus want) {
    double got = NAN;
    SkStatus st = sk_parse_float(in, &got);
    if (st != want) {
        printf("FAIL sk_parse_float(%s): status=%d want=%d\n", in, st, want);
        failures++;
    }
}

int main(void) {
    expect_ok("12.25", 12.25);
    expect_ok("-0.5", -0.5);
    expect_ok("1e3", 1000.0);
    expect_ok("  42 ", 42.0);
    expect_status("", SK_EMPTY);
    expect_status("   ", SK_EMPTY);
    expect_status("1,234.56", SK_INVALID);
    expect_status("NaN", SK_INVALID);
    expect_status(NULL, SK_NULL_INPUT);

    if (strlen(sk_version()) == 0) {
        printf("FAIL sk_version empty\n");
        failures++;
    }
    if (failures) {
        printf("c-smoke: %d failure(s)\n", failures);
        return 1;
    }
    printf("c-smoke: all assertions passed\n");
    return 0;
}
