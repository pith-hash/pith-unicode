// SPDX-License-Identifier: MIT
// Copyright (c) 2026 pith-hash
"use strict";

// Hex-exact conformance: the committed reference vectors through koffi.
// Every vector in the repository-root reference.json is replayed through
// the cdylib and compared byte-exact — the historical NFC/NFD rows and
// the tier-1 form_vectors (all four forms) and casefold_vectors (full +
// simple folding) sections. The same vectors the Rust gen-reference
// verify gate and the Python/Go SDKs check.

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const {
  FfiError,
  FORM_NFKC,
  FORM_NFKD,
  NORMALIZATION_FORMS,
  loadLibrary,
  nfc,
  nfd,
  nfkc,
  nfkd,
  normalize,
  isNormalized,
  casefold,
  casefoldSimple,
} = require("../index.js");

const REPO_ROOT = path.resolve(__dirname, "..", "..", "..");

const REFERENCE = JSON.parse(fs.readFileSync(path.join(REPO_ROOT, "reference.json"), "utf8"));
const VECTORS = REFERENCE.vectors;
const FORM_VECTORS = REFERENCE.form_vectors;
const CASEFOLD_VECTORS = REFERENCE.casefold_vectors;

test("cdylib is discoverable", () => {
  const { findCdylib } = require("../index.js");
  assert.ok(fs.statSync(findCdylib()).isFile());
});

for (const [idx, vector] of VECTORS.entries()) {
  test(`reference vector ${idx} is reproduced hex-exact`, () => {
    const [inputHex, nfcHex, nfdHex] = vector;
    const data = Buffer.from(inputHex, "hex");
    assert.deepEqual(nfc(data), Buffer.from(nfcHex, "hex"), `vector ${idx} nfc`);
    assert.deepEqual(nfd(data), Buffer.from(nfdHex, "hex"), `vector ${idx} nfd`);
  });
}

for (const [idx, vector] of FORM_VECTORS.entries()) {
  test(`form vector ${idx} is reproduced hex-exact`, () => {
    const [inputHex, nfcHex, nfdHex, nfkcHex, nfkdHex] = vector;
    const data = Buffer.from(inputHex, "hex");
    assert.deepEqual(nfc(data), Buffer.from(nfcHex, "hex"), `form vector ${idx} nfc`);
    assert.deepEqual(nfd(data), Buffer.from(nfdHex, "hex"), `form vector ${idx} nfd`);
    assert.deepEqual(nfkc(data), Buffer.from(nfkcHex, "hex"), `form vector ${idx} nfkc`);
    assert.deepEqual(nfkd(data), Buffer.from(nfkdHex, "hex"), `form vector ${idx} nfkd`);
    // The form dispatch answers byte-identically through every spelling.
    assert.deepEqual(normalize("NFKC", data), Buffer.from(nfkcHex, "hex"));
    assert.deepEqual(normalize("nfkd", data), Buffer.from(nfkdHex, "hex"));
    assert.deepEqual(normalize(FORM_NFKC, data), Buffer.from(nfkcHex, "hex"));
    // Quick-check detection is the exact slow-path truth.
    for (const [form, outHex] of [
      [1, nfcHex],
      [2, nfdHex],
      [3, nfkcHex],
      [4, nfkdHex],
    ]) {
      assert.equal(
        isNormalized(form, data),
        data.equals(Buffer.from(outHex, "hex")),
        `form vector ${idx} isNormalized ${form}`,
      );
    }
  });
}

for (const [idx, vector] of CASEFOLD_VECTORS.entries()) {
  test(`casefold vector ${idx} is reproduced hex-exact`, () => {
    const [inputHex, fullHex, simpleHex] = vector;
    const data = Buffer.from(inputHex, "hex");
    assert.deepEqual(casefold(data), Buffer.from(fullHex, "hex"), `fold vector ${idx} full`);
    assert.deepEqual(casefoldSimple(data), Buffer.from(simpleHex, "hex"), `fold vector ${idx} simple`);
  });
}

test("first vector matches the rust-pinned literal", () => {
  // reference.json vectors[0], re-derived by the Rust unit tests; this
  // test fails loudly even if reference.json were regenerated wrongly.
  assert.deepEqual(VECTORS[0], ["e1b88a", "e1b88a", "44cc87"]);
  assert.deepEqual(nfc(Buffer.from("e1b88a", "hex")), Buffer.from("e1b88a", "hex"));
  assert.deepEqual(nfd(Buffer.from("e1b88a", "hex")), Buffer.from("44cc87", "hex"));
});

