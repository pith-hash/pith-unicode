//! The C ABI surface of `pith-unicode`: the entry points the Python
//! (ctypes), Node (koffi) and Go (cgo) SDKs bind through.
//!
//! The suite's FFI convention, defined by this module and mirrored by
//! every `pith-*` cdylib:
//!
//! * one flat set of `#[unsafe(no_mangle)] pub unsafe extern "C"`
//!   functions — raw pointers plus lengths, no structs across the
//!   boundary;
//! * every function returns a status code (see the constants below),
//!   never a `Result`, never a panic: the normalizer has no panic
//!   path on validated input, and invalid UTF-8 is rejected —
//!   [`PITH_E_REJECTED`] — before the core ever sees it;
//! * the operations hand ownership to the caller and ship a matching
//!   [`pith_unicode_free`]: the buffer is handed out as a boxed slice
//!   together with its exact length, and released by reconstructing
//!   that boxed slice from the same length;
//! * the `unsafe` allowance is confined to this module; the core
//!   normalizer stays unsafe-free behind the crate-root `#![deny]`.

#![allow(unsafe_code)]

use crate::{NormalizationForm, casefold, casefold_simple, is_normalized, nfc, nfd, nfkc, nfkd};

/// Status: success.
pub const PITH_OK: i32 = 0;
/// Status: a caller argument is invalid — a null pointer.
pub const PITH_E_INVALID: i32 = -1;
/// Status: the core refused the input: the bytes are not valid UTF-8
/// (a rejection, never a panic).
pub const PITH_E_REJECTED: i32 = -2;

/// Normalizes `len` bytes at `data` to Unicode Normalization Form C
/// (canonical composition) and hands the UTF-8 result to the caller.
///
/// On success the function allocates a buffer, writes its address
/// through `out`, its length through `out_len`, and returns
/// [`PITH_OK`]; the caller owns the buffer and must release it with
/// [`pith_unicode_free`], passing back the same pointer *and* the
/// length that came back through `out_len`. Empty input is valid and
/// produces an empty (zero-length) buffer.
///
/// # Safety
///
/// `data` must point to `len` readable bytes; `out` to one writable
/// pointer; `out_len` to one writable `usize`. All must stay valid for
/// the duration of the call; the function retains nothing.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pith_unicode_nfc(
    data: *const u8,
    len: usize,
    out: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    unsafe { normalize(data, len, out, out_len, nfc_bytes) }
}

/// Normalizes `len` bytes at `data` to Unicode Normalization Form D
/// (canonical decomposition), with the same buffer contract as
/// [`pith_unicode_nfc`].
///
/// # Safety
///
/// `data` must point to `len` readable bytes; `out` to one writable
/// pointer; `out_len` to one writable `usize`. All must stay valid for
/// the duration of the call; the function retains nothing.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pith_unicode_nfd(
    data: *const u8,
    len: usize,
    out: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    unsafe { normalize(data, len, out, out_len, nfd_bytes) }
}

/// Normalizes `len` bytes at `data` to Unicode Normalization Form KC
/// (compatibility composition), with the same buffer contract as
/// [`pith_unicode_nfc`].
///
/// # Safety
///
/// Same contract as [`pith_unicode_nfd`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pith_unicode_nfkc(
    data: *const u8,
    len: usize,
    out: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    unsafe { normalize(data, len, out, out_len, nfkc_bytes) }
}

/// Normalizes `len` bytes at `data` to Unicode Normalization Form KD
/// (compatibility decomposition), with the same buffer contract as
/// [`pith_unicode_nfc`].
///
/// # Safety
///
/// Same contract as [`pith_unicode_nfd`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pith_unicode_nfkd(
    data: *const u8,
    len: usize,
    out: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    unsafe { normalize(data, len, out, out_len, nfkd_bytes) }
}

/// Answers whether the `len` bytes at `data` are already in the
/// normalization form `form` — 1 = NFC, 2 = NFD, 3 = NFKC, 4 = NFKD
/// ([`NormalizationForm::code`]).
///
/// On success writes `1` (already normalized) or `0` through `out` and
/// returns [`PITH_OK`]. An out-of-range `form` is [`PITH_E_INVALID`];
/// bytes that are not valid UTF-8 are [`PITH_E_REJECTED`].
///
/// # Safety
///
/// `data` must point to `len` readable bytes and `out` to one writable
/// byte, all valid for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pith_unicode_is_normalized(
    form: u32,
    data: *const u8,
    len: usize,
    out: *mut u8,
) -> i32 {
    if data.is_null() || out.is_null() {
        return PITH_E_INVALID;
    }
    let Some(form) = NormalizationForm::from_code(form) else {
        return PITH_E_INVALID;
    };
    let bytes = unsafe { core::slice::from_raw_parts(data, len) };
    match core::str::from_utf8(bytes) {
        Ok(text) => {
            unsafe { *out = u8::from(is_normalized(form, text)) };
            PITH_OK
        }
        Err(_) => PITH_E_REJECTED,
    }
}

