// SPDX-License-Identifier: MIT
// Copyright (c) 2026 pith-hash

// Package pithunicode provides Go bindings for the pith-unicode Rust
// cdylib: Unicode normalisation (NFC, NFD, NFKC, NFKD), quick-check
// detection and case folding.
//
// The single Rust core (built by `cargo build --release`) is loaded at
// runtime; the package carries zero module dependencies. On unix the
// cdylib is opened with dlopen through cgo, on Windows with
// LoadLibrary through the standard syscall package — both resolve the
// library through the same discovery chain, so `go build ./... &&
// go test ./...` works unchanged on every OS the CD matrix builds.
//
// Discovery order (the suite's cdylib convention):
//
//  1. PITH_CDYLIB — an explicit cdylib file path;
//  2. PITH_CDYLIB_DIR — a directory scanned for the cdylib names (the
//     CD pipeline points this at target/release);
//  3. <repo root>/target/release — the repository working-tree layout,
//     anchored at this package's source directory, so a source
//     checkout runs against a local cargo build unconfigured.
//
// The FFI surface is six buffer-handed-out operations plus two query
// operations: pith_unicode_{nfc,nfd,nfkc,nfkd,casefold,casefold_simple}
// validate the input as UTF-8 and hand the caller an owned buffer,
// pith_unicode_is_normalized answers the exact quick-check question,
// and pith_unicode_free releases a handed-out buffer. Invalid UTF-8 is
// a *FfiError with Status == StatusRejected — never a crash.
package pithunicode

import (
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"sync"
	"unsafe"
)

// Status codes returned by the cdylib's C ABI.
const (
	// StatusOK: success.
	StatusOK int32 = 0
	// StatusInvalid: a caller argument is invalid (a null pointer).
	StatusInvalid int32 = -1
	// StatusRejected: the core refused the input (the bytes are not
	// valid UTF-8).
	StatusRejected int32 = -2
)

// NormalizationForm addresses a UAX #15 normalization form, the FFI
// `form` argument encoding.
type NormalizationForm int32

// The four forms.
const (
	FormNFC  NormalizationForm = 1 // canonical composition
	FormNFD  NormalizationForm = 2 // canonical decomposition
	FormNFKC NormalizationForm = 3 // compatibility composition
	FormNFKD NormalizationForm = 4 // compatibility decomposition
)

// Valid reports whether f is one of the four defined forms.
func (f NormalizationForm) Valid() bool {
	return f >= FormNFC && f <= FormNFKD
}

// String returns the form's conventional name.
func (f NormalizationForm) String() string {
	switch f {
	case FormNFC:
		return "NFC"
	case FormNFD:
		return "NFD"
	case FormNFKC:
		return "NFKC"
	case FormNFKD:
		return "NFKD"
	default:
		return fmt.Sprintf("NormalizationForm(%d)", int32(f))
	}
}

// ffiOpName is the cdylib symbol of each form's buffer operation.
var ffiOpName = map[NormalizationForm]string{
	FormNFC:  "pith_unicode_nfc",
	FormNFD:  "pith_unicode_nfd",
	FormNFKC: "pith_unicode_nfkc",
	FormNFKD: "pith_unicode_nfkd",
}

// cdylibNames are the file names cargo may drop into the build
// directory, per platform (windows / linux / macOS).
var cdylibNames = []string{"pith_unicode.dll", "libpith_unicode.so", "libpith_unicode.dylib"}

// FfiError reports a non-zero status code from the cdylib.
type FfiError struct {
	// Op is the FFI operation name.
	Op string
	// Status is the raw status code the FFI returned.
	Status int32
}

func (e *FfiError) Error() string {
	kind := "unknown failure"
	switch e.Status {
	case StatusInvalid:
		kind = "invalid argument"
	case StatusRejected:
		kind = "input rejected"
	}
	return fmt.Sprintf("%s failed: %s (status %d)", e.Op, kind, e.Status)
}

// FindCdylib locates the cdylib through the suite's discovery chain.
func FindCdylib() (string, error) {
	if p := os.Getenv("PITH_CDYLIB"); p != "" {
		if st, err := os.Stat(p); err == nil && st.Mode().IsRegular() {
			return filepath.Abs(p)
		}
	}
	_, thisFile, _, ok := runtime.Caller(0)
	if !ok {
		return "", fmt.Errorf("pithunicode: cannot locate the package source directory")
	}
	pkgDir := filepath.Dir(thisFile)
	repoRoot := filepath.Dir(filepath.Dir(pkgDir)) // sdk/go -> sdk -> repo root

	var dirs []string
	if env := os.Getenv("PITH_CDYLIB_DIR"); env != "" {
		dirs = append(dirs, env)
		if !filepath.IsAbs(env) {
			dirs = append(dirs, filepath.Join(repoRoot, env))
		}
	}
	dirs = append(dirs, filepath.Join(repoRoot, "target", "release"))
	for _, dir := range dirs {
		for _, name := range cdylibNames {
			p := filepath.Join(dir, name)
			if st, err := os.Stat(p); err == nil && st.Mode().IsRegular() {
				return p, nil
			}
		}
	}
	return "", fmt.Errorf(
		"pithunicode: no cdylib found (searched PITH_CDYLIB, PITH_CDYLIB_DIR and <repo>/target/release); run `cargo build --release` first",
	)
}

