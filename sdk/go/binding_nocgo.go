// SPDX-License-Identifier: MIT
// Copyright (c) 2026 pith-hash

//go:build !windows && !cgo

package pithunicode

import "fmt"

// ffiCall is unavailable without cgo on unix: there is no pure-Go
// dlopen in the standard library. Build with CGO_ENABLED=1 (the CD
// pipeline always does).
func ffiCall(string, string, *byte, int, **byte, *uintptr) (int32, error) {
	return 0, fmt.Errorf("pithunicode: cgo is required to load the cdylib on this platform (build with CGO_ENABLED=1)")
}

// ffiFree mirrors the unavailable calls.
func ffiFree(string, *byte, uintptr) {}
