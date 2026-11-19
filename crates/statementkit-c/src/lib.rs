//! C ABI for statementkit.
//!
//! Contract (mirrored by hand in `include/statementkit.h` — the numeric
//! discriminants are contractual): status codes only, no Rust panic may
//! unwind into C, caller never frees anything this library returns.

use std::ffi::CStr;
use std::os::raw::c_char;
use std::panic::{AssertUnwindSafe, catch_unwind};

use statementkit::{Error, Session, parse_float};

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

// Session surface. `SkSession` is opaque; the caller frees only via
// `sk_session_close` and never frees report or error bytes.

/// Read a `char*` array of length `n` into borrowed strs. Empty array
/// with null pointer is fine; a null element is `NullInput`, invalid
/// UTF-8 is `Invalid`.
unsafe fn read_str_array<'a>(
    ptr: *const *const c_char,
    n: usize,
) -> Result<Vec<&'a str>, SkStatus> {
    if n == 0 {
        return Ok(Vec::new());
    }
    if ptr.is_null() {
        return Err(SkStatus::NullInput);
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let p = unsafe { *ptr.add(i) };
        if p.is_null() {
            return Err(SkStatus::NullInput);
        }
        match unsafe { CStr::from_ptr(p) }.to_str() {
            Ok(text) => out.push(text),
            Err(_) => return Err(SkStatus::Invalid),
        }
    }
    Ok(out)
}

/// Copy `msg` into a caller-owned error buffer, NUL-terminated,
/// truncated to fit.
unsafe fn write_err(err: *mut c_char, cap: usize, msg: &str) {
    if err.is_null() || cap == 0 {
        return;
    }
    let bytes = msg.as_bytes();
    let n = bytes.len().min(cap - 1);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), err.cast::<u8>(), n);
        *err.add(n) = 0;
    }
}

/// Open a streaming session over the named targets; with `tickers`
/// non-empty, the session applies that ticker selection and emits a
/// `filter` report fragment. On success `*out` holds the session; on
/// failure the status is returned and, if `err` is non-null, a message
/// is written into it. An unknown target name is `SK_INVALID`.
///
/// # Safety
/// `out` writable; `names`/`tickers` per [`read_str_array`]; `err`
/// writable for `err_cap` bytes or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sk_session_open(
    out: *mut *mut Session,
    names: *const *const c_char,
    n_names: usize,
    tickers: *const *const c_char,
    n_tickers: usize,
    err: *mut c_char,
    err_cap: usize,
) -> SkStatus {
    let guarded = catch_unwind(AssertUnwindSafe(|| {
        if out.is_null() {
            return SkStatus::NullInput;
        }
        unsafe { *out = std::ptr::null_mut() };
        let names = match unsafe { read_str_array(names, n_names) } {
            Ok(names) => names,
            Err(status) => return status,
        };
        let tickers = match unsafe { read_str_array(tickers, n_tickers) } {
            Ok(tickers) => tickers,
            Err(status) => return status,
        };
        match Session::open_selected(&names, &tickers) {
            Ok(session) => {
                unsafe { *out = Box::into_raw(Box::new(session)) };
                SkStatus::Ok
            }
            Err(e) => {
                unsafe { write_err(err, err_cap, &e.to_string()) };
                SkStatus::Invalid
            }
        }
    }));
    guarded.unwrap_or(SkStatus::Panic)
}

/// Feed a byte chunk of the current file. UTF-8 is not required to end
/// at a boundary; incomplete trailing lines carry to the next chunk.
///
/// # Safety
/// `s` from `sk_session_open`; `data` readable for `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sk_session_feed(s: *mut Session, data: *const u8, len: usize) -> SkStatus {
    let guarded = catch_unwind(AssertUnwindSafe(|| {
        if s.is_null() || (data.is_null() && len > 0) {
            return SkStatus::NullInput;
        }
        let bytes = if len == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(data, len) }
        };
        unsafe { (*s).feed(bytes) };
        SkStatus::Ok
    }));
    guarded.unwrap_or(SkStatus::Panic)
}

/// Close the current file; start the next one with [`sk_session_feed`].
///
/// # Safety
/// `s` from `sk_session_open`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sk_session_end_file(s: *mut Session) -> SkStatus {
    let guarded = catch_unwind(AssertUnwindSafe(|| {
        if s.is_null() {
            return SkStatus::NullInput;
        }
        unsafe { (*s).end_file() };
        SkStatus::Ok
    }));
    guarded.unwrap_or(SkStatus::Panic)
}

