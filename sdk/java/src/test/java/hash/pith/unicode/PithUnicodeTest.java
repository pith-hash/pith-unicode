// SPDX-License-Identifier: MIT
// Copyright (c) 2026 pith-hash
package hash.pith.unicode;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.Arrays;
import org.junit.jupiter.api.Test;

/**
 * Hex-exact conformance: the committed {@code reference.json} vectors
 * through JNI — the same vectors the Rust {@code gen-reference verify}
 * gate and the Python/Node/Go SDK suites replay, plus Rust-derived
 * literal pins and the refusal paths (never a crash).
 *
 * <p>The replay is the FULL set: the 952 historical NFC/NFD rows, all
 * {@code form_vectors} (the same rows the Python/Node/Go suites
 * replay, through all four forms with a quick-check cross-check per
 * row) and all {@code casefold_vectors} (full + simple folding). The
 * suite runs in well under a second of native time (measured: the
 * whole JUnit run is dominated by JVM startup, not by the ~30k JNI
 * calls), so nothing is sampled away.</p>
 */
class PithUnicodeTest {

    private static final Path REPO_ROOT = findRepoRoot();

    /**
     * The repo root: the nearest ancestor (or the working directory
     * itself) holding {@code reference.json} — works both for
     * {@code mvn test} from {@code sdk/java} and from the repo root.
     */
    private static Path findRepoRoot() {
        Path dir = Paths.get("").toAbsolutePath();
        for (int up = 0; up <= 6 && dir != null; up++) {
            if (Files.isRegularFile(dir.resolve("reference.json"))) {
                return dir;
            }
            dir = dir.getParent();
        }
        throw new IllegalStateException(
                "no reference.json found at or above " + Paths.get("").toAbsolutePath());
    }

    private static JsonNode reference() throws Exception {
        return new ObjectMapper().readTree(REPO_ROOT.resolve("reference.json").toFile());
    }

    private static byte[] hex(String value) {
        int len = value.length();
        byte[] out = new byte[len / 2];
        for (int i = 0; i < out.length; i++) {
            out[i] = (byte) Integer.parseInt(value.substring(2 * i, 2 * i + 2), 16);
        }
        return out;
    }

    @Test
    void cdylib_is_discoverable() {
        assertTrue(Files.isRegularFile(Paths.get(PithUnicode.cdylibPath())),
                PithUnicode.cdylibPath());
    }

    @Test
    void every_historical_vector_is_reproduced_hex_exact() throws Exception {
        JsonNode vectors = reference().get("vectors");
        for (int i = 0; i < vectors.size(); i++) {
            JsonNode vector = vectors.get(i);
            byte[] data = hex(vector.get(0).asText());
            assertArrayEquals(hex(vector.get(1).asText()), PithUnicode.nfc(data),
                    "vector " + i + " nfc");
            assertArrayEquals(hex(vector.get(2).asText()), PithUnicode.nfd(data),
                    "vector " + i + " nfd");
        }
        assertEquals(952, vectors.size(), "the historical set is complete");
    }