/// Folds `len` bytes at `data` to their full case folding (UAX #44
/// statuses `C`+`F`), with the same buffer contract as
/// [`pith_unicode_nfc`]. Turkic `T` entries are not applied.
///
/// # Safety
///
/// Same contract as [`pith_unicode_nfd`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pith_unicode_casefold(
    data: *const u8,
    len: usize,
    out: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    unsafe { normalize(data, len, out, out_len, casefold_bytes) }
}

/// Folds `len` bytes at `data` to their simple case folding (statuses
/// `C`+`S`), with the same buffer contract as [`pith_unicode_nfc`].
///
/// # Safety
///
/// Same contract as [`pith_unicode_nfd`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pith_unicode_casefold_simple(
    data: *const u8,
    len: usize,
    out: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    unsafe { normalize(data, len, out, out_len, casefold_simple_bytes) }
}

/// Releases a buffer handed out by [`pith_unicode_nfc`] or
/// [`pith_unicode_nfd`].
///
/// # Safety
///
/// `ptr` must be a pointer returned by one of the normalization
/// functions with the `out_len` value that came back with it, and must
/// not have been released (or otherwise freed) before. Null is
/// accepted and ignored, so callers can free unconditionally on the
/// error path.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pith_unicode_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() {
        return;
    }
    let slice = unsafe { core::slice::from_raw_parts_mut(ptr, len) };
    drop(unsafe { Box::from_raw(slice) });
}

/// The safe core of [`pith_unicode_nfc`]: validate the bytes as UTF-8,
/// then normalize. Invalid UTF-8 maps to [`PITH_E_REJECTED`].
fn nfc_bytes(bytes: &[u8]) -> Result<Vec<u8>, i32> {
    let text = core::str::from_utf8(bytes).map_err(|_| PITH_E_REJECTED)?;
    Ok(nfc(text).into_bytes())
}

/// The safe core of [`pith_unicode_nfd`]: validate the bytes as UTF-8,
/// then normalize. Invalid UTF-8 maps to [`PITH_E_REJECTED`].
fn nfd_bytes(bytes: &[u8]) -> Result<Vec<u8>, i32> {
    let text = core::str::from_utf8(bytes).map_err(|_| PITH_E_REJECTED)?;
    Ok(nfd(text).into_bytes())
}

/// The safe core of [`pith_unicode_nfkc`].
fn nfkc_bytes(bytes: &[u8]) -> Result<Vec<u8>, i32> {
    let text = core::str::from_utf8(bytes).map_err(|_| PITH_E_REJECTED)?;
    Ok(nfkc(text).into_bytes())
}

/// The safe core of [`pith_unicode_nfkd`].
fn nfkd_bytes(bytes: &[u8]) -> Result<Vec<u8>, i32> {
    let text = core::str::from_utf8(bytes).map_err(|_| PITH_E_REJECTED)?;
    Ok(nfkd(text).into_bytes())
}

/// The safe core of [`pith_unicode_casefold`].
fn casefold_bytes(bytes: &[u8]) -> Result<Vec<u8>, i32> {
    let text = core::str::from_utf8(bytes).map_err(|_| PITH_E_REJECTED)?;
    Ok(casefold(text).into_bytes())
}

/// The safe core of [`pith_unicode_casefold_simple`].
fn casefold_simple_bytes(bytes: &[u8]) -> Result<Vec<u8>, i32> {
    let text = core::str::from_utf8(bytes).map_err(|_| PITH_E_REJECTED)?;
    Ok(casefold_simple(text).into_bytes())
}

