// SPDX-License-Identifier: MIT
// Copyright (c) 2026 pith-hash
package hash.pith.unicode;

import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;

/**
 * Java JNI bindings for the {@code pith-unicode} cdylib: Unicode
 * normalisation (NFC, NFD, NFKC, NFKD), quick-check detection and case
 * folding — the same C library the Python (ctypes), Node (koffi) and
 * Go (cgo/syscall) SDKs bind through.
 *
 * <p>The cdylib is resolved once at class-load time, mirroring the
 * discovery chain of the other SDKs: (1) the {@code PITH_CDYLIB}
 * environment variable — the explicit file; (2) {@code PITH_CDYLIB_DIR}
 * — a directory holding one of the platform library names; (3) a
 * {@code target/release} directory at the working directory or up to
 * six ancestors above it. {@link LinkageError} names every candidate
 * when nothing matches.</p>
 *
 * <p>Normalization results are fresh {@code byte[]} copies of UTF-8 —
 * the handed-out cdylib buffer is copied into the Java array and
 * released before returning. Invalid UTF-8 raises {@link FfiError},
 * carrying the C ABI status code; an unknown form code raises
 * {@link IllegalArgumentException} (refused client-side, mirroring the
 * Python/Node SDKs).</p>
 */
public final class PithUnicode {

    /** Status: success. */
    public static final int PITH_OK = 0;

    /** Status: invalid argument — a null array or an unknown form code. */
    public static final int PITH_E_INVALID = -1;

    /** Status: the core refused the input (the bytes are not valid UTF-8). */
    public static final int PITH_E_REJECTED = -2;

    /** Normalization form code: canonical composition (UAX #15 NFC). */
    public static final int FORM_NFC = 1;

    /** Normalization form code: canonical decomposition (UAX #15 NFD). */
    public static final int FORM_NFD = 2;

    /** Normalization form code: compatibility composition (UAX #15 NFKC). */
    public static final int FORM_NFKC = 3;

    /** Normalization form code: compatibility decomposition (UAX #15 NFKD). */
    public static final int FORM_NFKD = 4;

    /** Platform cdylib file names, in probe order. */
    private static final String[] CDYLIB_NAMES = {
        "pith_unicode.dll", "libpith_unicode.so", "libpith_unicode.dylib",
    };

    private static final String CDYLIB_PATH = findCdylib();

    static {
        System.load(CDYLIB_PATH);
    }

    private PithUnicode() {
    }

    /** The absolute path of the loaded cdylib (tests and diagnostics). */
    public static String cdylibPath() {
        return CDYLIB_PATH;
    }

    private static native byte[] nfcNative(byte[] data, int[] status);

    private static native byte[] nfdNative(byte[] data, int[] status);

    private static native byte[] nfkcNative(byte[] data, int[] status);

    private static native byte[] nfkdNative(byte[] data, int[] status);

    private static native byte[] casefoldNative(byte[] data, int[] status);

    private static native byte[] casefoldSimpleNative(byte[] data, int[] status);

    private static native int isNormalizedNative(int form, byte[] data, int[] status);

    /** A fresh one-element status slot for a native call. */
    private static int[] statusSlot() {
        return new int[1];
    }

    /**
     * Normalizes {@code data} to Unicode Normalization Form C (canonical
     * composition) and returns the UTF-8 result as a fresh copy. Empty
     * input is valid and returns an empty array.
     *
     * @throws FfiError with status {@code PITH_E_REJECTED} for bytes
     *     that are not valid UTF-8 — never a crash
     */
    public static byte[] nfc(byte[] data) {
        int[] status = statusSlot();
        byte[] out = nfcNative(data, status);
        if (status[0] != PITH_OK) {
            throw new FfiError("pith_unicode_nfc", status[0]);
        }
        return out;
    }

    /**
     * Normalizes {@code data} to Unicode Normalization Form D (canonical
     * decomposition), with the same contract as {@link #nfc}.
     */
    public static byte[] nfd(byte[] data) {
        int[] status = statusSlot();
        byte[] out = nfdNative(data, status);
        if (status[0] != PITH_OK) {
            throw new FfiError("pith_unicode_nfd", status[0]);
        }
        return out;
    }

    /**
     * Normalizes {@code data} to Unicode Normalization Form KC
     * (compatibility composition), with the same contract as
     * {@link #nfc}.
     */
    public static byte[] nfkc(byte[] data) {
        int[] status = statusSlot();
        byte[] out = nfkcNative(data, status);
        if (status[0] != PITH_OK) {
            throw new FfiError("pith_unicode_nfkc", status[0]);
        }
        return out;
    }

