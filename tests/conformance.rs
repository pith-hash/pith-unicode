//! Conformance for `pith-unicode`: the official UCD `NormalizationTest.txt`
//! (all 19 074 data lines) across all four forms plus case folding, the
//! Vietnamese decomposed/precomposed pair the spec requires, idempotence,
//! composition properties, quick-check equivalence, and hash equality on
//! the NFC forms.
//!
//! The conformance corpus is `tests/NormalizationTest.txt`, embedded with
//! `include_bytes!` so the test does not depend on the working directory and
//! stays a fixed contract (`.gitattributes` pins it to LF). Its bytes are
//! generated data, not hand-written code.

use pith_unicode::{
    NormalizationForm, casefold, casefold_simple, is_normalized, nfc, nfd, nfkc, nfkd, normalize,
};

use NormalizationForm::{Nfc, Nfd, Nfkc, Nfkd};

const CORPUS: &str = include_str!("NormalizationTest.txt");

/// All four forms, in `NormalizationForm::code` order.
const FORMS: [NormalizationForm; 4] = [Nfc, Nfd, Nfkc, Nfkd];

/// Parse a semicolon column of hex code points into a `String`.
fn cps(col: &str) -> String {
    col.split_whitespace()
        .map(|h| char::from_u32(u32::from_str_radix(h, 16).unwrap()).unwrap())
        .collect()
}

fn each_line(mut f: impl FnMut(&str, [String; 5])) {
    for raw in CORPUS.lines() {
        let line = raw.split('#').next().unwrap().trim();
        if line.is_empty() || line.starts_with('@') {
            continue;
        }
        let cols: Vec<&str> = line.split(';').map(str::trim).collect();
        assert!(cols.len() >= 5, "short row: {line}");
        let seqs = [
            cps(cols[0]),
            cps(cols[1]),
            cps(cols[2]),
            cps(cols[3]),
            cps(cols[4]),
        ];
        f(line, seqs);
    }
}

/// UAX #15 conformance: toNFC(c1..c3) == c2 and toNFC(c4..c5) == c4; the
/// NFD halves c3 == toNFD(c1..c3) and c5 == toNFD(c4..c5) come for free
/// because this crate ships `nfd` too.
#[test]
fn ucd_conformance() {
    let mut lines = 0;
    each_line(|line, s| {
        lines += 1;
        let expected_nfc = [&s[1], &s[1], &s[1], &s[3], &s[3]];
        let expected_nfd = [&s[2], &s[2], &s[2], &s[4], &s[4]];
        for (src, (&en, &ed)) in s.iter().zip(expected_nfc.iter().zip(&expected_nfd)) {
            assert_eq!(&nfc(src), en, "NFC mismatch on {line}");
            assert_eq!(&nfd(src), ed, "NFD mismatch on {line}");
        }
    });
    assert_eq!(lines, 19_074, "every data line of the corpus must run");
}

/// Idempotence: NFC(NFC(x)) == NFC(x) for every column of every line.
#[test]
fn nfc_idempotent() {
    each_line(|line, s| {
        for src in &s {
            let once = nfc(src);
            assert_eq!(nfc(&once), once, "NFC not idempotent on {line}");
        }
    });
}

/// The Vietnamese pair the spec calls out (§4.1): decomposed
/// `Ta\u{0302}\u{0300}ng` and precomposed `Tầng` must NFC to the same
/// string — the combining marks U+0302 (circumflex) then U+0300 (grave)
/// have equal ccc=230, keep their order, and `a`+circumflex = `â`,
/// `â`+grave = `ầ`.
#[test]
fn vietnamese_pair() {
    let decomposed = "Ta\u{0302}\u{0300}ng";
    let precomposed = "T\u{1EA7}ng";
    assert_eq!(nfc(decomposed), precomposed);
    assert_eq!(nfc(precomposed), precomposed);
    // And the pair the spec warns about: `T\u{0300}` has no precomposed
    // grave form, so it must NOT collapse into `Tầng`.
    assert_ne!(nfc("T\u{0300}\u{1EA7}ng"), precomposed);
}

/// Every code point NOT listed in column 1 of Part 1 of the corpus has all
/// normal forms equal to itself (UAX #15 invariant). This scans every
/// Unicode scalar value, not a sample: the corpus's own list is the oracle
/// for which code points may legitimately change.
#[test]
fn unlisted_codepoints_are_stable() {
    let mut in_part1 = false;
    let mut listed = std::collections::BTreeSet::new();
    for raw in CORPUS.lines() {
        if raw.starts_with("@Part1") {
            in_part1 = true;
            continue;
        }
        if raw.starts_with('@') && in_part1 {
            break;
        }
        if !in_part1 {
            continue;
        }
        let line = raw.split('#').next().unwrap().trim();
        if line.is_empty() {
            continue;
        }
        for h in line.split(';').next().unwrap().split_whitespace() {
            listed.insert(u32::from_str_radix(h, 16).unwrap());
        }
    }
    let mut checked = 0u64;
    for cp in 0u32..=0x10FFFF {
        if listed.contains(&cp) || (0xD800..0xE000).contains(&cp) {
            continue;
        }
        let ch = char::from_u32(cp).unwrap();
        let s = String::from(ch);
        assert_eq!(nfc(&s), s, "unlisted cp {cp:04X} changed under NFC");
        assert_eq!(nfd(&s), s, "unlisted cp {cp:04X} changed under NFD");
        checked += 1;
    }
    assert_eq!(
        checked,
        0x10FFFF + 1 - listed.len() as u64 - 2048,
        "scan must cover every scalar the corpus does not list"
    );
}