    @Test
    void every_form_vector_is_reproduced_hex_exact() throws Exception {
        JsonNode vectors = reference().get("form_vectors");
        long started = System.nanoTime();
        for (int i = 0; i < vectors.size(); i++) {
            JsonNode vector = vectors.get(i);
            byte[] data = hex(vector.get(0).asText());
            byte[] nfc = PithUnicode.nfc(data);
            byte[] nfd = PithUnicode.nfd(data);
            byte[] nfkc = PithUnicode.nfkc(data);
            byte[] nfkd = PithUnicode.nfkd(data);
            assertArrayEquals(hex(vector.get(1).asText()), nfc, "form vector " + i + " nfc");
            assertArrayEquals(hex(vector.get(2).asText()), nfd, "form vector " + i + " nfd");
            assertArrayEquals(hex(vector.get(3).asText()), nfkc, "form vector " + i + " nfkc");
            assertArrayEquals(hex(vector.get(4).asText()), nfkd, "form vector " + i + " nfkd");
            // The dispatch answers byte-identically through every form
            // code, and the quick check is the exact slow-path truth:
            // the form is a fixed point iff normalization is the
            // identity on the input.
            assertArrayEquals(nfc, PithUnicode.normalize(PithUnicode.FORM_NFC, data), "form " + i);
            assertArrayEquals(nfd, PithUnicode.normalize(PithUnicode.FORM_NFD, data), "form " + i);
            assertArrayEquals(nfkc, PithUnicode.normalize(PithUnicode.FORM_NFKC, data), "form " + i);
            assertArrayEquals(nfkd, PithUnicode.normalize(PithUnicode.FORM_NFKD, data), "form " + i);
            assertEquals(Arrays.equals(data, nfc), PithUnicode.isNormalized(PithUnicode.FORM_NFC, data),
                    "form vector " + i + " isNormalized NFC");
            assertEquals(Arrays.equals(data, nfd), PithUnicode.isNormalized(PithUnicode.FORM_NFD, data),
                    "form vector " + i + " isNormalized NFD");
            assertEquals(Arrays.equals(data, nfkc), PithUnicode.isNormalized(PithUnicode.FORM_NFKC, data),
                    "form vector " + i + " isNormalized NFKC");
            assertEquals(Arrays.equals(data, nfkd), PithUnicode.isNormalized(PithUnicode.FORM_NFKD, data),
                    "form vector " + i + " isNormalized NFKD");
        }
        long replayMillis = (System.nanoTime() - started) / 1_000_000L;
        // Recorded so a future slowdown is visible in CI logs: the full
        // replay (this is the measured number the "full set" decision
        // rests on) is native-bound and must stay far below the
        // surefire default timeout.
        assertTrue(replayMillis < 60_000, "form-vector replay took " + replayMillis + " ms");
        assertEquals(2387, vectors.size(), "the form-vector set is complete");
    }

    @Test
    void every_casefold_vector_is_reproduced_hex_exact() throws Exception {
        JsonNode vectors = reference().get("casefold_vectors");
        for (int i = 0; i < vectors.size(); i++) {
            JsonNode vector = vectors.get(i);
            byte[] data = hex(vector.get(0).asText());
            assertArrayEquals(hex(vector.get(1).asText()), PithUnicode.casefold(data),
                    "fold vector " + i + " full");
            assertArrayEquals(hex(vector.get(2).asText()), PithUnicode.casefoldSimple(data),
                    "fold vector " + i + " simple");
        }
        assertEquals(103, vectors.size(), "the casefold set is complete");
    }

    @Test
    void rust_pinned_literals_are_reproduced() {
        // reference.json vectors[0] and its final vector, pinned in the
        // Rust unit tests too; fails loudly even if reference.json were
        // regenerated wrongly.
        assertArrayEquals(hex("e1b88a"), PithUnicode.nfc(hex("e1b88a")));
        assertArrayEquals(hex("44cc87"), PithUnicode.nfd(hex("e1b88a")));
        assertArrayEquals(hex("54e1baa76e67"), PithUnicode.nfc(hex("5461cc82cc806e67")));
        assertArrayEquals(hex("5461cc82cc806e67"), PithUnicode.nfd(hex("5461cc82cc806e67")));
        // The tier-1 pins the Rust unit tests carry.
        assertArrayEquals("fi".getBytes(), PithUnicode.nfkc(hex("efac81")));
        assertArrayEquals("fi".getBytes(), PithUnicode.nfkd(hex("efac81")));
        assertTrue(PithUnicode.isNormalized(PithUnicode.FORM_NFC, hex("efac81")));
        assertTrue(!PithUnicode.isNormalized(PithUnicode.FORM_NFKC, hex("efac81")));
        assertArrayEquals("ss".getBytes(), PithUnicode.casefold(hex("c39f")));
        assertArrayEquals(hex("c39f"), PithUnicode.casefoldSimple(hex("e1ba9e")));
        assertArrayEquals("ss".getBytes(), PithUnicode.casefold(hex("e1ba9e")));
    }

