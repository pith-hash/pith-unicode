// SPDX-License-Identifier: MIT
// Copyright (c) 2026 pith-hash

//! Integration-level FFI smoke: the exported C ABI entry points work
//! from outside the crate, the way the Python/Node/Go SDKs drive them
//! — raw pointers in, an owned buffer out, released by the same-length
//! free. The per-branch refusal coverage lives in `src/ffi.rs`'s unit
//! tests; this file keeps the exports honest across crate boundaries.

use pith_unicode::ffi::{
    PITH_E_INVALID, PITH_E_REJECTED, PITH_OK, pith_unicode_free, pith_unicode_nfc, pith_unicode_nfd,
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
