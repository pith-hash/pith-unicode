// SPDX-License-Identifier: MIT
// Copyright (c) 2026 pith-hash

package pithunicode

import (
	"bytes"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"testing"
)

// repoRoot resolves the repository root relative to this package
// (sdk/go -> sdk -> repo root), the anchor for reference.json.
func repoRoot(t *testing.T) string {
	t.Helper()
	root, err := filepath.Abs(filepath.Join("..", ".."))
	if err != nil {
		t.Fatal(err)
	}
	if st, err := os.Stat(filepath.Join(root, "reference.json")); err != nil || st.IsDir() {
		t.Fatalf("reference.json not found at %s", root)
	}
	return root
}

// vectors parses the committed reference.json: [input_hex, nfc_hex,
// nfd_hex], byte-hex UTF-8 in and out.
func vectors(t *testing.T) [][]string {
	t.Helper()
	return section(t, "vectors")
}

// formVectors parses reference.json's form_vectors: [input_hex, nfc,
// nfd, nfkc, nfkd] — all four forms per row.
func formVectors(t *testing.T) [][]string {
	t.Helper()
	return section(t, "form_vectors")
}

// casefoldVectors parses reference.json's casefold_vectors:
// [input_hex, full_fold, simple_fold].
func casefoldVectors(t *testing.T) [][]string {
	t.Helper()
	return section(t, "casefold_vectors")
}

// section decodes one string-table section of the committed
// reference.json.
func section(t *testing.T, name string) [][]string {
	t.Helper()
	raw, err := os.ReadFile(filepath.Join(repoRoot(t), "reference.json"))
	if err != nil {
		t.Fatal(err)
	}
	var parsed struct {
		Vectors         [][]string `json:"vectors"`
		FormVectors     [][]string `json:"form_vectors"`
		CasefoldVectors [][]string `json:"casefold_vectors"`
	}
	if err := json.Unmarshal(raw, &parsed); err != nil {
		t.Fatal(err)
	}
	switch name {
	case "vectors":
		return parsed.Vectors
	case "form_vectors":
		return parsed.FormVectors
	case "casefold_vectors":
		return parsed.CasefoldVectors
	default:
		t.Fatalf("unknown section %q", name)
		return nil
	}
}

// TestReferenceVectorsHexExact replays every committed reference.json
// vector through the cdylib and compares byte-exact — NFC and NFD
// output bytes against the recorded hex, both asserted per vector.
// The same vectors the Rust gen-reference verify gate and the
// Python/Node SDKs check.
func TestReferenceVectorsHexExact(t *testing.T) {
	for idx, want := range vectors(t) {
		t.Run(fmt.Sprintf("vector_%d", idx), func(t *testing.T) {
			if len(want) != 3 {
				t.Fatalf("vector %d: want [input, nfc, nfd], got %v", idx, want)
			}
			data, err := hex.DecodeString(want[0])
			if err != nil {
				t.Fatalf("vector %d: bad input hex: %v", idx, err)
			}
			gotNfc, err := Nfc(data)
			if err != nil {
				t.Fatalf("vector %d Nfc: %v", idx, err)
			}
			if gotNfc, wantNfc := hex.EncodeToString(gotNfc), want[1]; gotNfc != wantNfc {
				t.Errorf("vector %d nfc: got %s, want %s", idx, gotNfc, wantNfc)
			}
			gotNfd, err := Nfd(data)
			if err != nil {
				t.Fatalf("vector %d Nfd: %v", idx, err)
			}
			if gotNfd, wantNfd := hex.EncodeToString(gotNfd), want[2]; gotNfd != wantNfd {
				t.Errorf("vector %d nfd: got %s, want %s", idx, gotNfd, wantNfd)
			}
		})
	}
}

