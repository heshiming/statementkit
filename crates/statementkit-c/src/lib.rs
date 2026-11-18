//! C ABI for statementkit.
//!
//! Contract (mirrored by hand in `include/statementkit.h` — the numeric
//! discriminants are contractual): status codes only, no Rust panic may
//! unwind into C, caller never frees anything this library returns.

use std::ffi::CStr;
use std::os::raw::c_char;
use std::panic::{AssertUnwindSafe, catch_unwind};

use statementkit::{Error, parse_float};

/// Result codes. Keep in sync with `SkStatus` in the header.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkStatus {
    Ok = 0,
    NullInput = 1,
    Empty = 2,
    Invalid = 3,
    /// An internal Rust panic was caught at the boundary.
    Panic = 4,
}

/// Parse a NUL-terminated float literal; writes the value to `out`.
///
/// Returns `SK_OK` (0) on success. `input` or `out` null is `SK_NULL_INPUT`.
///
/// # Safety
/// `input` must be null or a valid NUL-terminated C string; `out` must be
/// null or a writable `double`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sk_parse_float(input: *const c_char, out: *mut f64) -> SkStatus {
    let guarded = catch_unwind(AssertUnwindSafe(|| {
        if input.is_null() || out.is_null() {
            return SkStatus::NullInput;
        }
        let text = match unsafe { CStr::from_ptr(input) }.to_str() {
            Ok(text) => text,
            Err(_) => return SkStatus::Invalid,
        };
        match parse_float(text) {
            Ok(value) => {
                unsafe { *out = value };
                SkStatus::Ok
            }
            Err(Error::Empty) => SkStatus::Empty,
            Err(Error::Invalid) => SkStatus::Invalid,
        }
    }));
    guarded.unwrap_or(SkStatus::Panic)
}

/// Library version as a static NUL-terminated string. Do not free.
static VERSION: &[u8] = concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes();

#[unsafe(no_mangle)]
pub extern "C" fn sk_version() -> *const c_char {
    VERSION.as_ptr().cast()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: *const c_char) -> (SkStatus, f64) {
        let mut out = f64::NAN;
        let status = unsafe { sk_parse_float(input, &mut out) };
        (status, out)
    }

    #[test]
    fn ffi_round_trip() {
        let (status, value) = parse(c"-3.5e2".as_ptr());
        assert_eq!((status, value), (SkStatus::Ok, -350.0));

        assert_eq!(parse(c"".as_ptr()).0, SkStatus::Empty);
        assert_eq!(parse(c"1,000".as_ptr()).0, SkStatus::Invalid);
        assert_eq!(parse(c"NaN".as_ptr()).0, SkStatus::Invalid);
    }

    #[test]
    fn ffi_null_inputs() {
        let null = std::ptr::null();
        let null_mut = std::ptr::null_mut();
        unsafe {
            assert_eq!(sk_parse_float(null, null_mut), SkStatus::NullInput);
            let mut out = 0.0;
            assert_eq!(sk_parse_float(null, &mut out), SkStatus::NullInput);
            assert_eq!(
                sk_parse_float(c"1.0".as_ptr(), null_mut),
                SkStatus::NullInput
            );
        }
    }

    #[test]
    fn ffi_rejects_non_utf8() {
        let bad = [0xFFu8, 0x00];
        assert_eq!(parse(bad.as_ptr().cast()).0, SkStatus::Invalid);
    }

    #[test]
    fn version_is_set() {
        let version = unsafe { CStr::from_ptr(sk_version()) };
        assert_eq!(version.to_str().unwrap(), env!("CARGO_PKG_VERSION"));
    }
}
