// Package statementkit is a cgo binding over statementkit's C ABI.
//
// Build: cargo build --release -p statementkit-c (the static library is
// linked from ../../target/release via ${SRCDIR}-relative flags).
package statementkit

/*
#cgo CFLAGS: -I${SRCDIR}/../../crates/statementkit-c/include
#cgo LDFLAGS: ${SRCDIR}/../../target/release/libstatementkit_c.a -lm -ldl -lpthread
#include "statementkit.h"
#include <stdlib.h>
*/
import "C"

import (
	"errors"
	"unsafe"
)

var (
	ErrEmpty     = errors.New("statementkit: empty input")
	ErrInvalid   = errors.New("statementkit: invalid number")
	ErrPanic     = errors.New("statementkit: panic caught at the FFI boundary")
	ErrNullInput = errors.New("statementkit: null input")
)

// ParseFloat parses a bare float literal. Go strings cannot be null, so
// ErrNullInput is unreachable from Go.
func ParseFloat(s string) (float64, error) {
	cs := C.CString(s)
	defer C.free(unsafe.Pointer(cs))
	var out C.double
	switch C.sk_parse_float(cs, &out) {
	case C.SK_OK:
		return float64(out), nil
	case C.SK_EMPTY:
		return 0, ErrEmpty
	case C.SK_INVALID:
		return 0, ErrInvalid
	case C.SK_NULL_INPUT:
		return 0, ErrNullInput
	default:
		return 0, ErrPanic
	}
}

// Version is the library version, e.g. "0.1.0".
func Version() string {
	return C.GoString(C.sk_version())
}