// TestPinnedReferenceVectors pins two vectors the Rust unit tests
// re-derive — reference.json vectors[0] and its last vector at
// implementation time — so the binding fails loudly even if
// reference.json were regenerated wrongly.
func TestPinnedReferenceVectors(t *testing.T) {
	table := vectors(t)
	if len(table) != 952 {
		t.Fatalf("reference.json carries %d vectors, want 952", len(table))
	}
	pins := []struct {
		at              int
		input, nfc, nfd string
	}{
		{0, "e1b88a", "e1b88a", "44cc87"},
		{len(table) - 1, "5461cc82cc806e67", "54e1baa76e67", "5461cc82cc806e67"},
	}
	for _, pin := range pins {
		t.Run(fmt.Sprintf("vector_%d", pin.at), func(t *testing.T) {
			if got := table[pin.at]; got[0] != pin.input || got[1] != pin.nfc || got[2] != pin.nfd {
				t.Fatalf("reference.json vectors[%d] = %v, want [%s %s %s]", pin.at, got, pin.input, pin.nfc, pin.nfd)
			}
			data, err := hex.DecodeString(pin.input)
			if err != nil {
				t.Fatal(err)
			}
			gotNfc, err := Nfc(data)
			if err != nil {
				t.Fatal(err)
			}
			if hex.EncodeToString(gotNfc) != pin.nfc {
				t.Errorf("Nfc(%s) = %s, want %s", pin.input, hex.EncodeToString(gotNfc), pin.nfc)
			}
			gotNfd, err := Nfd(data)
			if err != nil {
				t.Fatal(err)
			}
			if hex.EncodeToString(gotNfd) != pin.nfd {
				t.Errorf("Nfd(%s) = %s, want %s", pin.input, hex.EncodeToString(gotNfd), pin.nfd)
			}
		})
	}
}

// TestInvalidUtf8IsRefused checks the refusal path: a status code,
// never a crash.
func TestInvalidUtf8IsRefused(t *testing.T) {
	for _, op := range []struct {
		name string
		fn   func([]byte) ([]byte, error)
	}{
		{"Nfc", Nfc},
		{"Nfd", Nfd},
		{"Nfkc", Nfkc},
		{"Nfkd", Nfkd},
		{"Casefold", Casefold},
		{"CasefoldSimple", CasefoldSimple},
	} {
		_, err := op.fn([]byte{0xff, 0xfe})
		var ffi *FfiError
		if e, ok := err.(*FfiError); ok {
			ffi = e
		} else {
			t.Fatalf("%s: want FfiError, got %v", op.name, err)
		}
		if ffi.Status != StatusRejected {
			t.Errorf("%s: want StatusRejected, got %d", op.name, ffi.Status)
		}
	}
	if _, err := IsNormalized(FormNFC, []byte{0xff, 0xfe}); !isRejected(err) {
		t.Errorf("IsNormalized: want StatusRejected, got %v", err)
	}
}

// isRejected reports whether err is an *FfiError with StatusRejected.
func isRejected(err error) bool {
	ffi, ok := err.(*FfiError)
	return ok && ffi.Status == StatusRejected
}

// TestNullDataPointerIsRefused drives the raw binding with a NULL
// data pointer: the FFI contract answers with a status, never a
// crash.
func TestNullDataPointerIsRefused(t *testing.T) {
	libPath, err := locate()
	if err != nil {
		t.Fatal(err)
	}
	for _, opName := range []string{
		"pith_unicode_nfc", "pith_unicode_nfd", "pith_unicode_nfkc",
		"pith_unicode_nfkd", "pith_unicode_casefold", "pith_unicode_casefold_simple",
	} {
		var out *byte
		var outLen uintptr
		status, err := ffiCall(libPath, opName, nil, 0, &out, &outLen)
		if err != nil {
			t.Fatalf("%s: %v", opName, err)
		}
		if status != StatusInvalid {
			t.Errorf("%s: want StatusInvalid, got %d", opName, status)
		}
	}
	var answer byte
	status, err := ffiIsNormalized(libPath, int32(FormNFC), nil, 0, &answer)
	if err != nil {
		t.Fatal(err)
	}
	if status != StatusInvalid {
		t.Errorf("is_normalized(nil): want StatusInvalid, got %d", status)
	}
	if _, err := IsNormalized(0, []byte("a")); !isStatusInvalid(err) {
		t.Errorf("IsNormalized(0): want StatusInvalid, got %v", err)
	}
}