/// Report length in bytes, excluding the NUL the two-call idiom adds.
///
/// # Safety
/// `s` from `sk_session_open`; `out_len` writable or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sk_session_report_len(s: *const Session, out_len: *mut usize) -> SkStatus {
    let guarded = catch_unwind(AssertUnwindSafe(|| {
        if s.is_null() || out_len.is_null() {
            return SkStatus::NullInput;
        }
        unsafe { *out_len = (*s).report_text().len() };
        SkStatus::Ok
    }));
    guarded.unwrap_or(SkStatus::Panic)
}

/// Write the report (compact JSON, numeric literals unquoted) into
/// `buf` of `cap` bytes, NUL-terminated. `cap` must be at least
/// `report_len + 1` from the two-call idiom; too small is `SK_INVALID`
/// with `buf` untouched.
///
/// # Safety
/// `s` from `sk_session_open`; `buf` writable for `cap` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sk_session_report(
    s: *const Session,
    buf: *mut c_char,
    cap: usize,
) -> SkStatus {
    let guarded = catch_unwind(AssertUnwindSafe(|| {
        if s.is_null() || buf.is_null() {
            return SkStatus::NullInput;
        }
        let text = unsafe { (*s).report_text() };
        if cap < text.len() + 1 {
            return SkStatus::Invalid;
        }
        unsafe {
            std::ptr::copy_nonoverlapping(text.as_ptr(), buf.cast::<u8>(), text.len());
            *buf.add(text.len()) = 0;
        }
        SkStatus::Ok
    }));
    guarded.unwrap_or(SkStatus::Panic)
}

/// Destroy the session. Null is a no-op `SK_OK`, so shells can wire
/// finalizers without double-free guards.
///
/// # Safety
/// `s` from `sk_session_open`, not already closed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sk_session_close(s: *mut Session) -> SkStatus {
    let guarded = catch_unwind(AssertUnwindSafe(|| {
        if !s.is_null() {
            drop(unsafe { Box::from_raw(s) });
        }
        SkStatus::Ok
    }));
    guarded.unwrap_or(SkStatus::Panic)
}

