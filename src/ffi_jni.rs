//! The JNI surface of `pith-unicode`: the `Java_hash_pith_unicode_PithUnicode_*`
//! exports the Java SDK (`sdk/java`) binds its `native` methods through.
//!
//! The C ABI of [`crate::ffi`] is untouched: JNI requires exports named
//! `Java_<package>_<Class>_<method>`, so the Java-facing shims live here
//! and forward every call to the existing `pith_unicode_*` C export —
//! same status codes, same refusals, no second implementation of the
//! normalizer. The module is compiled out of the unit-test build
//! (`#[cfg(not(test))]` at the registration site in `lib.rs`); the
//! integration tests in `tests/java_ffi.rs` exercise the export against
//! a synthetic environment so the coverage gate still sees the glue.
//!
//! JNI conventions of this module (the Java-side contract):
//!
//! * the export takes the JNI environment first and the receiving
//!   class second (the methods are static), then the Java arguments;
//! * the status code crosses back through a trailing one-element
//!   `int[]` — the same `PITH_OK` / `PITH_E_INVALID` / `PITH_E_REJECTED`
//!   values the C ABI returns;
//! * a normalization result crosses back as a fresh `jbyteArray` (a
//!   null return whenever the status is not `PITH_OK`); the C export's
//!   handed-out buffer is copied into the Java array and released
//!   through [`pith_unicode_free`] before returning;
//! * the quick-check answer crosses back as a `jint` (0/1, 0 unless
//!   the status is `PITH_OK`);
//! * a null `data` array maps to `PITH_E_INVALID` exactly as the C ABI
//!   maps a null pointer; a null environment or status array
//!   short-circuits to a null/zero return without touching memory (both
//!   are unreachable through the Java wrapper, which always passes live
//!   arrays from a live JVM).
//!
//! The suite is zero-third-party (CI's `check-zero-deps.py` fails any
//! registry crate), so the JNI function table is hand-declared below:
//! every slot is pointer-sized and the positions are the fixed
//! `JNINativeInterface_` member order of `jni.h`. The slot indices were
//! parsed mechanically from a JDK `include/jni.h` and are validated
//! end-to-end against a live JVM every time the Java suite runs.

#![allow(unsafe_code)]
// The JNI typedefs keep the jni.h spelling (jint, jbyte, ...).
#![allow(non_camel_case_types)]

use core::ffi::c_void;

use crate::ffi::{
    PITH_E_INVALID, PITH_OK, pith_unicode_casefold, pith_unicode_casefold_simple,
    pith_unicode_free, pith_unicode_is_normalized, pith_unicode_nfc, pith_unicode_nfd,
    pith_unicode_nfkc, pith_unicode_nfkd,
};

/// A JNI environment handle — C-mode `JNIEnv*`, a pointer to the
/// function table.
type JNIEnv = *const JniTable;

/// Any Java array reference; the glue only checks nullness before
/// handing arrays through the table.
type JArray = *mut c_void;

/// A Java `int[]` reference.
type JIntArray = *mut c_void;

/// A Java class object reference (static methods receive the class).
type JClass = *mut c_void;

/// `jbyte` per `jni.h`.
type jbyte = i8;
/// `jint`/`jsize` per `jni.h`.
type jint = i32;

/// The JNI function-table slots this module calls.
///
/// Underscore-prefixed gap fields hold the slots between the used ones
/// (slot = field position; the four reserved pointers are part of the
/// prefix). Slot indices parsed mechanically from a JDK
/// `include/jni.h`: `GetArrayLength` = 171, `NewByteArray` = 176,
/// `GetByteArrayRegion` = 200, `SetByteArrayRegion` = 208,
/// `SetIntArrayRegion` = 211.
#[repr(C)]
struct JniTable {
    /// Slots 0..=170: the four reserved pointers through
    /// `ReleaseStringUTFChars`.
    _prefix: [*mut c_void; 171],
    /// Slot 171.
    get_array_length: unsafe extern "system" fn(env: *mut JNIEnv, array: JArray) -> jint,
    /// Slots 172..=175.
    _gap_before_new_byte_array: [*mut c_void; 4],
    /// Slot 176.
    new_byte_array: unsafe extern "system" fn(env: *mut JNIEnv, len: jint) -> JArray,
    /// Slots 177..=199.
    _gap_before_byte_region: [*mut c_void; 23],
    /// Slot 200.
    get_byte_array_region: unsafe extern "system" fn(
        env: *mut JNIEnv,
        array: JArray,
        start: jint,
        len: jint,
        buf: *mut jbyte,
    ),
    /// Slots 201..=207.
    _gap_before_set_byte_region: [*mut c_void; 7],
    /// Slot 208.
    set_byte_array_region: unsafe extern "system" fn(
        env: *mut JNIEnv,
        array: JArray,
        start: jint,
        len: jint,
        buf: *const jbyte,
    ),
    /// Slots 209..=210.
    _gap_before_int_region: [*mut c_void; 2],
    /// Slot 211.
    set_int_array_region: unsafe extern "system" fn(
        env: *mut JNIEnv,
        array: JIntArray,
        start: jint,
        len: jint,
        buf: *const jint,
    ),
}

