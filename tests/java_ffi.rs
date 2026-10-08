//! Fake-JNI-environment coverage for the glue in `src/ffi_jni.rs`.
//!
//! `src/ffi_jni.rs` is compiled out of the unit-test build (the
//! `#[unsafe(no_mangle)]` exports would collide with the unit-test
//! binary), so this integration test drives every export through an
//! `unsafe extern` declaration against a synthetic environment: a
//! zeroed function table whose slots the glue calls carry test-local
//! implementations backed by in-test buffers. The real-JVM proof is
//! the Java suite (`sdk/java`, `mvn test` against the built cdylib);
//! this file keeps the glue executed and visible to the coverage gate
//! with zero new dependencies (the suite's `check-zero-deps.py` gate
//! forbids registry crates, so the plain `std` mutexes stay unwrapped
//! here).

#![allow(unsafe_code)]
// The JNI typedefs keep the jni.h spelling.
#![allow(non_camel_case_types)]

use core::ffi::c_void;
use pith_unicode::ffi::{PITH_E_INVALID, PITH_E_REJECTED, pith_unicode_nfc};
use std::sync::{LazyLock, Mutex};

type JNIEnv = *const FakeTable;
type JArray = *mut c_void;
type JIntArray = *mut c_void;
type JClass = *mut c_void;
type jbyte = i8;
type jint = i32;

/// Mirror of `src/ffi_jni.rs`'s function table — the same slot
/// positions (`GetArrayLength` = 171, `NewByteArray` = 176,
/// `GetByteArrayRegion` = 200, `SetByteArrayRegion` = 208,
/// `SetIntArrayRegion` = 211, four reserved pointers in the prefix).
#[repr(C)]
struct FakeTable {
    /// Slots 0..=170.
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

// The exported symbols under test (linked from the crate's rlib).
unsafe extern "system" {
    fn Java_hash_pith_unicode_PithUnicode_nfcNative(
        env: *mut JNIEnv,
        class: JClass,
        data: JArray,
        status: JIntArray,
    ) -> JArray;
    fn Java_hash_pith_unicode_PithUnicode_nfdNative(
        env: *mut JNIEnv,
        class: JClass,
        data: JArray,
        status: JIntArray,
    ) -> JArray;
    fn Java_hash_pith_unicode_PithUnicode_nfkcNative(
        env: *mut JNIEnv,
        class: JClass,
        data: JArray,
        status: JIntArray,
    ) -> JArray;
    fn Java_hash_pith_unicode_PithUnicode_nfkdNative(
        env: *mut JNIEnv,
        class: JClass,
        data: JArray,
        status: JIntArray,
    ) -> JArray;
    fn Java_hash_pith_unicode_PithUnicode_casefoldNative(
        env: *mut JNIEnv,
        class: JClass,
        data: JArray,
        status: JIntArray,
    ) -> JArray;
    fn Java_hash_pith_unicode_PithUnicode_casefoldSimpleNative(
        env: *mut JNIEnv,
        class: JClass,
        data: JArray,
        status: JIntArray,
    ) -> JArray;
    fn Java_hash_pith_unicode_PithUnicode_isNormalizedNative(
        env: *mut JNIEnv,
        class: JClass,
        form: jint,
        data: JArray,
        status: JIntArray,
    ) -> jint;
}

/// The state of one native call under test.
struct FakeCall {
    input: Vec<u8>,
    negative_length: bool,
    /// The `byte[]` the glue allocated through `NewByteArray`, with the
    /// bytes `SetByteArrayRegion` wrote into it.
    out: Vec<u8>,
    out_written: bool,
    out_ints: Vec<i32>,
}

static CALL: LazyLock<Mutex<Option<FakeCall>>> = LazyLock::new(|| Mutex::new(None));
static SERIAL: Mutex<()> = Mutex::new(());

/// `GetArrayLength` (slot 171): the current input's length.
unsafe extern "system" fn fake_get_array_length(_env: *mut JNIEnv, _array: JArray) -> jint {
    let guard = CALL.lock().unwrap();
    let current = guard.as_ref().expect("no fake call state installed");
    if current.negative_length {
        -1
    } else {
        current.input.len() as jint
    }
}

/// `NewByteArray` (slot 176): allocates the test-side output buffer and
/// hands back a pointer into it, exactly as a JVM hands back a live
/// array reference.
unsafe extern "system" fn fake_new_byte_array(_env: *mut JNIEnv, len: jint) -> JArray {
    let mut guard = CALL.lock().unwrap();
    let current = guard.as_mut().expect("no fake call state installed");
    current.out = vec![0u8; len as usize];
    let ptr = current.out.as_mut_ptr();
    ptr.cast()
}

/// `GetByteArrayRegion` (slot 200): copies the current input bytes.
unsafe extern "system" fn fake_get_byte_array_region(
    _env: *mut JNIEnv,
    _array: JArray,
    start: jint,
    len: jint,
    buf: *mut jbyte,
) {
    let guard = CALL.lock().unwrap();
    let current = guard.as_ref().expect("no fake call state installed");
    let start = start as usize;
    let bytes = &current.input[start..start + len as usize];
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf.cast(), bytes.len()) };
}

