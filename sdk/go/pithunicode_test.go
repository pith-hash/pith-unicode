// SPDX-License-Identifier: MIT
// Copyright (c) 2026 pith-hash

package pithunicode

import (
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
	raw, err := os.ReadFile(filepath.Join(repoRoot(t), "reference.json"))
	if err != nil {
		t.Fatal(err)
	}
	var parsed struct {
		Vectors [][]string `json:"vectors"`
	}
	if err := json.Unmarshal(raw, &parsed); err != nil {
		t.Fatal(err)
	}
	return parsed.Vectors
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
}

// TestNullDataPointerIsRefused drives the raw binding with a NULL
// data pointer: the FFI contract answers with a status, never a
// crash.
func TestNullDataPointerIsRefused(t *testing.T) {
	libPath, err := locate()
	if err != nil {
		t.Fatal(err)
	}
	for _, opName := range []string{"pith_unicode_nfc", "pith_unicode_nfd"} {
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
	} {
		got, err := op.fn(nil)
		if err != nil {
			t.Fatalf("%s(\"\"): %v", op.name, err)
		}
		if len(got) != 0 {
			t.Errorf("%s(\"\") = %x, want empty", op.name, got)
		}
	}
}