/// Pointer plumbing shared by both exports: validate the raw
/// arguments, run the safe core, and hand the result to the caller as
/// an owned boxed slice.
///
/// # Safety
///
/// Same contract as the exported functions: `data` must point to
/// `len` readable bytes, `out` and `out_len` to one writable slot
/// each, all valid for the duration of the call.
unsafe fn normalize(
    data: *const u8,
    len: usize,
    out: *mut *mut u8,
    out_len: *mut usize,
    core: fn(&[u8]) -> Result<Vec<u8>, i32>,
) -> i32 {
    if data.is_null() || out.is_null() || out_len.is_null() {
        return PITH_E_INVALID;
    }
    let bytes = unsafe { core::slice::from_raw_parts(data, len) };
    match core(bytes) {
        Ok(normalized) => {
            let len = normalized.len();
            // Hand the exact-length buffer to the caller; `pith_unicode_free`
            // reconstructs the boxed slice from the same length.
            let ptr = Box::into_raw(normalized.into_boxed_slice());
            unsafe {
                *out = ptr.cast::<u8>();
                *out_len = len;
            }
            PITH_OK
        }
        Err(status) => status,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        PITH_E_INVALID, PITH_E_REJECTED, PITH_OK, casefold_bytes, casefold_simple_bytes, nfc_bytes,
        nfd_bytes, nfkc_bytes, nfkd_bytes, pith_unicode_casefold, pith_unicode_casefold_simple,
        pith_unicode_free, pith_unicode_is_normalized, pith_unicode_nfc, pith_unicode_nfd,
        pith_unicode_nfkc, pith_unicode_nfkd,
    };

    /// Every buffer-handed-out export, for the shared-contract loops.
    const BUFFER_OPS: &[unsafe extern "C" fn(*const u8, usize, *mut *mut u8, *mut usize) -> i32] =
        &[
            pith_unicode_nfc,
            pith_unicode_nfd,
            pith_unicode_nfkc,
            pith_unicode_nfkd,
            pith_unicode_casefold,
            pith_unicode_casefold_simple,
        ];

    /// `reference.json` vector 0, driven through the raw FFI: NFC is a
    /// fixed point on the precomposed input and NFD decomposes it
    /// exactly as recorded. Pinned literally so a wrongly regenerated
    /// reference.json cannot mask drift.
    #[test]
    fn ffi_reproduces_the_first_reference_vector() {
        let input = [0xe1, 0xb8, 0x8a]; // U+1E0A LATIN CAPITAL LETTER D WITH DOT ABOVE
        let want_nfc = [0xe1, 0xb8, 0x8a];
        let want_nfd = [0x44, 0xcc, 0x87]; // "D" + U+0307 COMBINING DOT ABOVE

        let mut out: *mut u8 = core::ptr::null_mut();
        let mut out_len: usize = 0;
        let status =
            unsafe { pith_unicode_nfc(input.as_ptr(), input.len(), &mut out, &mut out_len) };
        assert_eq!(status, PITH_OK);
        assert_eq!(out_len, want_nfc.len());
        assert_eq!(
            unsafe { core::slice::from_raw_parts(out, out_len) },
            &want_nfc
        );
        unsafe { pith_unicode_free(out, out_len) };

        let status =
            unsafe { pith_unicode_nfd(input.as_ptr(), input.len(), &mut out, &mut out_len) };
        assert_eq!(status, PITH_OK);
        assert_eq!(out_len, want_nfd.len());
        assert_eq!(
            unsafe { core::slice::from_raw_parts(out, out_len) },
            &want_nfd
        );
        unsafe { pith_unicode_free(out, out_len) };
    }

    /// `reference.json`'s last vector at implementation time, pinned
    /// literally: composed input stays composed through NFC and round
    /// trips through NFD.
    #[test]
    fn ffi_reproduces_the_last_reference_vector() {
        let input: [u8; 8] = [0x54, 0x61, 0xcc, 0x82, 0xcc, 0x80, 0x6e, 0x67];
        let want_nfc: [u8; 6] = [0x54, 0xe1, 0xba, 0xa7, 0x6e, 0x67];

        let mut out: *mut u8 = core::ptr::null_mut();
        let mut out_len: usize = 0;
        let status =
            unsafe { pith_unicode_nfc(input.as_ptr(), input.len(), &mut out, &mut out_len) };
        assert_eq!(status, PITH_OK);
        assert_eq!(out_len, want_nfc.len());
        assert_eq!(
            unsafe { core::slice::from_raw_parts(out, out_len) },
            &want_nfc
        );
        unsafe { pith_unicode_free(out, out_len) };

        let status =
            unsafe { pith_unicode_nfd(input.as_ptr(), input.len(), &mut out, &mut out_len) };
        assert_eq!(status, PITH_OK);
        assert_eq!(out_len, input.len());
        assert_eq!(unsafe { core::slice::from_raw_parts(out, out_len) }, &input);
        unsafe { pith_unicode_free(out, out_len) };
    }

    /// Empty input is valid: the FFI hands back a zero-length buffer
    /// (a dangling but aligned pointer from the empty boxed slice),
    /// which the same-length free accepts.
    #[test]
    fn empty_input_round_trips_through_the_ffi() {
        for op in BUFFER_OPS {
            let mut out: *mut u8 = core::ptr::null_mut();
            let mut out_len: usize = 0;
            let status = unsafe { op(b"".as_ptr(), 0, &mut out, &mut out_len) };
            assert_eq!(status, PITH_OK);
            assert_eq!(out_len, 0);
            // A zero-length read off the handed-out pointer is legal.
            assert!(unsafe { core::slice::from_raw_parts(out, out_len) }.is_empty());
            unsafe { pith_unicode_free(out, out_len) };
        }
    }

    /// Null pointers are [`PITH_E_INVALID`] for both operations; a
    /// null buffer is a legal free.
    #[test]
    fn ffi_rejects_null_pointers() {
        let mut out: *mut u8 = core::ptr::null_mut();
        let mut out_len: usize = 0;
        let text = [0x61u8];

        for op in BUFFER_OPS {
            let null_data = unsafe { op(core::ptr::null(), 0, &mut out, &mut out_len) };
            assert_eq!(null_data, PITH_E_INVALID);

            let null_out = unsafe {
                op(
                    text.as_ptr(),
                    text.len(),
                    core::ptr::null_mut(),
                    &mut out_len,
                )
            };
            assert_eq!(null_out, PITH_E_INVALID);

            let null_out_len =
                unsafe { op(text.as_ptr(), text.len(), &mut out, core::ptr::null_mut()) };
            assert_eq!(null_out_len, PITH_E_INVALID);
        }

        unsafe { pith_unicode_free(core::ptr::null_mut(), 0) };
    }

    /// Invalid UTF-8 is [`PITH_E_REJECTED`] for both operations — a
    /// status, never a panic and never a crash.
    #[test]
    fn ffi_rejects_invalid_utf8() {
        let bad = [0xffu8, 0xfe];
        for op in BUFFER_OPS {
            let mut out: *mut u8 = core::ptr::null_mut();
            let mut out_len: usize = 0;
            let status = unsafe { op(bad.as_ptr(), bad.len(), &mut out, &mut out_len) };
            assert_eq!(status, PITH_E_REJECTED);
        }
    }

    /// The safe cores mirror the exports without raw pointers: the
    /// first reference vector passes byte-exact, invalid UTF-8 and
    /// nothing else is refused, and empty input is a valid identity.
    #[test]
    fn safe_cores_mirror_the_exports() {
        assert_eq!(nfc_bytes(b"\xe1\xb8\x8a"), Ok(b"\xe1\xb8\x8a".to_vec()));
        assert_eq!(nfd_bytes(b"\xe1\xb8\x8a"), Ok(b"D\xcc\x87".to_vec()));
        assert_eq!(nfc_bytes(b"\xff\xfe"), Err(PITH_E_REJECTED));
        assert_eq!(nfd_bytes(b"\xff\xfe"), Err(PITH_E_REJECTED));
        assert_eq!(nfc_bytes(b""), Ok(Vec::new()));
        assert_eq!(nfd_bytes(b""), Ok(Vec::new()));
        // Tier-1 cores: compatibility forms and both folding flavors.
        assert_eq!(nfkc_bytes(b"\xef\xac\x81"), Ok(b"fi".to_vec()));
        assert_eq!(nfkd_bytes(b"\xef\xac\x81"), Ok(b"fi".to_vec()));
        assert_eq!(nfkc_bytes(b"\xff\xfe"), Err(PITH_E_REJECTED));
        assert_eq!(nfkd_bytes(b"\xff\xfe"), Err(PITH_E_REJECTED));
        assert_eq!(casefold_bytes(b"\xc3\x9f"), Ok(b"ss".to_vec()));
        assert_eq!(
            casefold_simple_bytes(b"\xe1\xba\x9e"),
            Ok(b"\xc3\x9f".to_vec())
        );
        assert_eq!(casefold_bytes(b"\xff\xfe"), Err(PITH_E_REJECTED));
        assert_eq!(casefold_simple_bytes(b"\xff\xfe"), Err(PITH_E_REJECTED));
        assert_eq!(nfkc_bytes(b""), Ok(Vec::new()));
        assert_eq!(nfkd_bytes(b""), Ok(Vec::new()));
        assert_eq!(casefold_bytes(b""), Ok(Vec::new()));
        assert_eq!(casefold_simple_bytes(b""), Ok(Vec::new()));
    }

    /// The compatibility exports on a pinned case: the ﬁ ligature is
    /// NFC-stable and NFKC/NFKD-expand to "fi".
    #[test]
    fn ffi_compatibility_exports_expand_the_ligature() {
        let input: [u8; 3] = [0xef, 0xac, 0x81]; // U+FB01 LATIN SMALL LIGATURE FI
        for (op, want) in [
            (
                pith_unicode_nfkc
                    as unsafe extern "C" fn(*const u8, usize, *mut *mut u8, *mut usize) -> i32,
                &b"fi"[..],
            ),
            (
                pith_unicode_nfkd
                    as unsafe extern "C" fn(*const u8, usize, *mut *mut u8, *mut usize) -> i32,
                &b"fi"[..],
            ),
        ] {
            let mut out: *mut u8 = core::ptr::null_mut();
            let mut out_len: usize = 0;
            let status = unsafe { op(input.as_ptr(), input.len(), &mut out, &mut out_len) };
            assert_eq!(status, PITH_OK);
            assert_eq!(unsafe { core::slice::from_raw_parts(out, out_len) }, want);
            unsafe { pith_unicode_free(out, out_len) };
        }
    }

    /// The folding exports on pinned cases: ß fully folds to "ss" and
    /// ẞ folds simply to ß.
    #[test]
    fn ffi_folding_exports_fold_pinned_inputs() {
        let sharp_s: [u8; 2] = [0xc3, 0x9f]; // ß
        let capital_sharp_s: [u8; 3] = [0xe1, 0xba, 0x9e]; // ẞ

        let mut out: *mut u8 = core::ptr::null_mut();
        let mut out_len: usize = 0;
        let status = unsafe {
            pith_unicode_casefold(sharp_s.as_ptr(), sharp_s.len(), &mut out, &mut out_len)
        };
        assert_eq!(status, PITH_OK);
        assert_eq!(unsafe { core::slice::from_raw_parts(out, out_len) }, b"ss");
        unsafe { pith_unicode_free(out, out_len) };

        let status = unsafe {
            pith_unicode_casefold_simple(
                capital_sharp_s.as_ptr(),
                capital_sharp_s.len(),
                &mut out,
                &mut out_len,
            )
        };
        assert_eq!(status, PITH_OK);
        assert_eq!(
            unsafe { core::slice::from_raw_parts(out, out_len) },
            &sharp_s
        );
        unsafe { pith_unicode_free(out, out_len) };
    }

    /// `pith_unicode_is_normalized`: every form code answers the exact
    /// slow-path truth on pinned inputs, out-of-range codes are
    /// [`PITH_E_INVALID`], invalid UTF-8 is [`PITH_E_REJECTED`], and a
    /// null argument is [`PITH_E_INVALID`].
    #[test]
    fn ffi_is_normalized_answers_exact_truth() {
        let ligature: [u8; 3] = [0xef, 0xac, 0x81]; // ﬁ: NFC/NFD yes, NFKC/NFKD no
        let composed: [u8; 2] = [0xc3, 0xa0]; // à: NFC/NFKC yes, NFD/NFKD no
        let decomposed: [u8; 3] = [0x61, 0xcc, 0x80]; // a + grave: NFD yes, NFC no

        for (form, bytes, want) in [
            (1u32, &composed[..], true),
            (2, &composed[..], false),
            (3, &composed[..], true),
            (4, &composed[..], false),
            (1, &decomposed[..], false),
            (2, &decomposed[..], true),
            (3, &decomposed[..], false),
            (4, &decomposed[..], true),
            (1, &ligature[..], true),
            (3, &ligature[..], false),
        ] {
            let mut out: u8 = 0xff;
            let status =
                unsafe { pith_unicode_is_normalized(form, bytes.as_ptr(), bytes.len(), &mut out) };
            assert_eq!(status, PITH_OK, "form {form}");
            assert_eq!(out, u8::from(want), "form {form}");
        }

        let mut out: u8 = 0xff;
        let status = unsafe { pith_unicode_is_normalized(0, composed.as_ptr(), 2, &mut out) };
        assert_eq!(status, PITH_E_INVALID);
        let status = unsafe { pith_unicode_is_normalized(5, composed.as_ptr(), 2, &mut out) };
        assert_eq!(status, PITH_E_INVALID);
        let status = unsafe { pith_unicode_is_normalized(1, b"\xff\xfe".as_ptr(), 2, &mut out) };
        assert_eq!(status, PITH_E_REJECTED);
        let status = unsafe { pith_unicode_is_normalized(1, core::ptr::null(), 0, &mut out) };
        assert_eq!(status, PITH_E_INVALID);
        let status =
            unsafe { pith_unicode_is_normalized(1, composed.as_ptr(), 2, core::ptr::null_mut()) };
        assert_eq!(status, PITH_E_INVALID);
    }
}