// isStatusInvalid reports whether err is an *FfiError with
// StatusInvalid.
func isStatusInvalid(err error) bool {
	ffi, ok := err.(*FfiError)
	return ok && ffi.Status == StatusInvalid
}

// TestEmptyInputIsValidAndRoundTrips exercises the empty-output
// allocation path: status OK, zero-length copy, no error.
func TestEmptyInputIsValidAndRoundTrips(t *testing.T) {
	for _, op := range []struct {
		name string
		fn   func([]byte) ([]byte, error)
	}{
		{"Nfc", Nfc},
		{"Nfd", Nfd},
		{"Nfkc", Nfkc},
		{"Nfkd", Nfkd},
		{"Casefold", Casefold},
		{"CasefoldSimple", CasefoldSimple},
	} {
		got, err := op.fn(nil)
		if err != nil {
			t.Fatalf("%s(\"\"): %v", op.name, err)
		}
		if len(got) != 0 {
			t.Errorf("%s(\"\") = %x, want empty", op.name, got)
		}
	}
	for _, form := range []NormalizationForm{FormNFC, FormNFD, FormNFKC, FormNFKD} {
		ok, err := IsNormalized(form, nil)
		if err != nil {
			t.Fatalf("IsNormalized(%s, nil): %v", form, err)
		}
		if !ok {
			t.Errorf("IsNormalized(%s, nil) = false, want true", form)
		}
	}
}

// TestFormVectorsHexExact replays every form_vectors row through all
// four forms and asserts the quick-check answer matches the slow-path
// bytes for every form.
func TestFormVectorsHexExact(t *testing.T) {
	rows := formVectors(t)
	if len(rows) == 0 {
		t.Fatal("reference.json form_vectors is empty")
	}
	for idx, row := range rows {
		if len(row) != 5 {
			t.Fatalf("form vector %d: want [input, nfc, nfd, nfkc, nfkd], got %d cols", idx, len(row))
		}
		data, err := hex.DecodeString(row[0])
		if err != nil {
			t.Fatalf("form vector %d: bad input hex: %v", idx, err)
		}
		t.Run(fmt.Sprintf("vector_%d", idx), func(t *testing.T) {
			for _, fc := range []struct {
				form NormalizationForm
				fn   func([]byte) ([]byte, error)
				want string
			}{
				{FormNFC, Nfc, row[1]},
				{FormNFD, Nfd, row[2]},
				{FormNFKC, Nfkc, row[3]},
				{FormNFKD, Nfkd, row[4]},
			} {
				got, err := fc.fn(data)
				if err != nil {
					t.Fatalf("%s: %v", fc.form, err)
				}
				if hex.EncodeToString(got) != fc.want {
					t.Errorf("%s(%s) = %s, want %s", fc.form, row[0], hex.EncodeToString(got), fc.want)
				}
				// Dispatch through Normalize answers byte-identically.
				viaForm, err := Normalize(fc.form, data)
				if err != nil {
					t.Fatalf("Normalize(%s): %v", fc.form, err)
				}
				if !bytes.Equal(viaForm, got) {
					t.Errorf("Normalize(%s) disagrees with %s", fc.form, fc.form)
				}
				// The quick check is the exact slow-path truth: the
				// form is a fixed point iff normalization is the
				// identity on the input.
				norm, err := IsNormalized(fc.form, data)
				if err != nil {
					t.Fatalf("IsNormalized(%s): %v", fc.form, err)
				}
				if norm != bytes.Equal(data, got) {
					t.Errorf("IsNormalized(%s, %s) = %v, disagreeing with the slow path", fc.form, row[0], norm)
				}
			}
		})
	}
}