    @Test
    void invalid_utf8_is_refused_not_crashing() {
        byte[] invalid = {(byte) 0xff, (byte) 0xfe};
        String[] ops = {"pith_unicode_nfc", "pith_unicode_nfd", "pith_unicode_nfkc",
            "pith_unicode_nfkd", "pith_unicode_casefold", "pith_unicode_casefold_simple"};
        for (String op : ops) {
            PithUnicode.FfiError err = assertThrows(PithUnicode.FfiError.class,
                    () -> callByName(op, invalid));
            assertEquals(PithUnicode.PITH_E_REJECTED, err.status, op);
            assertEquals(op, err.op);
        }
        PithUnicode.FfiError err = assertThrows(PithUnicode.FfiError.class,
                () -> PithUnicode.isNormalized(PithUnicode.FORM_NFC, invalid));
        assertEquals(PithUnicode.PITH_E_REJECTED, err.status);
    }

    private static byte[] callByName(String op, byte[] data) {
        switch (op) {
            case "pith_unicode_nfc":
                return PithUnicode.nfc(data);
            case "pith_unicode_nfd":
                return PithUnicode.nfd(data);
            case "pith_unicode_nfkc":
                return PithUnicode.nfkc(data);
            case "pith_unicode_nfkd":
                return PithUnicode.nfkd(data);
            case "pith_unicode_casefold":
                return PithUnicode.casefold(data);
            case "pith_unicode_casefold_simple":
                return PithUnicode.casefoldSimple(data);
            default:
                throw new IllegalArgumentException(op);
        }
    }

    @Test
    void null_data_is_refused_not_crashing() {
        PithUnicode.FfiError err = assertThrows(PithUnicode.FfiError.class,
                () -> PithUnicode.nfc(null));
        assertEquals(PithUnicode.PITH_E_INVALID, err.status);
        assertEquals("pith_unicode_nfc", err.op);
        err = assertThrows(PithUnicode.FfiError.class, () -> PithUnicode.nfd(null));
        assertEquals(PithUnicode.PITH_E_INVALID, err.status);
        err = assertThrows(PithUnicode.FfiError.class, () -> PithUnicode.nfkc(null));
        assertEquals(PithUnicode.PITH_E_INVALID, err.status);
        err = assertThrows(PithUnicode.FfiError.class, () -> PithUnicode.nfkd(null));
        assertEquals(PithUnicode.PITH_E_INVALID, err.status);
        err = assertThrows(PithUnicode.FfiError.class, () -> PithUnicode.casefold(null));
        assertEquals(PithUnicode.PITH_E_INVALID, err.status);
        err = assertThrows(PithUnicode.FfiError.class, () -> PithUnicode.casefoldSimple(null));
        assertEquals(PithUnicode.PITH_E_INVALID, err.status);
        err = assertThrows(PithUnicode.FfiError.class,
                () -> PithUnicode.isNormalized(PithUnicode.FORM_NFC, null));
        assertEquals(PithUnicode.PITH_E_INVALID, err.status);
    }

    @Test
    void unknown_form_is_refused_client_side() {
        assertThrows(IllegalArgumentException.class, () -> PithUnicode.normalize(0, new byte[1]));
        assertThrows(IllegalArgumentException.class, () -> PithUnicode.normalize(5, new byte[1]));
        assertThrows(IllegalArgumentException.class,
                () -> PithUnicode.isNormalized(99, new byte[1]));
    }

    @Test
    void empty_input_is_valid_and_round_trips() {
        byte[] empty = new byte[0];
        assertEquals(0, PithUnicode.nfc(empty).length);
        assertEquals(0, PithUnicode.nfd(empty).length);
        assertEquals(0, PithUnicode.nfkc(empty).length);
        assertEquals(0, PithUnicode.nfkd(empty).length);
        assertEquals(0, PithUnicode.casefold(empty).length);
        assertEquals(0, PithUnicode.casefoldSimple(empty).length);
        assertTrue(PithUnicode.isNormalized(PithUnicode.FORM_NFC, empty));
        assertTrue(PithUnicode.isNormalized(PithUnicode.FORM_NFD, empty));
        assertTrue(PithUnicode.isNormalized(PithUnicode.FORM_NFKC, empty));
        assertTrue(PithUnicode.isNormalized(PithUnicode.FORM_NFKD, empty));
    }
}
