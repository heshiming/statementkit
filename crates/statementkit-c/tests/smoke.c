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

static const char CSV[] =
    "Statement,Header,Field,Value\n"
    "Statement,Data,Base Currency,USD\n"
    "Change in Combined NAV,Header,Field,Value\n"
    "Change in Combined NAV,Data,Ending Value,100000\n"
    "Trades,Header,Symbol,Description,Side,Quantity,Trade Price,Comm/Fee\n"
    "Trades,Data,ABC,Alfa,BUY,10,100,-5\n"
    "Trades,Data,MNEH6,Micro-B,SELL,2,90,-1\n";

static void session_checks(void) {
    const char *names[] = {"trades"};
    const char *tickers[] = {"ABC"};
    SkSession *s = NULL;
    char err[512];

    SkStatus st = sk_session_open(&s, names, 1, NULL, 0, err, sizeof err);
    if (st != SK_OK || s == NULL) {
        printf("FAIL session open: status=%d err=%s\n", st, err);
        failures++;
        return;
    }
    const unsigned char *bytes = (const unsigned char *)CSV;
    size_t half = sizeof(CSV) / 2;
    if (sk_session_feed(s, bytes, half) != SK_OK ||
        sk_session_feed(s, bytes + half, sizeof(CSV) - half) != SK_OK ||
        sk_session_end_file(s) != SK_OK) {
        puts("FAIL session feed/end_file");
        failures++;
    }
    size_t len = 0;
    if (sk_session_report_len(s, &len) != SK_OK || len == 0) {
        puts("FAIL session report_len");
        failures++;
    }
    char *buf = malloc(len + 1);
    if (buf == NULL) {
        puts("FAIL session malloc");
        failures++;
        goto done;
    }
    if (sk_session_report(s, buf, len) != SK_INVALID) {
        puts("FAIL session report small cap not rejected");
        failures++;
    }
    if (sk_session_report(s, buf, len + 1) != SK_OK || strstr(buf, "\"trades\"") == NULL ||
        strstr(buf, "\"files\"") == NULL) {
        printf("FAIL session report: %s\n", buf);
        failures++;
    }
    free(buf);
done:
    if (sk_session_close(s) != SK_OK || sk_session_close(NULL) != SK_OK) {
        puts("FAIL session close");
        failures++;
    }

    /* unknown target: NULL session, status + message */
    const char *bad[] = {"nope"};
    err[0] = '\0';
    st = sk_session_open(&s, bad, 1, NULL, 0, err, sizeof err);
    if (st != SK_INVALID || s != NULL || strstr(err, "unknown targets: nope") == NULL) {
        printf("FAIL unknown target: status=%d err=%s\n", st, err);
        failures++;
    }

    /* selection: report carries the filter fragment */
    st = sk_session_open(&s, names, 1, tickers, 1, err, sizeof err);
    if (st != SK_OK) {
        printf("FAIL selection open: %s\n", err);
        failures++;
    } else {
        sk_session_feed(s, bytes, sizeof(CSV));
        sk_session_end_file(s);
        sk_session_report_len(s, &len);
        char *fb = malloc(len + 1);
        sk_session_report(s, fb, len + 1);
        if (strstr(fb, "\"filter\"") == NULL) {
            puts("FAIL selection filter fragment missing");
            failures++;
        }
        free(fb);
        sk_session_close(s);
    }

    /* null discipline at the boundary */
    if (sk_session_feed(NULL, bytes, 4) != SK_NULL_INPUT ||
        sk_session_end_file(NULL) != SK_NULL_INPUT ||
        sk_session_report_len(NULL, NULL) != SK_NULL_INPUT) {
        puts("FAIL session null-input discipline");
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

    session_checks();

    if (strlen(sk_version()) == 0) {
        printf("c-smoke: %d failure(s)\n", failures);
        return 1;
    }
    printf("c-smoke: all assertions passed\n");
    return 0;
}