// TestCasefoldVectorsHexExact replays every casefold_vectors row
// through both folding flavours.
func TestCasefoldVectorsHexExact(t *testing.T) {
	rows := casefoldVectors(t)
	if len(rows) == 0 {
		t.Fatal("reference.json casefold_vectors is empty")
	}
	for idx, row := range rows {
		if len(row) != 3 {
			t.Fatalf("casefold vector %d: want [input, full, simple], got %d cols", idx, len(row))
		}
		data, err := hex.DecodeString(row[0])
		if err != nil {
			t.Fatalf("casefold vector %d: bad input hex: %v", idx, err)
		}
		t.Run(fmt.Sprintf("vector_%d", idx), func(t *testing.T) {
			full, err := Casefold(data)
			if err != nil {
				t.Fatal(err)
			}
			if hex.EncodeToString(full) != row[1] {
				t.Errorf("Casefold(%s) = %s, want %s", row[0], hex.EncodeToString(full), row[1])
			}
			simple, err := CasefoldSimple(data)
			if err != nil {
				t.Fatal(err)
			}
			if hex.EncodeToString(simple) != row[2] {
				t.Errorf("CasefoldSimple(%s) = %s, want %s", row[0], hex.EncodeToString(simple), row[2])
			}
		})
	}
}

// TestTier1PinnedOps pins the tier-1 cases the Rust unit tests pin:
// the ﬁ ligature expands under NFKC/NFKD, ß folds fully to "ss", ẞ
// folds simply to ß, and the quick-check edge answers exactly.
func TestTier1PinnedOps(t *testing.T) {
	lig, err := hex.DecodeString("efac81")
	if err != nil {
		t.Fatal(err)
	}
	for _, fc := range []struct {
		name string
		fn   func([]byte) ([]byte, error)
	}{
		{"Nfkc", Nfkc},
		{"Nfkd", Nfkd},
	} {
		got, err := fc.fn(lig)
		if err != nil {
			t.Fatal(err)
		}
		if string(got) != "fi" {
			t.Errorf("%s(ﬁ) = %q, want %q", fc.name, got, "fi")
		}
	}
	if ok, err := IsNormalized(FormNFC, lig); err != nil || !ok {
		t.Errorf("IsNormalized(NFC, ﬁ) = %v, %v; want true, nil", ok, err)
	}
	if ok, err := IsNormalized(FormNFKC, lig); err != nil || ok {
		t.Errorf("IsNormalized(NFKC, ﬁ) = %v, %v; want false, nil", ok, err)
	}
	ss, _ := hex.DecodeString("c39f") // ß
	full, err := Casefold(ss)
	if err != nil || string(full) != "ss" {
		t.Errorf("Casefold(ß) = %q, %v; want \"ss\", nil", full, err)
	}
	cap, _ := hex.DecodeString("e1ba9e") // ẞ
	simple, err := CasefoldSimple(cap)
	if err != nil || string(simple) != string(ss) {
		t.Errorf("CasefoldSimple(ẞ) = %q, %v; want \"ß\", nil", simple, err)
	}
	fullCap, err := Casefold(cap)
	if err != nil || string(fullCap) != "ss" {
		t.Errorf("Casefold(ẞ) = %q, %v; want \"ss\", nil", fullCap, err)
	}
	// NormalizationForm plumbing.
	if !(FormNFC.Valid() && FormNFD.Valid() && FormNFKC.Valid() && FormNFKD.Valid()) {
		t.Error("the four defined forms must be Valid")
	}
	if FormNFC.String() != "NFC" || FormNFKD.String() != "NFKD" {
		t.Error("form String() names drifted")
	}
}