/// `SetByteArrayRegion` (slot 208): copies into the allocated output —
/// faithful to the JVM — and marks the write.
unsafe extern "system" fn fake_set_byte_array_region(
    _env: *mut JNIEnv,
    array: JArray,
    start: jint,
    len: jint,
    buf: *const jbyte,
) {
    let mut guard = CALL.lock().unwrap();
    let current = guard.as_mut().expect("no fake call state installed");
    let start = start as usize;
    unsafe {
        std::ptr::copy_nonoverlapping(
            buf.cast(),
            current.out.as_mut_ptr().add(start),
            len as usize,
        )
    };
    current.out_written = true;
    // Keep the pointer `array` honest for assertions on nullness only.
    let _ = array;
}

/// `SetIntArrayRegion` (slot 211): copies into the target array —
/// faithful to the JVM — and records the written values.
unsafe extern "system" fn fake_set_int_array_region(
    _env: *mut JNIEnv,
    array: JIntArray,
    _start: jint,
    len: jint,
    buf: *const jint,
) {
    unsafe { std::ptr::copy_nonoverlapping(buf, array as *mut jint, len as usize) };
    let mut guard = CALL.lock().unwrap();
    let current = guard.as_mut().expect("no fake call state installed");
    for i in 0..len as usize {
        current.out_ints.push(unsafe { *buf.add(i) });
    }
}

/// A zero-initialized `FakeTable`, leaked.
///
/// Raw `alloc_zeroed` bytes rather than `mem::zeroed`: the latter
/// runtime-refuses zeroed fn-pointer fields, while the former is just
/// memory — every slot the glue calls is assigned below before use.
fn zeroed_table() -> *mut FakeTable {
    let raw = unsafe { std::alloc::alloc_zeroed(std::alloc::Layout::new::<FakeTable>()) };
    assert!(!raw.is_null(), "alloc_zeroed failed");
    raw.cast::<FakeTable>()
}

/// Runs `f` against a synthetic environment backed by `input`,
/// returning `(result, status, java_out)` — the export's return value,
/// the status slot the glue wrote, and the bytes the glue produced
/// through `NewByteArray`/`SetByteArrayRegion` (`None` when no array
/// was allocated).
///
/// The big lock serializes sections across test threads: the fake
/// table callbacks address this one global call state.
fn with_fake_env<T>(
    input: Vec<u8>,
    negative_length: bool,
    f: impl FnOnce(*mut JNIEnv) -> T,
) -> (T, i32, Option<Vec<u8>>) {
    let _serial = SERIAL.lock().unwrap();
    *CALL.lock().unwrap() = Some(FakeCall {
        input,
        negative_length,
        out: Vec::new(),
        out_written: false,
        out_ints: Vec::new(),
    });

    let table = zeroed_table();
    unsafe {
        (*table).get_array_length = fake_get_array_length;
        (*table).new_byte_array = fake_new_byte_array;
        (*table).get_byte_array_region = fake_get_byte_array_region;
        (*table).set_byte_array_region = fake_set_byte_array_region;
        (*table).set_int_array_region = fake_set_int_array_region;
    }
    let functions: *const FakeTable = table;
    let env: *mut JNIEnv = Box::into_raw(Box::new(functions));

    let result = f(env);
    let state = CALL.lock().unwrap().take().expect("fake call state");
    let out = if state.out_written {
        Some(state.out)
    } else {
        None
    };
    let status = state.out_ints.first().copied().unwrap_or(i32::MIN);
    (result, status, out)
}

/// A live one-element status array; its content is read back through
/// the fake `SetIntArrayRegion` bookkeeping in [`with_fake_env`].
fn status_slot() -> JIntArray {
    Box::into_raw(Box::new([i32::MIN; 1])) as JIntArray
}

/// A non-null opaque array handle (the glue only checks nullness).
const SOME_ARRAY: JArray = 1usize as JArray;

/// [`with_fake_env`] with the negative-length fake installed.
fn with_fake_env_neg_len<T>(
    input: Vec<u8>,
    f: impl FnOnce(*mut JNIEnv) -> T,
) -> (T, i32, Option<Vec<u8>>) {
    with_fake_env(input, true, f)
}