test("last vector matches the rust-pinned literal", () => {
  // reference.json's final vector at implementation time, pinned the
  // same way: file contents and SDK output against literals.
  assert.deepEqual(VECTORS[VECTORS.length - 1], ["5461cc82cc806e67", "54e1baa76e67", "5461cc82cc806e67"]);
  assert.deepEqual(
    nfc(Buffer.from("5461cc82cc806e67", "hex")),
    Buffer.from("54e1baa76e67", "hex"),
  );
  assert.deepEqual(
    nfd(Buffer.from("5461cc82cc806e67", "hex")),
    Buffer.from("5461cc82cc806e67", "hex"),
  );
});

test("tier-1 ops match pinned literals", () => {
  const lig = Buffer.from("efac81", "hex");
  assert.deepEqual(nfkc(lig), Buffer.from("fi"));
  assert.deepEqual(nfkd(lig), Buffer.from("fi"));
  assert.equal(isNormalized("NFC", lig), true);
  assert.equal(isNormalized("NFKC", lig), false);
  assert.deepEqual(casefold(Buffer.from("c39f", "hex")), Buffer.from("ss"));
  assert.deepEqual(casefoldSimple(Buffer.from("e1ba9e", "hex")), Buffer.from("c39f", "hex"));
  assert.deepEqual(casefold(Buffer.from("ẞ")), Buffer.from("ss"));
  assert.deepEqual(NORMALIZATION_FORMS, { NFC: 1, NFD: 2, NFKC: 3, NFKD: 4 });
  assert.deepEqual([FORM_NFKC, FORM_NFKD], [3, 4]);
});

test("unknown form is a TypeError", () => {
  assert.throws(() => normalize("NFKX", Buffer.alloc(1)), TypeError);
  assert.throws(() => isNormalized(99, Buffer.alloc(1)), TypeError);
  assert.throws(() => normalize(0, Buffer.alloc(1)), TypeError);
});

test("invalid utf-8 is refused, not crashing", () => {
  const ops = [nfc, nfd, nfkc, nfkd, casefold, casefoldSimple];
  for (const op of ops) {
    assert.throws(() => op(Buffer.from([0xff, 0xfe])), (err) => {
      assert.ok(err instanceof FfiError);
      assert.equal(err.status, -2);
      return true;
    });
  }
  assert.throws(() => isNormalized("NFC", Buffer.from([0xff, 0xfe])), (err) => {
    assert.ok(err instanceof FfiError);
    assert.equal(err.status, -2);
    return true;
  });
});

test("null data pointer is refused", () => {
  // koffi passes null as a NULL pointer; the FFI contract answers
  // with a status, never a crash.
  const lib = loadLibrary();
  const out = [null];
  const outLen = [0];
  for (const key of ["nfc", "nfd", "nfkc", "nfkd", "casefold", "casefoldSimple"]) {
    assert.equal(lib[key](null, 0, out, outLen), -1);
  }
  const answer = [0xff];
  assert.equal(lib.isNormalized(1, null, 0, answer), -1);
  assert.equal(lib.isNormalized(1, Buffer.alloc(1), 1, null), -1);
  assert.equal(lib.isNormalized(0, Buffer.alloc(1), 1, answer), -1);
});

test("empty input is valid and round trips", () => {
  assert.deepEqual(nfc(Buffer.alloc(0)), Buffer.alloc(0));
  assert.deepEqual(nfd(Buffer.alloc(0)), Buffer.alloc(0));
  assert.deepEqual(nfkc(Buffer.alloc(0)), Buffer.alloc(0));
  assert.deepEqual(nfkd(Buffer.alloc(0)), Buffer.alloc(0));
  assert.deepEqual(casefold(Buffer.alloc(0)), Buffer.alloc(0));
  assert.deepEqual(casefoldSimple(Buffer.alloc(0)), Buffer.alloc(0));
  for (const form of Object.keys(NORMALIZATION_FORMS)) {
    assert.equal(isNormalized(form, Buffer.alloc(0)), true);
  }
});