/// The JNI 1.1-era function table behind an environment handle.
///
/// # Safety
///
/// `env` must be a live JNI environment pointer.
unsafe fn table<'a>(env: *mut JNIEnv) -> &'a JniTable {
    // `env` points at the function-table pointer (C-mode `JNIEnv*`):
    // deref twice to reach the table itself.
    unsafe { &**env }
}

/// Copies a Java `byte[]` through the environment into an owned
/// buffer.
///
/// # Safety
///
/// `env` must be a live JNI environment and `array` a live `byte[]`
/// reference for the duration of the call; a null array is
/// [`PITH_E_INVALID`], mirroring the C ABI's null-pointer rule.
unsafe fn java_bytes(env: *mut JNIEnv, array: JArray) -> Result<Vec<u8>, i32> {
    if array.is_null() {
        return Err(PITH_E_INVALID);
    }
    let functions = unsafe { table(env) };
    let len = unsafe { (functions.get_array_length)(env, array) };
    if len < 0 {
        return Err(PITH_E_INVALID);
    }
    let mut bytes = vec![0u8; len as usize];
    unsafe { (functions.get_byte_array_region)(env, array, 0, len, bytes.as_mut_ptr().cast()) };
    Ok(bytes)
}

/// Writes `value` into the one-element `int[]` status slot.
///
/// # Safety
///
/// `status` must be a live `int[]` of length ≥ 1 (checked by the
/// caller).
unsafe fn set_status(env: *mut JNIEnv, status: JIntArray, value: jint) {
    let functions = unsafe { table(env) };
    unsafe { (functions.set_int_array_region)(env, status, 0, 1, &value) };
}

/// Wraps `bytes` in a fresh Java `byte[]` through the environment.
///
/// # Safety
///
/// `env` must be a live JNI environment.
unsafe fn java_byte_array(env: *mut JNIEnv, bytes: &[u8]) -> JArray {
    let functions = unsafe { table(env) };
    let array = unsafe { (functions.new_byte_array)(env, bytes.len() as jint) };
    if array.is_null() {
        // The JVM raised OutOfMemoryError; a null return carries it.
        return std::ptr::null_mut();
    }
    unsafe {
        (functions.set_byte_array_region)(
            env,
            array,
            0,
            bytes.len() as jint,
            bytes.as_ptr().cast(),
        );
    }
    array
}

/// The shared body of the six buffer-handed-out exports: copy the Java
/// input in, run the C export, copy the handed-out buffer into a fresh
/// Java array, release the buffer, and report the status.
///
/// # Safety
///
/// `env` must be a live JNI environment and `data`/`status` live Java
/// array references for the duration of the call.
unsafe fn buffer_export(
    env: *mut JNIEnv,
    data: JArray,
    status: JIntArray,
    op: unsafe extern "C" fn(*const u8, usize, *mut *mut u8, *mut usize) -> i32,
) -> JArray {
    if env.is_null() || status.is_null() {
        return std::ptr::null_mut();
    }
    let bytes = match unsafe { java_bytes(env, data) } {
        Ok(bytes) => bytes,
        Err(status_code) => {
            unsafe { set_status(env, status, status_code) };
            return std::ptr::null_mut();
        }
    };
    let mut out: *mut u8 = core::ptr::null_mut();
    let mut out_len: usize = 0;
    let code = unsafe { op(bytes.as_ptr(), bytes.len(), &mut out, &mut out_len) };
    if code != PITH_OK {
        unsafe { set_status(env, status, code) };
        return std::ptr::null_mut();
    }
    // The handed-out buffer is a boxed slice owned by this call; copy
    // it into the Java array and release it before returning.
    let slice = unsafe { core::slice::from_raw_parts(out, out_len) };
    let result = unsafe { java_byte_array(env, slice) };
    unsafe { pith_unicode_free(out, out_len) };
    unsafe { set_status(env, status, PITH_OK) };
    result
}