// locate resolves the cdylib path once per process.
var locate = sync.OnceValues(FindCdylib)

// Nfc normalizes data to Unicode Normalization Form C (canonical
// composition) and returns the UTF-8 result as a fresh copy. Empty
// input is valid and returns an empty slice. Invalid UTF-8 is a
// *FfiError with Status == StatusRejected — never a crash.
func Nfc(data []byte) ([]byte, error) {
	return normalize("pith_unicode_nfc", data)
}

// Nfd normalizes data to Unicode Normalization Form D (canonical
// decomposition), with the same contract as Nfc.
func Nfd(data []byte) ([]byte, error) {
	return normalize("pith_unicode_nfd", data)
}

// Nfkc normalizes data to Unicode Normalization Form KC (compatibility
// composition), with the same contract as Nfc.
func Nfkc(data []byte) ([]byte, error) {
	return normalize("pith_unicode_nfkc", data)
}

// Nfkd normalizes data to Unicode Normalization Form KD (compatibility
// decomposition), with the same contract as Nfc.
func Nfkd(data []byte) ([]byte, error) {
	return normalize("pith_unicode_nfkd", data)
}

// Normalize normalizes data to form (FormNFC, FormNFD, FormNFKC or
// FormNFKD), with the same contract as Nfc. An invalid form is an
// *FfiError with Status == StatusInvalid.
func Normalize(form NormalizationForm, data []byte) ([]byte, error) {
	op, ok := ffiOpName[form]
	if !ok {
		return nil, &FfiError{Op: "pith_unicode_normalize", Status: StatusInvalid}
	}
	return normalize(op, data)
}

// IsNormalized answers whether data is already in form — the exact
// UAX #15 quick-check answer, never a pessimistic approximation. An
// invalid form is an *FfiError with Status == StatusInvalid; invalid
// UTF-8 is StatusRejected.
func IsNormalized(form NormalizationForm, data []byte) (bool, error) {
	if !form.Valid() {
		return false, &FfiError{Op: "pith_unicode_is_normalized", Status: StatusInvalid}
	}
	libPath, err := locate()
	if err != nil {
		return false, err
	}
	var answer byte
	var dataPtr *byte
	if len(data) > 0 {
		dataPtr = &data[0]
	} else {
		dataPtr = &emptyByte
	}
	status, err := ffiIsNormalized(libPath, int32(form), dataPtr, len(data), &answer)
	if err != nil {
		return false, err
	}
	if status != StatusOK {
		return false, &FfiError{Op: "pith_unicode_is_normalized", Status: status}
	}
	return answer == 1, nil
}

// Casefold folds data to its full case folding (UAX #44 statuses C and
// F; mappings may expand, e.g. ß → ss). The Turkic T entries are locale
// data and are not applied. Same contract as Nfc.
func Casefold(data []byte) ([]byte, error) {
	return normalize("pith_unicode_casefold", data)
}

// CasefoldSimple folds data to its simple case folding (statuses C and
// S): a strict one-to-one mapping (ẞ → ß, ß → itself).
func CasefoldSimple(data []byte) ([]byte, error) {
	return normalize("pith_unicode_casefold_simple", data)
}

// emptyByte backs the data pointer for empty inputs: the FFI contract
// treats a NULL data pointer as a caller bug (status -1), so an empty
// — possibly nil — Go slice must hand over a valid non-null pointer
// that reads as zero bytes.
var emptyByte byte

// normalize runs one FFI normalisation op and copies the handed-out
// cdylib buffer into a fresh Go slice before releasing it.
func normalize(opName string, data []byte) ([]byte, error) {
	libPath, err := locate()
	if err != nil {
		return nil, err
	}
	var out *byte
	var outLen uintptr
	var dataPtr *byte
	if len(data) > 0 {
		dataPtr = &data[0]
	} else {
		dataPtr = &emptyByte
	}
	status, err := ffiCall(libPath, opName, dataPtr, len(data), &out, &outLen)
	if err != nil {
		return nil, err
	}
	if status != StatusOK {
		return nil, &FfiError{Op: opName, Status: status}
	}
	buf := make([]byte, outLen)
	copy(buf, unsafe.Slice(out, outLen))
	ffiFree(libPath, out, outLen)
	return buf, nil
}