/// The JNI statuses mirror the C ABI's refusal codes exactly
/// (PITH_E_INVALID, PITH_E_REJECTED). Calling the C ABI entry here is
/// the parity cross-check - and it also puts the rlib on this binary's
/// link line, which is what lets the bare JNI declarations resolve on
/// targets whose archive search only sees command-line archives (MSVC).
#[test]
fn jni_statuses_mirror_the_c_abi() {
    let mut out: *mut u8 = core::ptr::null_mut();
    let mut out_len: usize = 0;
    let status = unsafe { pith_unicode_nfc(core::ptr::null(), 0, &mut out, &mut out_len) };
    assert_eq!(status, PITH_E_INVALID, "null pointer status");
    let status = unsafe { pith_unicode_nfc(b"\xff\xfe".as_ptr(), 2, &mut out, &mut out_len) };
    assert_eq!(status, PITH_E_REJECTED, "invalid utf-8 status");
}

/// reference.json vectors[0]: U+1E0A - NFC is the identity, NFD
/// decomposes to D + U+0307.
#[test]
fn jni_buffer_exports_reproduce_the_pinned_vector() {
    for (export, input, want) in [
        ("nfc", vec![0xe1, 0xb8, 0x8a], vec![0xe1, 0xb8, 0x8a]),
        ("nfd", vec![0xe1, 0xb8, 0x8a], vec![0x44, 0xcc, 0x87]),
    ] {
        let (out, status, java_out) = with_fake_env(input.clone(), false, |env| unsafe {
            match export {
                "nfc" => Java_hash_pith_unicode_PithUnicode_nfcNative(
                    env,
                    std::ptr::null_mut(),
                    SOME_ARRAY,
                    status_slot(),
                ),
                _ => Java_hash_pith_unicode_PithUnicode_nfdNative(
                    env,
                    std::ptr::null_mut(),
                    SOME_ARRAY,
                    status_slot(),
                ),
            }
        });
        assert_eq!(status, 0, "status {export}");
        assert_eq!(java_out.expect("array allocated"), want, "bytes {export}");
        assert!(!out.is_null(), "non-null array reference {export}");
    }
}

/// The compatibility and folding exports on pinned cases: the ligature
/// expands, the sharp-s family folds as pinned.
#[test]
fn jni_tier1_exports_fold_pinned_inputs() {
    for (name, input, want) in [
        ("nfkc", vec![0xef, 0xac, 0x81], b"fi".to_vec()),
        ("nfkd", vec![0xef, 0xac, 0x81], b"fi".to_vec()),
        ("casefold", vec![0xc3, 0x9f], b"ss".to_vec()),
        ("casefoldSimple", vec![0xe1, 0xba, 0x9e], vec![0xc3, 0x9f]),
    ] {
        let (out, status, java_out) = with_fake_env(input, false, |env| unsafe {
            match name {
                "nfkc" => Java_hash_pith_unicode_PithUnicode_nfkcNative(
                    env,
                    std::ptr::null_mut(),
                    SOME_ARRAY,
                    status_slot(),
                ),
                "nfkd" => Java_hash_pith_unicode_PithUnicode_nfkdNative(
                    env,
                    std::ptr::null_mut(),
                    SOME_ARRAY,
                    status_slot(),
                ),
                "casefold" => Java_hash_pith_unicode_PithUnicode_casefoldNative(
                    env,
                    std::ptr::null_mut(),
                    SOME_ARRAY,
                    status_slot(),
                ),
                _ => Java_hash_pith_unicode_PithUnicode_casefoldSimpleNative(
                    env,
                    std::ptr::null_mut(),
                    SOME_ARRAY,
                    status_slot(),
                ),
            }
        });
        assert_eq!(status, 0, "status {name}");
        assert_eq!(java_out.expect("array allocated"), want, "bytes {name}");
        assert!(!out.is_null(), "non-null array reference {name}");
    }
}

