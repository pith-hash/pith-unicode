# SPDX-License-Identifier: MIT
# Copyright (c) 2026 pith-hash
"""Hex-exact conformance: the committed reference vectors through ctypes.

Every vector in the repository-root ``reference.json`` is replayed
through the cdylib and compared byte-exact — the historical NFC/NFD
rows and the tier-1 ``form_vectors`` (all four forms) and
``casefold_vectors`` (full + simple folding) sections. The same vectors
the Rust ``gen-reference verify`` gate and the Node/Go SDKs check.
"""

from __future__ import annotations

import ctypes
import json
from pathlib import Path

import pytest

import pith_unicode
from pith_unicode import FfiError, find_cdylib

REPO_ROOT = Path(__file__).resolve().parents[3]

_raw = json.loads((REPO_ROOT / "reference.json").read_text(encoding="utf-8"))

#: ``[[input_hex, nfc_hex, nfd_hex] × 952]``, byte-hex UTF-8 in and out.
VECTORS = _raw["vectors"]

#: ``[[input_hex, nfc, nfd, nfkc, nfkd] × N]`` — all four forms.
FORM_VECTORS = _raw["form_vectors"]

#: ``[[input_hex, full_fold, simple_fold] × N]``.
CASEFOLD_VECTORS = _raw["casefold_vectors"]


def test_cdylib_is_discoverable() -> None:
    path = find_cdylib()
    assert path.is_file(), path


@pytest.mark.parametrize("idx", range(len(VECTORS)))
def test_reference_vector_is_reproduced_hex_exact(idx: int) -> None:
    input_hex, nfc_hex, nfd_hex = VECTORS[idx]
    data = bytes.fromhex(input_hex)
    assert pith_unicode.nfc(data) == bytes.fromhex(nfc_hex), f"vector {idx} nfc"
    assert pith_unicode.nfd(data) == bytes.fromhex(nfd_hex), f"vector {idx} nfd"


@pytest.mark.parametrize("idx", range(len(FORM_VECTORS)))
def test_form_vector_is_reproduced_hex_exact(idx: int) -> None:
    input_hex, nfc_hex, nfd_hex, nfkc_hex, nfkd_hex = FORM_VECTORS[idx]
    data = bytes.fromhex(input_hex)
    assert pith_unicode.nfc(data) == bytes.fromhex(nfc_hex), f"form vector {idx} nfc"
    assert pith_unicode.nfd(data) == bytes.fromhex(nfd_hex), f"form vector {idx} nfd"
    assert pith_unicode.nfkc(data) == bytes.fromhex(nfkc_hex), f"form vector {idx} nfkc"
    assert pith_unicode.nfkd(data) == bytes.fromhex(nfkd_hex), f"form vector {idx} nfkd"
    # The form dispatch answers byte-identically through every spelling.
    assert pith_unicode.normalize("NFKC", data) == bytes.fromhex(nfkc_hex)
    assert pith_unicode.normalize("nfkd", data) == bytes.fromhex(nfkd_hex)
    assert pith_unicode.normalize(pith_unicode.FORM_NFC, data) == bytes.fromhex(nfc_hex)
    # Quick-check detection is the exact slow-path truth.
    for form, out_hex in [
        (pith_unicode.FORM_NFC, nfc_hex),
        (pith_unicode.FORM_NFD, nfd_hex),
        (pith_unicode.FORM_NFKC, nfkc_hex),
        (pith_unicode.FORM_NFKD, nfkd_hex),
    ]:
        assert pith_unicode.is_normalized(form, data) == (
            data == bytes.fromhex(out_hex)
        ), f"form vector {idx} is_normalized"


@pytest.mark.parametrize("idx", range(len(CASEFOLD_VECTORS)))
def test_casefold_vector_is_reproduced_hex_exact(idx: int) -> None:
    input_hex, full_hex, simple_hex = CASEFOLD_VECTORS[idx]
    data = bytes.fromhex(input_hex)
    assert pith_unicode.casefold(data) == bytes.fromhex(full_hex), f"fold vector {idx} full"
    assert pith_unicode.casefold_simple(data) == bytes.fromhex(
        simple_hex
    ), f"fold vector {idx} simple"
    # Independent oracle: Python's own full case folding must agree.
    assert pith_unicode.casefold(data) == data.decode("utf-8").casefold().encode("utf-8")


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