macro_rules! jni_buffer_export {
    ($name:ident, $op:expr, $doc:expr) => {
        #[doc = $doc]
        ///
        /// # Safety
        ///
        /// `env` must be a live JNI environment and `data`/`status` live
        /// Java array references for the duration of the call.
        //
        // Private: the JVM links the export by symbol name; a public
        // Rust signature over the private table type would trip
        // `private_interfaces`.
        #[unsafe(no_mangle)]
        unsafe extern "system" fn $name(
            env: *mut JNIEnv,
            _class: JClass,
            data: JArray,
            status: JIntArray,
        ) -> JArray {
            unsafe { buffer_export(env, data, status, $op) }
        }
    };
}

jni_buffer_export!(
    Java_hash_pith_unicode_PithUnicode_nfcNative,
    pith_unicode_nfc,
    "The Java binding of [`pith_unicode_nfc`]: NFC over a Java `byte[]`."
);
jni_buffer_export!(
    Java_hash_pith_unicode_PithUnicode_nfdNative,
    pith_unicode_nfd,
    "The Java binding of [`pith_unicode_nfd`]: NFD over a Java `byte[]`."
);
jni_buffer_export!(
    Java_hash_pith_unicode_PithUnicode_nfkcNative,
    pith_unicode_nfkc,
    "The Java binding of [`pith_unicode_nfkc`]: NFKC over a Java `byte[]`."
);
jni_buffer_export!(
    Java_hash_pith_unicode_PithUnicode_nfkdNative,
    pith_unicode_nfkd,
    "The Java binding of [`pith_unicode_nfkd`]: NFKD over a Java `byte[]`."
);
jni_buffer_export!(
    Java_hash_pith_unicode_PithUnicode_casefoldNative,
    pith_unicode_casefold,
    "The Java binding of [`pith_unicode_casefold`]: full case folding over a Java `byte[]`."
);
jni_buffer_export!(
    Java_hash_pith_unicode_PithUnicode_casefoldSimpleNative,
    pith_unicode_casefold_simple,
    "The Java binding of [`pith_unicode_casefold_simple`]: simple case folding over a Java `byte[]`."
);

/// The Java binding of [`pith_unicode_is_normalized`]: the exact UAX #15
/// quick-check answer for `form` — 1 = NFC, 2 = NFD, 3 = NFKC, 4 =
/// NFKD. The answer crosses back as a `jint` (0/1, 0 unless the status
/// is [`PITH_OK`]); the status (`PITH_E_INVALID` for a null array or an
/// out-of-range form, `PITH_E_REJECTED` for bytes that are not valid
/// UTF-8) through `status[0]`.
///
/// # Safety
///
/// `env` must be a live JNI environment and `data`/`status` live Java
/// array references for the duration of the call.
//
// Private: the JVM links the export by symbol name; a public Rust
// signature over the private table type would trip
// `private_interfaces`.
#[unsafe(no_mangle)]
unsafe extern "system" fn Java_hash_pith_unicode_PithUnicode_isNormalizedNative(
    env: *mut JNIEnv,
    _class: JClass,
    form: jint,
    data: JArray,
    status: JIntArray,
) -> jint {
    if env.is_null() || status.is_null() {
        return 0;
    }
    let bytes = match unsafe { java_bytes(env, data) } {
        Ok(bytes) => bytes,
        Err(status_code) => {
            unsafe { set_status(env, status, status_code) };
            return 0;
        }
    };
    let mut answer: u8 = 0;
    let code = unsafe {
        pith_unicode_is_normalized(
            form as u32,
            bytes.as_ptr(),
            bytes.len(),
            core::ptr::addr_of_mut!(answer),
        )
    };
    unsafe { set_status(env, status, code) };
    if code == PITH_OK { answer as jint } else { 0 }
}
