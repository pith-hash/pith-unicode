// SPDX-License-Identifier: MIT
// Copyright (c) 2026 pith-hash
"use strict";

/**
 * pith-unicode SDK: Unicode NFC/NFD normalisation through koffi.
 *
 * Every function validates its input as UTF-8 inside the Rust core and
 * returns a fresh Buffer — the handed-out cdylib buffer is copied and
 * released before returning. Invalid UTF-8 throws an `FfiError` with
 * `status === -2`; it is never a crash.
 */

const koffi = require("koffi");
const fs = require("node:fs");
const path = require("node:path");

const STATUS_OK = 0;
const STATUS_INVALID = -1;
const STATUS_REJECTED = -2;

/** Every cdylib file name cargo may drop into the build directory, per platform. */
const CDYLIB_NAMES = ["pith_unicode.dll", "libpith_unicode.so", "libpith_unicode.dylib"];

const PKG_ROOT = path.join(__dirname);
const REPO_ROOT = path.resolve(__dirname, "..", "..");

/** Backs the data pointer for empty inputs: the FFI contract treats a
 * NULL data pointer as a caller bug (status -1), so an empty Buffer
 * must hand over a valid non-null pointer that reads as zero bytes. */
const EMPTY_SCRATCH = Buffer.alloc(1);

/** FfiError: a non-zero status code came back from the cdylib. */
class FfiError extends Error {
  /**
   * @param {string} op the FFI operation name
   * @param {number} status the raw status code
   */
  constructor(op, status) {
    const kind = { [STATUS_INVALID]: "invalid argument", [STATUS_REJECTED]: "input rejected" }[status] ?? "unknown failure";
    super(`${op} failed: ${kind} (status ${status})`);
    this.name = "FfiError";
    /** The raw status code the FFI returned. */
    this.status = status;
  }
}

/**
 * Locates the cdylib through the suite's discovery chain.
 * @returns {string} an absolute path to the cdylib file
 * @throws {Error} when nothing is found
 */
function findCdylib() {
  const explicit = process.env.PITH_CDYLIB;
  if (explicit && fs.statSync(explicit, { throwIfNoEntry: false })?.isFile()) {
    return path.resolve(explicit);
  }
  /** @type {string[]} */
  const dirs = [];
  const envDir = process.env.PITH_CDYLIB_DIR;
  if (envDir) {
    dirs.push(envDir);
    if (!path.isAbsolute(envDir)) {
      dirs.push(path.join(REPO_ROOT, envDir));
    }
  }
  const osArch = `${process.platform}-${process.arch}`;
  dirs.push(path.join(PKG_ROOT, "prebuilds", osArch));
  dirs.push(path.join(PKG_ROOT, "prebuilds"));
  dirs.push(path.join(REPO_ROOT, "target", "release"));
  for (const dir of dirs) {
    for (const name of CDYLIB_NAMES) {
      const p = path.join(dir, name);
      if (fs.statSync(p, { throwIfNoEntry: false })?.isFile()) return p;
    }
  }
  throw new Error(
    "no pith-unicode cdylib found (searched PITH_CDYLIB, PITH_CDYLIB_DIR, prebuilds/ and <repo>/target/release); " +
      "run `cargo build --release` first",
  );
}

let cached = undefined;

/**
 * Loads the cdylib and binds the exported symbols (lazily, once).
 * Exported so the null-pointer refusal tests can drive the raw
 * binding with a null data pointer.
 *
 * @returns {{nfc: Function, nfd: Function, free: Function}}
 */
function loadLibrary() {
  if (cached) return cached;
  const lib = koffi.load(findCdylib());
  // One shared prototype shape: nfc and nfd have identical C signatures.
  const bind = (name) =>
    lib.func(name, "int32_t", [
      "const uint8_t *",
      "size_t",
      koffi.out(koffi.pointer("void *")),
      koffi.out(koffi.pointer("size_t")),
    ]);
  const nfc = bind("pith_unicode_nfc");
  const nfd = bind("pith_unicode_nfd");
  const free = lib.func("void pith_unicode_free(void *ptr, size_t len)");
  cached = { nfc, nfd, free };
  return cached;
}

/**
 * Runs one normalisation op and copies the handed-out cdylib buffer
 * into a fresh Buffer before releasing it.
 *
 * @param {string} opName the FFI operation name (for errors)
 * @param {Buffer} data the UTF-8 bytes to normalize
 * @returns {Buffer} the normalized UTF-8 bytes
 * @throws {FfiError} with `status === -2` for bytes that are not valid UTF-8
 */
function normalize(opName, data) {
  if (!Buffer.isBuffer(data)) {
    throw new TypeError("data must be a Buffer");
  }
  const lib = loadLibrary();
  const op = opName === "pith_unicode_nfc" ? lib.nfc : lib.nfd;
  const out = [null];
  const outLen = [0];
  // koffi hands a NULL pointer for a zero-length Buffer, but the FFI
  // contract treats NULL as a caller bug (status -1); empty input must
  // read as zero bytes off a valid non-null pointer instead.
  const ptr = data.length > 0 ? data : EMPTY_SCRATCH;
  const status = op(ptr, data.length, out, outLen);
  if (status !== STATUS_OK) {
    throw new FfiError(opName, status);
  }
  try {
    // koffi.decode hands back a Uint8Array view over the external
    // buffer; copy it into a Buffer before the cdylib buffer is freed.
    return Buffer.from(koffi.decode(out[0], "uint8_t", Number(outLen[0])));
  } finally {
    lib.free(out[0], Number(outLen[0]));
  }
}

/**
 * Normalizes `data` to Unicode Normalization Form C (canonical
 * composition) and returns the UTF-8 result as a fresh Buffer.
 *
 * @param {Buffer} data the UTF-8 bytes to normalize
 * @returns {Buffer} the NFC-normalized bytes (empty in → empty out)
 * @throws {FfiError} with `status === -2` for invalid UTF-8
 */
function nfc(data) {
  return normalize("pith_unicode_nfc", data);
}

/**
 * Normalizes `data` to Unicode Normalization Form D (canonical
 * decomposition), with the same contract as `nfc`.
 *
 * @param {Buffer} data the UTF-8 bytes to normalize
 * @returns {Buffer} the NFD-normalized bytes
 * @throws {FfiError} with `status === -2` for invalid UTF-8
 */
function nfd(data) {
  return normalize("pith_unicode_nfd", data);
}

module.exports = {
  STATUS_OK,
  STATUS_INVALID,
  STATUS_REJECTED,
  CDYLIB_NAMES,
  FfiError,
  findCdylib,
  loadLibrary,
  nfc,
  nfd,
};