def test_tier1_ops_match_pinned_literals() -> None:
    # Pinned tier-1 cases (also pinned in the Rust unit tests): the ﬁ
    # ligature expands under NFKC/NFKD, ß folds fully to "ss", ẞ folds
    # simply to ß, and the quick-check edge answers exactly.
    lig = bytes.fromhex("efac81")
    assert pith_unicode.nfkc(lig) == b"fi"
    assert pith_unicode.nfkd(lig) == b"fi"
    assert pith_unicode.is_normalized("NFC", lig) is True
    assert pith_unicode.is_normalized("NFKC", lig) is False
    assert pith_unicode.casefold(bytes.fromhex("c39f")) == b"ss"
    assert pith_unicode.casefold_simple(bytes.fromhex("e1ba9e")) == bytes.fromhex("c39f")
    assert pith_unicode.casefold("ẞ".encode()) == b"ss"
    # The form code round trip through the SDK constants.
    assert (pith_unicode.FORM_NFC, pith_unicode.FORM_NFD) == (1, 2)
    assert (pith_unicode.FORM_NFKC, pith_unicode.FORM_NFKD) == (3, 4)
    assert pith_unicode.NORMALIZATION_FORMS == {
        "NFC": 1,
        "NFD": 2,
        "NFKC": 3,
        "NFKD": 4,
    }


def test_unknown_form_is_a_value_error() -> None:
    with pytest.raises(ValueError):
        pith_unicode.normalize("NFKX", b"a")
    with pytest.raises(ValueError):
        pith_unicode.is_normalized(99, b"a")
    with pytest.raises(ValueError):
        pith_unicode.normalize(0, b"a")


def test_invalid_utf8_is_refused_not_crashing() -> None:
    with pytest.raises(FfiError) as err:
        pith_unicode.nfc(b"\xff\xfe")
    assert err.value.status == -2
    with pytest.raises(FfiError) as err:
        pith_unicode.nfd(b"\xff\xfe")
    assert err.value.status == -2
    for op in (pith_unicode.nfkc, pith_unicode.nfkd, pith_unicode.casefold, pith_unicode.casefold_simple):
        with pytest.raises(FfiError) as err:
            op(b"\xff\xfe")
        assert err.value.status == -2
    with pytest.raises(FfiError) as err:
        pith_unicode.is_normalized("NFC", b"\xff\xfe")
    assert err.value.status == -2


def test_null_data_pointer_is_refused() -> None:
    # ctypes passes None as a NULL pointer through the c_void_p
    # argtype; the FFI contract answers with a status, never a crash.
    lib = pith_unicode._load()
    out = ctypes.c_void_p()
    out_len = ctypes.c_size_t()
    for op in (
        "pith_unicode_nfc",
        "pith_unicode_nfd",
        "pith_unicode_nfkc",
        "pith_unicode_nfkd",
        "pith_unicode_casefold",
        "pith_unicode_casefold_simple",
    ):
        assert getattr(lib, op)(None, 0, ctypes.byref(out), ctypes.byref(out_len)) == -1
    answer = ctypes.c_uint8()
    assert lib.pith_unicode_is_normalized(1, None, 0, ctypes.byref(answer)) == -1
    assert lib.pith_unicode_is_normalized(1, b"a", 1, None) == -1
    assert lib.pith_unicode_is_normalized(0, b"a", 1, ctypes.byref(answer)) == -1


def test_empty_input_is_valid_and_round_trips() -> None:
    assert pith_unicode.nfc(b"") == b""
    assert pith_unicode.nfd(b"") == b""
    assert pith_unicode.nfkc(b"") == b""
    assert pith_unicode.nfkd(b"") == b""
    assert pith_unicode.casefold(b"") == b""
    assert pith_unicode.casefold_simple(b"") == b""
    for form in pith_unicode.NORMALIZATION_FORMS:
        assert pith_unicode.is_normalized(form, b"") is True
