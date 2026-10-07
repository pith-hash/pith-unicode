// SPDX-License-Identifier: MIT
// Copyright (c) 2026 pith-hash
"use strict";

// Hex-exact conformance: the committed reference vectors through koffi.
// Every vector in the repository-root reference.json is replayed through
// the cdylib and compared byte-exact — NFC and NFD output bytes against
// the recorded hex. The same vectors the Rust gen-reference verify gate
// and the Python/Go SDKs check.

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const { FfiError, loadLibrary, nfc, nfd } = require("../index.js");

const REPO_ROOT = path.resolve(__dirname, "..", "..", "..");

const REFERENCE = JSON.parse(fs.readFileSync(path.join(REPO_ROOT, "reference.json"), "utf8")).vectors;

test("cdylib is discoverable", () => {
  const { findCdylib } = require("../index.js");
  assert.ok(fs.statSync(findCdylib()).isFile());
});

for (const [idx, vector] of REFERENCE.entries()) {
  test(`reference vector ${idx} is reproduced hex-exact`, () => {
    const [inputHex, nfcHex, nfdHex] = vector;
    const data = Buffer.from(inputHex, "hex");
    assert.deepEqual(nfc(data), Buffer.from(nfcHex, "hex"), `vector ${idx} nfc`);
    assert.deepEqual(nfd(data), Buffer.from(nfdHex, "hex"), `vector ${idx} nfd`);
  });
}

test("first vector matches the rust-pinned literal", () => {
  // reference.json vectors[0], re-derived by the Rust unit tests; this
  // test fails loudly even if reference.json were regenerated wrongly.
  assert.deepEqual(REFERENCE[0], ["e1b88a", "e1b88a", "44cc87"]);
  assert.deepEqual(nfc(Buffer.from("e1b88a", "hex")), Buffer.from("e1b88a", "hex"));
  assert.deepEqual(nfd(Buffer.from("e1b88a", "hex")), Buffer.from("44cc87", "hex"));
});

test("last vector matches the rust-pinned literal", () => {
  // reference.json's final vector at implementation time, pinned the
  // same way: file contents and SDK output against literals.
  assert.deepEqual(REFERENCE[REFERENCE.length - 1], ["5461cc82cc806e67", "54e1baa76e67", "5461cc82cc806e67"]);
  assert.deepEqual(
    nfc(Buffer.from("5461cc82cc806e67", "hex")),
    Buffer.from("54e1baa76e67", "hex"),
  );
  assert.deepEqual(
    nfd(Buffer.from("5461cc82cc806e67", "hex")),
    Buffer.from("5461cc82cc806e67", "hex"),
  );
});

test("invalid utf-8 is refused, not crashing", () => {
  for (const op of [nfc, nfd]) {
    assert.throws(() => op(Buffer.from([0xff, 0xfe])), (err) => {
      assert.ok(err instanceof FfiError);
      assert.equal(err.status, -2);
      return true;
    });
  }
});

test("null data pointer is refused", () => {
  // koffi passes null as a NULL pointer; the FFI contract answers
  // with a status, never a crash.
  const { nfc: rawNfc, nfd: rawNfd } = loadLibrary();
  const out = [null];
  const outLen = [0];
  assert.equal(rawNfc(null, 0, out, outLen), -1);
  assert.equal(rawNfd(null, 0, out, outLen), -1);
});

test("empty input is valid and round trips", () => {
  assert.deepEqual(nfc(Buffer.alloc(0)), Buffer.alloc(0));
  assert.deepEqual(nfd(Buffer.alloc(0)), Buffer.alloc(0));
});