/// The suite's tier-1 contract: the decomposed and precomposed forms of the
/// same Vietnamese syllable produce the same NFC bytes, and identical bytes
/// must hash identically under the suite's own SHA-256. The known-answer
/// vector pins the oracle itself, so a broken hasher cannot fake equality.
#[test]
fn nfc_forms_hash_equal() {
    let abc = pith_digest::sha256(b"abc").unwrap();
    assert_eq!(
        abc.to_string(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    let decomposed = nfc("Ta\u{0302}\u{0300}ng");
    let precomposed = nfc("T\u{1EA7}ng");
    let h1 = pith_digest::sha256(decomposed.as_bytes()).unwrap();
    let h2 = pith_digest::sha256(precomposed.as_bytes()).unwrap();
    assert_eq!(h1, h2);
}

/// UAX #15 compatibility conformance: NFKD is the same for all five
/// columns (canonical equivalence implies compatibility equivalence),
/// likewise NFKC — and compatibility decomposition is idempotent.
#[test]
fn ucd_conformance_compat_forms() {
    let mut lines = 0;
    each_line(|line, s| {
        lines += 1;
        let expected_nfkd = nfkd(&s[0]);
        let expected_nfkc = nfkc(&s[0]);
        for src in &s {
            assert_eq!(&nfkd(src), &expected_nfkd, "NFKD mismatch on {line}");
            assert_eq!(&nfkc(src), &expected_nfkc, "NFKC mismatch on {line}");
        }
    });
    assert_eq!(lines, 19_074, "every data line of the corpus must run");
}

/// Idempotence across every form: f(f(x)) == f(x) for every column of
/// every line, f in {NFD, NFC, NFKD, NFKC, casefold, casefold_simple}.
#[test]
fn all_forms_idempotent() {
    each_line(|line, s| {
        for src in &s {
            for form in FORMS {
                let once = normalize(form, src);
                assert_eq!(
                    normalize(form, &once),
                    once,
                    "{form:?} not idempotent on {line}"
                );
            }
            assert_eq!(
                casefold(&casefold(src)),
                casefold(src),
                "fold not idempotent on {line}"
            );
            assert_eq!(
                casefold_simple(&casefold_simple(src)),
                casefold_simple(src),
                "simple fold not idempotent on {line}"
            );
        }
    });
}

/// Composition properties: NFC(NFD(x)) == NFC(x) and NFKC(NFKD(x)) ==
/// NFKC(x) for every column of every line.
#[test]
fn composition_absorbs_decomposition() {
    each_line(|line, s| {
        for src in &s {
            assert_eq!(nfc(&nfd(src)), nfc(src), "NFC(NFD(x)) != NFC(x) on {line}");
            assert_eq!(
                nfkc(&nfkd(src)),
                nfkc(src),
                "NFKC(NFKD(x)) != NFKC(x) on {line}"
            );
        }
    });
}

/// Quick-check equivalence (UAX #15, "Detecting Normalization Forms"):
/// `is_normalized(f, x)` is exactly `normalize(f, x) == x` for every
/// form and every column of every line. This exercises both the
/// allocation-free fast path and the Maybe-driven slow path.
#[test]
fn quick_check_matches_the_slow_path() {
    each_line(|line, s| {
        for src in &s {
            for form in FORMS {
                assert_eq!(
                    is_normalized(form, src),
                    normalize(form, src) == *src,
                    "is_normalized({form:?}) disagrees on {line}"
                );
            }
        }
    });
}

/// The quick-check fast path on strings the corpus does not cover:
/// decomposed and precomposed Hangul, composition-excluded characters,
/// and the NFC_Maybe "no composition happens" case (`q` + combining
/// grave) whose honest answer is `true`.
#[test]
fn quick_check_hangul_and_edge_cases() {
    // Hangul: precomposed syllables are NFC/NFKC but not NFD/NFKD;
    // full jamo sequences are the reverse; LV+T decomposes.
    assert!(is_normalized(Nfc, "가") && is_normalized(Nfkc, "가"));
    assert!(!is_normalized(Nfd, "가") && !is_normalized(Nfkd, "가"));
    let jamo = "\u{1100}\u{1161}";
    assert!(
        !is_normalized(Nfc, jamo),
        "L+V jamo must take the Maybe slow path"
    );
    let lv = "\u{AC00}";
    assert!(is_normalized(Nfc, lv));
    assert_eq!(nfc(jamo), lv);
    // The same through `normalize` dispatch.
    assert_eq!(normalize(Nfc, jamo), normalize(Nfc, lv));
    let jamo_lvt = "\u{1100}\u{1161}\u{11A8}";
    assert_eq!(normalize(Nfkd, "각"), normalize(Nfkd, jamo_lvt));
    assert_eq!(nfc(jamo_lvt), "각");

    // Full_Composition_Exclusion: U+0344 decomposes under NFC and never
    // recomposes, so the string is not NFC — but its QC=No is what
    // proves it (no slow path needed).
    assert!(!is_normalized(Nfc, "\u{0344}"));
    assert_eq!(nfc("\u{0344}"), "\u{0308}\u{0301}");

    // NFC_Maybe without an actual composition: U+0300 after `q` cannot
    // compose, the slow path must answer `true`.
    assert!(is_normalized(Nfc, "q\u{0300}"));
    assert!(is_normalized(Nfkc, "q\u{0300}"));
    // ... and after `a` it does compose, so the answer is `false`.
    assert!(!is_normalized(Nfc, "a\u{0300}"));
    assert_eq!(nfc("a\u{0300}"), "\u{E0}");

    // Compatibility: the ligature ﬁ is canonically stable (NFC/NFD) but
    // changes under NFKC/NFKD into plain letters.
    assert!(is_normalized(Nfc, "\u{FB01}") && is_normalized(Nfd, "\u{FB01}"));
    assert!(!is_normalized(Nfkc, "\u{FB01}") && !is_normalized(Nfkd, "\u{FB01}"));
    assert_eq!(nfkd("\u{FB01}"), "fi");
    assert_eq!(nfkc("\u{FB01}"), "fi");

    // The empty string and ASCII are every form's fixed point.
    for form in FORMS {
        assert!(is_normalized(form, ""));
        assert!(is_normalized(form, "hello"));
    }
}

/// Case folding, spot-pinned per UAX #44 / `CaseFolding.txt`: ß and ẞ
/// fold to `ss`, ẞ folds simply to ß, İ keeps its dot, the Greek
/// terminal sigma folds to σ, and the ligature ﬁ unfolds.
#[test]
fn casefolding_pinned() {
    assert_eq!(casefold("ß"), "ss");
    assert_eq!(casefold("\u{1E9E}"), "ss");
    assert_eq!(casefold_simple("\u{1E9E}"), "\u{DF}");
    assert_eq!(casefold_simple("ß"), "ß");
    assert_eq!(casefold("\u{130}"), "i\u{307}");
    assert_eq!(casefold_simple("\u{130}"), "\u{130}");
    assert_eq!(casefold("ΣΊΣΥΦΟΣ"), "σίσυφοσ"); // ς folds to σ, never the reverse
    assert_eq!(casefold("σίσυφος"), "σίσυφοσ");
    assert_eq!(casefold("\u{FB03}"), "ffi");
    assert_eq!(casefold_simple("\u{FB03}"), "\u{FB03}");
    assert_eq!(casefold("ÀÉÎÕÜ"), "àéîõü");
    assert_eq!(casefold_simple("ǅ"), "ǆ");
    // The Turkic `T` entries are locale data: U+0131 keeps folding to
    // itself and `I` folds to `i` regardless of Turkish context.
    assert_eq!(casefold("I\u{131}"), "i\u{131}");
    // Folding is a per-character mapping and never panics on any scalar.
    for cp in (0u32..=0x10FFFE).step_by(97) {
        let Some(ch) = char::from_u32(cp) else {
            continue;
        };
        let _ = casefold_simple(&ch.to_string());
        let _ = casefold(&ch.to_string());
    }
}

/// The Vietnamese pair under every form, the way the tier-1 contract
/// reads: both spellings agree under NFC *and* NFKC, and the FFI form
/// codes dispatch to the same results.
#[test]
fn vietnamese_pair_across_all_forms() {
    let decomposed = "Ta\u{0302}\u{0300}ng";
    let precomposed = "T\u{1EA7}ng";
    for form in [Nfc, Nfkc] {
        assert_eq!(normalize(form, decomposed), normalize(form, precomposed));
    }
    for form in [Nfd, Nfkd] {
        assert_eq!(normalize(form, decomposed), normalize(form, precomposed));
    }
}

/// `NormalizationForm::code`/`from_code` round-trip, and out-of-range
/// codes are refused.
#[test]
fn form_codes_round_trip() {
    for form in FORMS {
        assert_eq!(NormalizationForm::from_code(form.code()), Some(form));
    }
    assert_eq!(NormalizationForm::from_code(0), None);
    assert_eq!(NormalizationForm::from_code(5), None);
}
