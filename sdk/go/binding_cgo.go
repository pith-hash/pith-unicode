// SPDX-License-Identifier: MIT
// Copyright (c) 2026 pith-hash

//go:build !windows && cgo

package pithunicode

/*
#include <dlfcn.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>

typedef int32_t (*pith_norm_fn)(const uint8_t *, size_t, uint8_t **, size_t *);
typedef void (*pith_free_fn)(uint8_t *, size_t);

static int32_t pith_call_norm(void *fn, const uint8_t *data, size_t len,
                              uint8_t **out, size_t *out_len) {
    return ((pith_norm_fn)fn)(data, len, out, out_len);
}

static void pith_call_free(void *fn, uint8_t *ptr, size_t len) {
    ((pith_free_fn)fn)(ptr, len);
}
*/
import "C"

import (
	"fmt"
	"unsafe"
)

// openCdylib dlopens libPath with error text surfaced verbatim.
func openCdylib(libPath string) (unsafe.Pointer, error) {
	cPath := C.CString(libPath)
	defer C.free(unsafe.Pointer(cPath))
	handle := C.dlopen(cPath, C.RTLD_NOW|C.RTLD_LOCAL)
	if handle == nil {
		msg := "unknown dlopen failure"
		if e := C.dlerror(); e != nil {
			msg = C.GoString(e)
		}
		return nil, fmt.Errorf("pithunicode: dlopen(%s): %s", libPath, msg)
	}
	return handle, nil
}

// resolveSymbol dlsyms one name off an open handle.
func resolveSymbol(handle unsafe.Pointer, libPath, name string) (unsafe.Pointer, error) {
	cName := C.CString(name)
	sym := C.dlsym(handle, cName)
	C.free(unsafe.Pointer(cName))
	if sym == nil {
		return nil, fmt.Errorf("pithunicode: symbol %s missing from %s", name, libPath)
	}
	return sym, nil
}

// ffiCall opens the cdylib, resolves opName and calls it. data may be
// nil (an empty input); out receives the handed-out buffer, outLen its
// exact length. The handle is released before returning; repeated
// calls reuse the loader's own refcount.
func ffiCall(libPath, opName string, data *byte, n int, out **byte, outLen *uintptr) (int32, error) {
	handle, err := openCdylib(libPath)
	if err != nil {
		return 0, err
	}
	defer C.dlclose(handle)

	sym, err := resolveSymbol(handle, libPath, opName)
	if err != nil {
		return 0, err
	}
	var cOut *C.uint8_t
	var cLen C.size_t
	rc := C.pith_call_norm(sym, (*C.uint8_t)(unsafe.Pointer(data)), C.size_t(n), &cOut, &cLen)
	*out = (*byte)(unsafe.Pointer(cOut))
	*outLen = uintptr(cLen)
	return int32(rc), nil
}

// ffiFree releases a buffer handed out by ffiCall. Null is accepted
// (the cdylib ignores it), matching the C contract.
func ffiFree(libPath string, ptr *byte, n uintptr) {
	handle, err := openCdylib(libPath)
	if err != nil {
		return // the library vanished mid-flight; nothing to free
	}
	defer C.dlclose(handle)
	sym, err := resolveSymbol(handle, libPath, "pith_unicode_free")
	if err != nil {
		return
	}
	C.pith_call_free(sym, (*C.uint8_t)(unsafe.Pointer(ptr)), C.size_t(n))
}
