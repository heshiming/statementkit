"""statementkit — ctypes shim over the C ABI.

Interim binding, zero dependencies: dlopens the shared/static library built
by `cargo build -p statementkit-c`. The pyo3 wheel crate supersedes this
when the API is real. Library discovery: $STATEMENTKIT_LIB, else the
workspace target/release dir next to this file.
"""

from __future__ import annotations

import ctypes
import ctypes.util
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
    root = pathlib.Path(__file__).resolve().parents[2]
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
        "statementkit native library not found; run `cargo build --release -p "
        "statementkit-c` or set $STATEMENTKIT_LIB"
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
