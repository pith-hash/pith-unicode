// SPDX-License-Identifier: MIT
// Copyright (c) 2026 pith-hash

//go:build windows

package pithunicode

import (
	"fmt"
	"syscall"
	"unsafe"
)

// openProc loads libPath and resolves name. The library is released
// before returning: on Windows FreeLibrary unmaps the cdylib, so the
// proc must be used (and its buffer copied out) inside the caller.
func openProc(libPath, name string) (proc uintptr, release func(), err error) {
	lib, err := syscall.LoadLibrary(libPath)
	if err != nil {
		return 0, nil, fmt.Errorf("pithunicode: LoadLibrary(%s): %w", libPath, err)
	}
	release = func() { syscall.FreeLibrary(lib) }
	proc, err = syscall.GetProcAddress(lib, name)
	if err != nil {
		release()
		return 0, nil, fmt.Errorf("pithunicode: symbol %s missing from %s: %w", name, libPath, err)
	}
	return proc, release, nil
}

// ffiCall loads the cdylib with LoadLibrary (absolute path, no PATH
// involvement), resolves opName and calls it. data may be nil (an
// empty input); out receives the handed-out buffer, outLen its exact
// length. The returned buffer stays alive in the cdylib until ffiFree.
func ffiCall(libPath, opName string, data *byte, n int, out **byte, outLen *uintptr) (int32, error) {
	proc, release, err := openProc(libPath, opName)
	if err != nil {
		return 0, err
	}
	defer release()

	var cOut *byte
	var cLen uintptr
	rc, _, _ := syscall.SyscallN(proc,
		uintptr(unsafe.Pointer(data)),
		uintptr(n),
		uintptr(unsafe.Pointer(&cOut)),
		uintptr(unsafe.Pointer(&cLen)),
	)
	*out = cOut
	*outLen = cLen
	return int32(rc), nil
}

// ffiFree resolves pith_unicode_free and releases a buffer handed out
// by ffiCall. Null is accepted (the cdylib ignores it).
func ffiFree(libPath string, ptr *byte, n uintptr) {
	proc, release, err := openProc(libPath, "pith_unicode_free")
	if err != nil {
		return // the library vanished mid-flight; nothing to free
	}
	defer release()
	syscall.SyscallN(proc, uintptr(unsafe.Pointer(ptr)), n)
}
