# SPDX-License-Identifier: MIT
# Copyright (c) 2026 pith-hash
"""Hex-exact conformance: the committed reference vectors through ctypes.

Every vector in the repository-root ``reference.json`` is replayed
through the cdylib and compared byte-exact — NFC and NFD output bytes
against the recorded hex. The same vectors the Rust ``gen-reference
verify`` gate and the Node/Go SDKs check.
"""

from __future__ import annotations

import ctypes
import json
from pathlib import Path

import pytest

import pith_unicode
from pith_unicode import FfiError, find_cdylib

REPO_ROOT = Path(__file__).resolve().parents[3]

#: ``[[input_hex, nfc_hex, nfd_hex] × 952]``, byte-hex UTF-8 in and out.
VECTORS = json.loads((REPO_ROOT / "reference.json").read_text(encoding="utf-8"))["vectors"]


def test_cdylib_is_discoverable() -> None:
    path = find_cdylib()
    assert path.is_file(), path


@pytest.mark.parametrize("idx", range(len(VECTORS)))
def test_reference_vector_is_reproduced_hex_exact(idx: int) -> None:
    input_hex, nfc_hex, nfd_hex = VECTORS[idx]
    data = bytes.fromhex(input_hex)
    assert pith_unicode.nfc(data) == bytes.fromhex(nfc_hex), f"vector {idx} nfc"
    assert pith_unicode.nfd(data) == bytes.fromhex(nfd_hex), f"vector {idx} nfd"


def test_first_vector_matches_the_rust_pinned_literal() -> None:
    # reference.json vectors[0], re-derived by the Rust unit tests; this
    # test fails loudly even if reference.json were regenerated wrongly.
    assert VECTORS[0] == ["e1b88a", "e1b88a", "44cc87"]
    assert pith_unicode.nfc(bytes.fromhex("e1b88a")) == bytes.fromhex("e1b88a")
    assert pith_unicode.nfd(bytes.fromhex("e1b88a")) == bytes.fromhex("44cc87")


def test_last_vector_matches_the_rust_pinned_literal() -> None:
    # reference.json's final vector at implementation time, pinned the
    # same way: file contents and SDK output against literals.
    assert VECTORS[-1] == ["5461cc82cc806e67", "54e1baa76e67", "5461cc82cc806e67"]
    assert pith_unicode.nfc(bytes.fromhex("5461cc82cc806e67")) == bytes.fromhex("54e1baa76e67")
    assert pith_unicode.nfd(bytes.fromhex("5461cc82cc806e67")) == bytes.fromhex(
        "5461cc82cc806e67"
    )


def test_invalid_utf8_is_refused_not_crashing() -> None:
    with pytest.raises(FfiError) as err:
        pith_unicode.nfc(b"\xff\xfe")
    assert err.value.status == -2
    with pytest.raises(FfiError) as err:
        pith_unicode.nfd(b"\xff\xfe")
    assert err.value.status == -2


def test_null_data_pointer_is_refused() -> None:
    # ctypes passes None as a NULL pointer through the c_void_p
    # argtype; the FFI contract answers with a status, never a crash.
    lib = pith_unicode._load()
    out = ctypes.c_void_p()
    out_len = ctypes.c_size_t()
    assert lib.pith_unicode_nfc(None, 0, ctypes.byref(out), ctypes.byref(out_len)) == -1
    assert lib.pith_unicode_nfd(None, 0, ctypes.byref(out), ctypes.byref(out_len)) == -1


def test_empty_input_is_valid_and_round_trips() -> None:
    assert pith_unicode.nfc(b"") == b""
    assert pith_unicode.nfd(b"") == b""
