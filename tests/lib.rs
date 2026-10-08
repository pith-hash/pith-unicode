// SPDX-License-Identifier: MIT
// Copyright (c) 2026 pith-hash

//! Integration-level FFI smoke: the exported C ABI entry points work
//! from outside the crate, the way the Python/Node/Go SDKs drive them
//! — raw pointers in, an owned buffer out, released by the same-length
//! free. The per-branch refusal coverage lives in `src/ffi.rs`'s unit
//! tests; this file keeps the exports honest across crate boundaries.

use pith_unicode::ffi::{
    PITH_E_INVALID, PITH_E_REJECTED, PITH_OK, pith_unicode_casefold, pith_unicode_casefold_simple,
    pith_unicode_free, pith_unicode_is_normalized, pith_unicode_nfc, pith_unicode_nfd,
    pith_unicode_nfkc, pith_unicode_nfkd,
};

/// `reference.json` vector 0 through the raw FFI: NFC is a fixed
/// point, NFD decomposes to "D" + U+0307.
#[test]
fn ffi_exports_work_across_the_crate_boundary() {
    let input = [0xe1, 0xb8, 0x8a];

    let mut out: *mut u8 = core::ptr::null_mut();
    let mut out_len: usize = 0;
    let status = unsafe { pith_unicode_nfc(input.as_ptr(), input.len(), &mut out, &mut out_len) };
    assert_eq!(status, PITH_OK);
    assert_eq!(out_len, 3);
    assert_eq!(unsafe { core::slice::from_raw_parts(out, out_len) }, &input);
    unsafe { pith_unicode_free(out, out_len) };

    let status = unsafe { pith_unicode_nfd(input.as_ptr(), input.len(), &mut out, &mut out_len) };
    assert_eq!(status, PITH_OK);
    assert_eq!(out_len, 3);
    assert_eq!(
        unsafe { core::slice::from_raw_parts(out, out_len) },
        &[0x44, 0xcc, 0x87]
    );
    unsafe { pith_unicode_free(out, out_len) };
}

/// Refusals from outside the crate: a null data pointer is
/// [`PITH_E_INVALID`], invalid UTF-8 is [`PITH_E_REJECTED`], and a
/// null buffer is a legal free.
#[test]
fn ffi_refusals_across_the_crate_boundary() {
    let mut out: *mut u8 = core::ptr::null_mut();
    let mut out_len: usize = 0;
    let status = unsafe { pith_unicode_nfc(core::ptr::null(), 0, &mut out, &mut out_len) };
    assert_eq!(status, PITH_E_INVALID);

    let status = unsafe { pith_unicode_nfd(b"\xff\xfe".as_ptr(), 2, &mut out, &mut out_len) };
    assert_eq!(status, PITH_E_REJECTED);

    unsafe { pith_unicode_free(core::ptr::null_mut(), 0) };
}

/// The tier-1 exports work from outside the crate the same way: the ligature
/// expands under NFKC/NFKD, both folds answer their pinned cases, and the
/// quick check reports the ligature NFC-stable but not NFKC-stable.
#[test]
fn ffi_tier1_exports_work_across_the_crate_boundary() {
    let ligature: [u8; 3] = [0xef, 0xac, 0x81]; // U+FB01 LATIN SMALL LIGATURE FI
    for op in [pith_unicode_nfkc, pith_unicode_nfkd] {
        let mut out: *mut u8 = core::ptr::null_mut();
        let mut out_len: usize = 0;
        let status = unsafe { op(ligature.as_ptr(), ligature.len(), &mut out, &mut out_len) };
        assert_eq!(status, PITH_OK);
        assert_eq!(unsafe { core::slice::from_raw_parts(out, out_len) }, b"fi");
        unsafe { pith_unicode_free(out, out_len) };
    }

    let sharp_s: [u8; 2] = [0xc3, 0x9f]; // LATIN SMALL LETTER SHARP S
    let mut out: *mut u8 = core::ptr::null_mut();
    let mut out_len: usize = 0;
    let status =
        unsafe { pith_unicode_casefold(sharp_s.as_ptr(), sharp_s.len(), &mut out, &mut out_len) };
    assert_eq!(status, PITH_OK);
    assert_eq!(unsafe { core::slice::from_raw_parts(out, out_len) }, b"ss");
    unsafe { pith_unicode_free(out, out_len) };

    let capital: [u8; 3] = [0xe1, 0xba, 0x9e]; // LATIN CAPITAL LETTER SHARP S
    let status = unsafe {
        pith_unicode_casefold_simple(capital.as_ptr(), capital.len(), &mut out, &mut out_len)
    };
    assert_eq!(status, PITH_OK);
    assert_eq!(
        unsafe { core::slice::from_raw_parts(out, out_len) },
        &sharp_s
    );
    unsafe { pith_unicode_free(out, out_len) };

    let mut answer: u8 = 0xff;
    let status =
        unsafe { pith_unicode_is_normalized(1, ligature.as_ptr(), ligature.len(), &mut answer) };
    assert_eq!(status, PITH_OK);
    assert_eq!(answer, 1);
    let status =
        unsafe { pith_unicode_is_normalized(3, ligature.as_ptr(), ligature.len(), &mut answer) };
    assert_eq!(status, PITH_OK);
    assert_eq!(answer, 0);
    let status =
        unsafe { pith_unicode_is_normalized(0, ligature.as_ptr(), ligature.len(), &mut answer) };
    assert_eq!(status, PITH_E_INVALID);
    let status = unsafe { pith_unicode_is_normalized(2, b"\xff\xfe".as_ptr(), 2, &mut answer) };
    assert_eq!(status, PITH_E_REJECTED);
    let status = unsafe { pith_unicode_is_normalized(4, core::ptr::null(), 0, &mut answer) };
    assert_eq!(status, PITH_E_INVALID);
}