    /**
     * Normalizes {@code data} to Unicode Normalization Form KD
     * (compatibility decomposition), with the same contract as
     * {@link #nfc}.
     */
    public static byte[] nfkd(byte[] data) {
        int[] status = statusSlot();
        byte[] out = nfkdNative(data, status);
        if (status[0] != PITH_OK) {
            throw new FfiError("pith_unicode_nfkd", status[0]);
        }
        return out;
    }

    /**
     * Normalizes {@code data} to {@code form} — one of the
     * {@code FORM_NFC} … {@code FORM_NFKD} codes — with the same
     * contract as {@link #nfc}.
     *
     * @throws IllegalArgumentException for an unknown form code
     */
    public static byte[] normalize(int form, byte[] data) {
        switch (form) {
            case FORM_NFC:
                return nfc(data);
            case FORM_NFD:
                return nfd(data);
            case FORM_NFKC:
                return nfkc(data);
            case FORM_NFKD:
                return nfkd(data);
            default:
                throw new IllegalArgumentException("unknown normalization form code: " + form);
        }
    }

    /**
     * Answers whether {@code data} is already in normalization
     * {@code form} — the exact UAX #15 quick-check answer, never a
     * pessimistic approximation.
     *
     * @throws IllegalArgumentException for an unknown form code
     * @throws FfiError for bytes that are not valid UTF-8
     */
    public static boolean isNormalized(int form, byte[] data) {
        if (form < FORM_NFC || form > FORM_NFKD) {
            throw new IllegalArgumentException("unknown normalization form code: " + form);
        }
        int[] status = statusSlot();
        int answer = isNormalizedNative(form, data, status);
        if (status[0] != PITH_OK) {
            throw new FfiError("pith_unicode_is_normalized", status[0]);
        }
        return answer == 1;
    }

    /**
     * Folds {@code data} to its full case folding (UAX #44 statuses
     * {@code C}+{@code F}; mappings may expand, e.g. ß → ss). The
     * Turkic {@code T} entries are locale data and are not applied.
     * Same contract as {@link #nfc}.
     */
    public static byte[] casefold(byte[] data) {
        int[] status = statusSlot();
        byte[] out = casefoldNative(data, status);
        if (status[0] != PITH_OK) {
            throw new FfiError("pith_unicode_casefold", status[0]);
        }
        return out;
    }

    /**
     * Folds {@code data} to its simple case folding (statuses
     * {@code C}+{@code S}): a strict one-to-one mapping (ẞ → ß,
     * ß → itself).
     */
    public static byte[] casefoldSimple(byte[] data) {
        int[] status = statusSlot();
        byte[] out = casefoldSimpleNative(data, status);
        if (status[0] != PITH_OK) {
            throw new FfiError("pith_unicode_casefold_simple", status[0]);
        }
        return out;
    }

    /**
     * A native call refused or failed: the C ABI status code plus the
     * operation that reported it — the Java face of the ctypes/koffi/
     * cgo {@code FfiError}.
     */
    public static final class FfiError extends RuntimeException {
        private static final long serialVersionUID = 1L;

        /** The refusing operation (its C ABI name). */
        public final String op;

        /** The C ABI status code ({@code -1} invalid, {@code -2} rejected). */
        public final int status;

        FfiError(String op, int status) {
            super(op + " failed with status " + status);
            this.op = op;
            this.status = status;
        }
    }

    /**
     * Resolves the cdylib path: {@code PITH_CDYLIB} (explicit file),
     * then {@code PITH_CDYLIB_DIR} (a directory holding one of the
     * platform names), then {@code target/release} at the working
     * directory and up to six ancestors — the same chain as the other
     * SDKs.
     */
    private static String findCdylib() {
        String explicit = System.getenv("PITH_CDYLIB");
        if (explicit != null && !explicit.isEmpty()) {
            return explicit;
        }

        String dir = System.getenv("PITH_CDYLIB_DIR");
        if (dir != null && !dir.isEmpty()) {
            Path base = Paths.get(dir).toAbsolutePath();
            for (String name : CDYLIB_NAMES) {
                Path candidate = base.resolve(name);
                if (Files.isRegularFile(candidate)) {
                    return candidate.toString();
                }
            }
        }

        Path cwd = Paths.get("").toAbsolutePath();
        for (Path base = cwd; base != null; base = base.getParent()) {
            for (String name : CDYLIB_NAMES) {
                Path candidate = base.resolve("target").resolve("release").resolve(name);
                if (Files.isRegularFile(candidate)) {
                    return candidate.toString();
                }
            }
        }

        throw new LinkageError(
                "cannot locate the pith_unicode cdylib; set PITH_CDYLIB or PITH_CDYLIB_DIR"
                + " (probed PITH_CDYLIB, PITH_CDYLIB_DIR, and target/release at "
                + cwd + " and its ancestors)");
    }
}