/// The quick-check export answers exactly, and every refusal path is a
/// status - never a crash.
#[test]
fn jni_is_normalized_answers_and_refuses() {
    let ligature = vec![0xef, 0xac, 0x81];
    for (form, want) in [(1i32, 1i32), (2, 1), (3, 0), (4, 0)] {
        let (answer, status, _) = with_fake_env(ligature.clone(), false, |env| unsafe {
            Java_hash_pith_unicode_PithUnicode_isNormalizedNative(
                env,
                std::ptr::null_mut(),
                form,
                SOME_ARRAY,
                status_slot(),
            )
        });
        assert_eq!(status, 0, "status form {form}");
        assert_eq!(answer, want, "answer form {form}");
    }
    let (answer, status, _) = with_fake_env(ligature.clone(), false, |env| unsafe {
        Java_hash_pith_unicode_PithUnicode_isNormalizedNative(
            env,
            std::ptr::null_mut(),
            0,
            SOME_ARRAY,
            status_slot(),
        )
    });
    assert_eq!(answer, 0, "form 0 answer");
    assert_eq!(status, -1, "form 0 status");
    let (answer, status, _) = with_fake_env(vec![0xff, 0xfe], false, |env| unsafe {
        Java_hash_pith_unicode_PithUnicode_isNormalizedNative(
            env,
            std::ptr::null_mut(),
            2,
            SOME_ARRAY,
            status_slot(),
        )
    });
    assert_eq!(answer, 0, "invalid utf-8 answer");
    assert_eq!(status, -2, "invalid utf-8 status");
}

/// Null data arrays and negative lengths map to PITH_E_INVALID exactly
/// as the C ABI maps a null pointer.
#[test]
fn jni_null_data_and_negative_length_are_invalid() {
    for name in ["nfc", "nfkd", "casefoldSimple"] {
        let (out, status, java_out) = with_fake_env(vec![0x61], false, |env| unsafe {
            match name {
                "nfc" => Java_hash_pith_unicode_PithUnicode_nfcNative(
                    env,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    status_slot(),
                ),
                "nfkd" => Java_hash_pith_unicode_PithUnicode_nfkdNative(
                    env,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    status_slot(),
                ),
                _ => Java_hash_pith_unicode_PithUnicode_casefoldSimpleNative(
                    env,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    status_slot(),
                ),
            }
        });
        assert_eq!(status, -1, "null data status {name}");
        assert!(out.is_null(), "null data array {name}");
        assert!(java_out.is_none(), "no array allocated {name}");

        // A negative GetArrayLength answer is refused too.
        let (out, status, _) = with_fake_env_neg_len(vec![0x61], |env| unsafe {
            match name {
                "nfc" => Java_hash_pith_unicode_PithUnicode_nfcNative(
                    env,
                    std::ptr::null_mut(),
                    SOME_ARRAY,
                    status_slot(),
                ),
                "nfkd" => Java_hash_pith_unicode_PithUnicode_nfkdNative(
                    env,
                    std::ptr::null_mut(),
                    SOME_ARRAY,
                    status_slot(),
                ),
                _ => Java_hash_pith_unicode_PithUnicode_casefoldSimpleNative(
                    env,
                    std::ptr::null_mut(),
                    SOME_ARRAY,
                    status_slot(),
                ),
            }
        });
        assert_eq!(status, -1, "negative length status {name}");
        assert!(out.is_null(), "negative length array {name}");
    }
}

/// Invalid UTF-8 through a buffer export is PITH_E_REJECTED.
#[test]
fn jni_invalid_utf8_is_rejected() {
    let (out, status, java_out) = with_fake_env(vec![0xff, 0xfe], false, |env| unsafe {
        Java_hash_pith_unicode_PithUnicode_nfcNative(
            env,
            std::ptr::null_mut(),
            SOME_ARRAY,
            status_slot(),
        )
    });
    assert_eq!(status, -2, "status");
    assert!(out.is_null(), "null array");
    assert!(java_out.is_none(), "no array allocated");
}

/// A null status array short-circuits to a null return without
/// touching memory; a null environment does the same and leaves the
/// status slot untouched.
#[test]
fn jni_null_status_or_environment_short_circuits() {
    let (out, _, _) = with_fake_env(vec![0x61], false, |env| unsafe {
        Java_hash_pith_unicode_PithUnicode_nfcNative(
            env,
            std::ptr::null_mut(),
            SOME_ARRAY,
            std::ptr::null_mut(),
        )
    });
    assert!(out.is_null(), "null status array");

    let (out, status, _) = with_fake_env(vec![0x61], false, |_env| unsafe {
        Java_hash_pith_unicode_PithUnicode_nfcNative(
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            SOME_ARRAY,
            status_slot(),
        )
    });
    assert!(out.is_null(), "null environment array");
    assert_eq!(status, i32::MIN, "status untouched without an environment");

    let (answer, _, _) = with_fake_env(vec![0x61], false, |_env| unsafe {
        Java_hash_pith_unicode_PithUnicode_isNormalizedNative(
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            1,
            SOME_ARRAY,
            status_slot(),
        )
    });
    assert_eq!(answer, 0, "null environment answer");
}
