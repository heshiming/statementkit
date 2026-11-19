package statementkit

/*
#cgo CFLAGS: -I${SRCDIR}/../../crates/statementkit-c/include
#include "statementkit.h"
#include <stdlib.h>
*/
import "C"

import (
	"fmt"
	"runtime"
	"unsafe"
)

// Session is a push streaming session over named targets. Feed the
// bytes of one file (in any chunk sizes), call EndFile, repeat; Report
// returns the compact JSON report text. Close frees the native
// session; a finalizer covers forgotten sessions.
type Session struct {
	p *C.SkSession
}

// NewSession opens targets with an optional ticker selection. An
// unknown target name returns an error carrying the ABI message.
func NewSession(targets, tickers []string) (*Session, error) {
	ct := make([]*C.char, len(targets))
	for i, t := range targets {
		ct[i] = C.CString(t)
	}
	defer freeAll(ct)
	cx := make([]*C.char, len(tickers))
	for i, t := range tickers {
		cx[i] = C.CString(t)
	}
	defer freeAll(cx)

	var namesPtr **C.char
	if len(ct) > 0 {
		namesPtr = &ct[0]
	}
	var tickPtr **C.char
	if len(cx) > 0 {
		tickPtr = &cx[0]
	}
	var errBuf [512]C.char
	var p *C.SkSession
	st := C.sk_session_open(&p, namesPtr, C.size_t(len(ct)),
		tickPtr, C.size_t(len(cx)), &errBuf[0], C.size_t(len(errBuf)))
	runtime.KeepAlive(ct)
	runtime.KeepAlive(cx)
	if st != C.SK_OK {
		return nil, fmt.Errorf("statementkit: %s", C.GoString(&errBuf[0]))
	}
	s := &Session{p: p}
	runtime.SetFinalizer(s, (*Session).Close)
	return s, nil
}

// Feed bytes of the current file; incomplete trailing lines carry over.
func (s *Session) Feed(data []byte) error {
	if len(data) == 0 {
		return nil
	}
	st := C.sk_session_feed(s.p, (*C.uchar)(unsafe.Pointer(unsafe.SliceData(data))),
		C.size_t(len(data)))
	return statusError(st)
}

// EndFile closes the current file; the next Feed starts the next one.
func (s *Session) EndFile() error {
	return statusError(C.sk_session_end_file(s.p))
}

// Report returns the report as compact JSON text with unquoted numeric
// literals. Parse with json.Number to keep numeric text exact.
func (s *Session) Report() (string, error) {
	var n C.size_t
	if err := statusError(C.sk_session_report_len(s.p, &n)); err != nil {
		return "", err
	}
	buf := make([]C.char, int(n)+1)
	if err := statusError(C.sk_session_report(s.p, &buf[0], C.size_t(int(n)+1))); err != nil {
		return "", err
	}
	return C.GoString(&buf[0]), nil
}

// Close destroys the native session; safe to call twice.
func (s *Session) Close() error {
	if s.p == nil {
		return nil
	}
	runtime.SetFinalizer(s, nil)
	st := statusError(C.sk_session_close(s.p))
	s.p = nil
	return st
}

func freeAll(cs []*C.char) {
	for _, c := range cs {
		C.free(unsafe.Pointer(c))
	}
}

func statusError(st C.SkStatus) error {
	switch st {
	case C.SK_OK:
		return nil
	case C.SK_NULL_INPUT:
		return ErrNullInput
	case C.SK_INVALID:
		return ErrInvalid
	case C.SK_EMPTY:
		return ErrEmpty
	default:
		return ErrPanic
	}
}
