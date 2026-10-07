# SPDX-License-Identifier: MIT
# Copyright (c) 2026 pith-hash
"""pith-unicode SDK: Unicode NFC/NFD normalisation through ctypes.

Every function validates its input as UTF-8 inside the Rust core and
returns fresh ``bytes`` — the handed-out cdylib buffer is copied into
the result and released before returning. Invalid UTF-8 raises
:class:`FfiError` with ``status == STATUS_REJECTED``; it is never a
crash.

Example:
    >>> import pith_unicode
    >>> pith_unicode.nfc("Tasosteel".encode("utf-8"))
    b'Tasosteel'
"""

from __future__ import annotations

import ctypes
import os
from pathlib import Path

__all__ = [
    "FfiError",
    "LibraryNotFoundError",
    "STATUS_OK",
    "STATUS_INVALID",
    "STATUS_REJECTED",
    "CDYLIB_NAMES",
    "find_cdylib",
    "nfc",
    "nfd",
]

#: Status: success.
STATUS_OK = 0
#: Status: a caller argument is invalid (a null pointer).
STATUS_INVALID = -1
#: Status: the core refused the input (the bytes are not valid UTF-8).
STATUS_REJECTED = -2

#: Every cdylib file name cargo may drop into the build directory, per
#: platform (windows / linux / macOS).
CDYLIB_NAMES = ("pith_unicode.dll", "libpith_unicode.so", "libpith_unicode.dylib")


class LibraryNotFoundError(OSError):
    """No cdylib was found through the discovery chain."""


class FfiError(Exception):
    """A non-zero status code came back from the cdylib."""

    def __init__(self, op: str, status: int) -> None:
        kind = {
            STATUS_INVALID: "invalid argument",
            STATUS_REJECTED: "input rejected",
        }.get(status, "unknown failure")
        super().__init__(f"{op} failed: {kind} (status {status})")
        #: The raw status code the FFI returned.
        self.status = status


def find_cdylib() -> Path:
    """Locates the cdylib through the suite's discovery chain."""
    explicit = os.environ.get("PITH_CDYLIB")
    if explicit:
        p = Path(explicit)
        if p.is_file():
            return p
    env_dir = os.environ.get("PITH_CDYLIB_DIR")
    candidates: list[Path] = []
    if env_dir:
        env_dir_path = Path(env_dir)
        candidates.append(env_dir_path)
        if not env_dir_path.is_absolute():
            # CD and local runs invoke tools from the repository root or
            # from sdk/<lang>; resolve the env value against both.
            candidates.append(Path.cwd() / env_dir_path)
            candidates.append(Path(__file__).resolve().parents[3] / env_dir_path)
    candidates.append(Path(__file__).resolve().parent)  # packaged wheel
    candidates.append(Path(__file__).resolve().parents[3] / "target" / "release")
    for directory in candidates:
        for name in CDYLIB_NAMES:
            p = directory / name
            if p.is_file():
                return p
    raise LibraryNotFoundError(
        "no pith-unicode cdylib found (searched PITH_CDYLIB, PITH_CDYLIB_DIR, "
        "the package directory and <repo>/target/release); "
        "run `cargo build --release` first"
    )


_lib: ctypes.CDLL | None = None


def _load() -> ctypes.CDLL:
    global _lib
    if _lib is None:
        lib = ctypes.CDLL(str(find_cdylib()))
        for op in ("pith_unicode_nfc", "pith_unicode_nfd"):
            fn = getattr(lib, op)
            fn.argtypes = [
                ctypes.c_void_p,  # data
                ctypes.c_size_t,  # len
                ctypes.POINTER(ctypes.c_void_p),  # out buffer
                ctypes.POINTER(ctypes.c_size_t),  # out length
            ]
            fn.restype = ctypes.c_int32
        lib.pith_unicode_free.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
        lib.pith_unicode_free.restype = None
        _lib = lib
    return _lib


def _normalize(op_name: str, data: bytes) -> bytes:
    """Runs one normalisation op and copies the handed-out buffer."""
    out = ctypes.c_void_p()
    out_len = ctypes.c_size_t()
    status = getattr(_load(), op_name)(
        data, len(data), ctypes.byref(out), ctypes.byref(out_len)
    )
    if status != STATUS_OK:
        raise FfiError(op_name, status)
    try:
        return ctypes.string_at(out, out_len.value)
    finally:
        _load().pith_unicode_free(out, out_len.value)


def nfc(data: bytes) -> bytes:
    """Normalizes ``data`` to Unicode Normalization Form C (canonical
    composition) and returns the UTF-8 result as fresh ``bytes``.

    Empty input is valid and returns ``b""``. Raises :class:`FfiError`
    with ``status == STATUS_REJECTED`` for bytes that are not valid
    UTF-8 — a rejection, never a crash.
    """
    return _normalize("pith_unicode_nfc", data)


def nfd(data: bytes) -> bytes:
    """Normalizes ``data`` to Unicode Normalization Form D (canonical
    decomposition), with the same contract as :func:`nfc`."""
    return _normalize("pith_unicode_nfd", data)