// Raw-wasm callers (JS loader) need to place bytes in linear memory and
// cannot use libc malloc. Native targets are unaffected.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn sk_wasm_alloc(len: usize) -> *mut u8 {
    match std::alloc::Layout::from_size_align(len, 8) {
        Ok(layout) => unsafe { std::alloc::alloc(layout) },
        Err(_) => std::ptr::null_mut(), // size overflow; caller checks null
    }
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn sk_wasm_free(ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        if let Ok(layout) = std::alloc::Layout::from_size_align(len, 8) {
            unsafe { std::alloc::dealloc(ptr, layout) };
        }
    }
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

    // appended into the tests module of crates/statementkit-c/src/lib.rs
    fn session_open(
        names: &[*const c_char],
        tickers: &[*const c_char],
    ) -> (*mut statementkit::Session, SkStatus, String) {
        let mut err = [0i8; 512];
        let mut out: *mut statementkit::Session = std::ptr::null_mut();
        let status = unsafe {
            sk_session_open(
                &mut out,
                names.as_ptr(),
                names.len(),
                tickers.as_ptr(),
                tickers.len(),
                err.as_mut_ptr().cast(),
                err.len(),
            )
        };
        (out, status, unsafe {
            CStr::from_ptr(err.as_ptr().cast())
                .to_string_lossy()
                .into_owned()
        })
    }

    fn cargs(items: &[&str]) -> (Vec<std::ffi::CString>, Vec<*const c_char>) {
        let strs: Vec<std::ffi::CString> = items
            .iter()
            .map(|s| std::ffi::CString::new(*s).unwrap())
            .collect();
        let ptrs = strs.iter().map(|s| s.as_ptr()).collect();
        (strs, ptrs)
    }

    const CSV: &[u8] = b"Statement,Header,Field,Value\n\
        Statement,Data,Base Currency,USD\n\
        Change in Combined NAV,Header,Field,Value\n\
        Change in Combined NAV,Data,Ending Value,100000\n\
        Trades,Header,Symbol,Description,Side,Quantity,Trade Price,Comm/Fee\n\
        Trades,Data,ABC,Alfa,BUY,10,100,-5\n";

    #[test]
    fn session_stream_round_trip() {
        let (_keep, names) = cargs(&["trades"]);
        let none: Vec<*const c_char> = Vec::new();
        let (s, status, _) = session_open(&names, &none);
        assert_eq!(status, SkStatus::Ok);
        assert!(!s.is_null());
        unsafe {
            assert_eq!(sk_session_feed(s, CSV.as_ptr(), 30), SkStatus::Ok);
            assert_eq!(
                sk_session_feed(s, CSV.as_ptr().add(30), CSV.len() - 30),
                SkStatus::Ok
            );
            assert_eq!(sk_session_end_file(s), SkStatus::Ok);
            let mut len = 0usize;
            assert_eq!(sk_session_report_len(s, &mut len), SkStatus::Ok);
            assert!(len > 0);
            let mut buf = vec![0i8; len + 1];
            assert_eq!(
                sk_session_report(s, buf.as_mut_ptr().cast(), len),
                SkStatus::Invalid
            );
            assert_eq!(
                sk_session_report(s, buf.as_mut_ptr().cast(), len + 1),
                SkStatus::Ok
            );
            let text = CStr::from_ptr(buf.as_ptr().cast())
                .to_str()
                .unwrap()
                .to_string();
            assert!(text.contains("\"trades\""), "{text}");
            assert!(text.contains("\"files\""), "{text}");
            assert_eq!(sk_session_close(s), SkStatus::Ok);
            assert_eq!(sk_session_close(std::ptr::null_mut()), SkStatus::Ok);
        }
    }

    #[test]
    fn session_open_errors_and_nulls() {
        let (_keep, names) = cargs(&["nope"]);
        let none: Vec<*const c_char> = Vec::new();
        let (s, status, msg) = session_open(&names, &none);
        assert_eq!(status, SkStatus::Invalid);
        assert!(s.is_null());
        assert!(msg.contains("unknown targets: nope"), "{msg}");

        let nulls: Vec<*const c_char> = Vec::new();
        let mut out: *mut statementkit::Session = std::ptr::null_mut();
        unsafe {
            assert_eq!(
                sk_session_open(
                    std::ptr::null_mut(),
                    nulls.as_ptr(),
                    0,
                    nulls.as_ptr(),
                    0,
                    std::ptr::null_mut(),
                    0
                ),
                SkStatus::NullInput
            );
            assert_eq!(
                sk_session_open(
                    &mut out,
                    std::ptr::null(),
                    2,
                    nulls.as_ptr(),
                    0,
                    std::ptr::null_mut(),
                    0
                ),
                SkStatus::NullInput
            );
            assert_eq!(
                sk_session_feed(std::ptr::null_mut(), CSV.as_ptr(), 3),
                SkStatus::NullInput
            );
            assert_eq!(
                sk_session_end_file(std::ptr::null_mut()),
                SkStatus::NullInput
            );
            assert_eq!(
                sk_session_report_len(std::ptr::null(), std::ptr::null_mut()),
                SkStatus::NullInput
            );
        }
    }

    #[test]
    fn session_selection_emits_filter() {
        let (_k1, names) = cargs(&["trades"]);
        let (_k2, tickers) = cargs(&["ABC"]);
        let (s, status, _) = session_open(&names, &tickers);
        assert_eq!(status, SkStatus::Ok);
        unsafe {
            assert_eq!(sk_session_feed(s, CSV.as_ptr(), CSV.len()), SkStatus::Ok);
            assert_eq!(sk_session_end_file(s), SkStatus::Ok);
            let mut len = 0usize;
            assert_eq!(sk_session_report_len(s, &mut len), SkStatus::Ok);
            let mut buf = vec![0i8; len + 1];
            assert_eq!(
                sk_session_report(s, buf.as_mut_ptr().cast(), len + 1),
                SkStatus::Ok
            );
            let text = CStr::from_ptr(buf.as_ptr().cast())
                .to_str()
                .unwrap()
                .to_string();
            assert!(text.contains("\"filter\""), "{text}");
            assert_eq!(sk_session_close(s), SkStatus::Ok);
        }
    }
}
