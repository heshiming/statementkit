"""statementkit — ctypes shim over the C ABI.

Interim binding, zero dependencies: dlopens the C ABI shared library.
pip wheels bundle the library next to this file; in-repo builds find it in
the workspace target/release dir. The pyo3 wheel crate supersedes this
when the API is real. Library discovery: $STATEMENTKIT_LIB, else the
package dir, else workspace target/release, else the loader path.
"""

from __future__ import annotations

import ctypes
import ctypes.util
import glob
import os
import pathlib


class StatementKitError(Exception):
    """Base for all statementkit failures, carrying the ABI status code."""

    def __init__(self, code: int, message: str):
        super().__init__(message)
        self.code = code


class NullInputError(StatementKitError):
    pass


class EmptyInputError(StatementKitError):
    pass


class InvalidInputError(StatementKitError):
    pass


class PanicError(StatementKitError):
    pass


_ERRORS = {
    1: NullInputError,
    2: EmptyInputError,
    3: InvalidInputError,
    4: PanicError,
}


def _find_library() -> str:
    env = os.environ.get("STATEMENTKIT_LIB")
    if env:
        return env
    here = pathlib.Path(__file__).resolve().parent
    for hit in glob.glob(str(here / "*statementkit_c*.so")) + \
            glob.glob(str(here / "*statementkit_c*.dylib")) + \
            glob.glob(str(here / "*statementkit_c*.dll")) + \
            glob.glob(str(here / "*statementkit_c*.pyd")):
        return hit
    root = here.parents[2]
    names = ("libstatementkit_c.so", "libstatementkit_c.dylib", "statementkit_c.dll")
    for name in names:
        candidate = root / "target" / "release" / name
        if candidate.exists():
            return str(candidate)
    for name in names:
        found = ctypes.util.find_library(name)
        if found:
            return found
    raise ImportError(
        "statementkit native library not found; install the statementkit "
        "wheel, run `cargo build --release -p statementkit-c` in the "
        "workspace, or set $STATEMENTKIT_LIB"
    )


_lib = ctypes.CDLL(_find_library())
_lib.sk_parse_float.argtypes = [ctypes.c_char_p, ctypes.POINTER(ctypes.c_double)]
_lib.sk_parse_float.restype = ctypes.c_int
_lib.sk_version.restype = ctypes.c_char_p


def parse_float(text: str | None) -> float:
    """Parse a bare float literal; raises a StatementKitError subclass."""
    if text is None:
        raise NullInputError(1, "input is None")
    out = ctypes.c_double()
    status = _lib.sk_parse_float(text.encode("utf-8"), ctypes.byref(out))
    if status != 0:
        error = _ERRORS.get(status, PanicError)
        raise error(status, f"sk_parse_float failed with status {status}")
    return out.value


def version() -> str:
    return _lib.sk_version().decode()


_lib.sk_session_open.argtypes = [ctypes.POINTER(ctypes.c_void_p), ctypes.POINTER(ctypes.c_char_p),
                     ctypes.c_size_t, ctypes.POINTER(ctypes.c_char_p),
                     ctypes.c_size_t, ctypes.c_char_p, ctypes.c_size_t]
_lib.sk_session_feed.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_size_t]
_lib.sk_session_end_file.argtypes = [ctypes.c_void_p]
_lib.sk_session_report_len.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_size_t)]
_lib.sk_session_report.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_size_t]
_lib.sk_session_close.argtypes = [ctypes.c_void_p]


class Session:
    """Push streaming session: feed bytes per file, end_file between files.

    report() returns the compact JSON text; report_json() parses it with
    decimal.Decimal so numeric literals keep full precision."""

    def __init__(self, targets, tickers=()):
        names = [t.encode("utf-8") for t in targets]
        tick = [t.encode("utf-8") for t in tickers]
        arr_n = (ctypes.c_char_p * len(names))(*names) if names else None
        arr_t = (ctypes.c_char_p * len(tick))(*tick) if tick else None
        err = ctypes.create_string_buffer(512)
        handle = ctypes.c_void_p()
        status = _lib.sk_session_open(ctypes.byref(handle), arr_n, len(names),
                             arr_t, len(tick), err, 512)
        if status != 0:
            error = _ERRORS.get(status, PanicError)
            raise error(status, err.value.decode("utf-8", "replace") or
                        f"session open failed with status {status}")
        self._handle = handle

    def _check(self, status):
        if status != 0:
            error = _ERRORS.get(status, PanicError)
            raise error(status, f"session call failed with status {status}")

    def feed(self, data: bytes):
        if data:
            self._check(_lib.sk_session_feed(self._handle, data, len(data)))

    def end_file(self):
        self._check(_lib.sk_session_end_file(self._handle))

    def report(self) -> str:
        length = ctypes.c_size_t()
        self._check(_lib.sk_session_report_len(self._handle, ctypes.byref(length)))
        buf = ctypes.create_string_buffer(length.value + 1)
        self._check(_lib.sk_session_report(self._handle, buf, length.value + 1))
        return buf.value.decode("utf-8")

    def report_json(self, **loads_kwargs):
        import json
        from decimal import Decimal
        loads_kwargs.setdefault("parse_float", Decimal)
        return json.loads(self.report(), **loads_kwargs)

    def close(self):
        if self._handle is not None:
            _lib.sk_session_close(self._handle)
            self._handle = None

    def __enter__(self):
        return self

    def __exit__(self, *_exc):
        self.close()

    def __del__(self):
        try:
            self.close()
        except Exception:
            pass
