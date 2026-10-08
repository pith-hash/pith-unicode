//! `gen-reference` — regenerate and verify the hex-exact vectors in
//! `reference.json`, the cross-SDK conformance artifact of `pith-unicode`.
//!
//! Modes:
//!
//! - `gen [PATH]`  write `reference.json` (default: `<repo root>/reference.json`)
//! - `verify [PATH]`  recompute every vector and byte-compare the JSON against
//!   the committed file; exit 1 on any drift
//!
//! The vector inputs are sampled deterministically from the UCD 15.1.0
//! conformance corpus `tests/NormalizationTest.txt` (embedded, no working
//! directory dependence), plus curated edge cases. Expected outputs are
//! recomputed by `pith_unicode` itself; `tests/conformance.rs` keeps the
//! external-oracle role by checking the full 19 074-line corpus directly.
//!
//! The file carries three sections:
//!
//! - `vectors` — the historical `[input, NFC, NFD]` rows: every 16th data
//!   line whose canonical normalization changes, plus the Vietnamese pair.
//!   This section is frozen: its sampling logic and byte layout never
//!   change, so rows 0..951 stay byte-identical forever.
//! - `form_vectors` — `[input, NFC, NFD, NFKC, NFKD]` rows: every 8th data
//!   line whose compatibility normalization changes, Hangul and quick-check
//!   edge cases, and the Vietnamese pair again.
//! - `casefold_vectors` — `[input, full fold, simple fold]` rows: every 8th
//!   data line whose folding changes, plus curated folding pins.

use std::process::ExitCode;

use pith_unicode::{casefold, casefold_simple, nfc, nfd, nfkc, nfkd};

const CORPUS: &str = include_str!("../../tests/NormalizationTest.txt");
#[cfg(test)]
const COMMITTED: &str = include_str!("../../reference.json");
const DEFAULT_PATH: &str = "reference.json";
/// Frozen stride of the historical `vectors` section.
const STRIDE: usize = 16;
/// Stride of the tier-1 `form_vectors` / `casefold_vectors` sections.
const FORM_STRIDE: usize = 8;

/// One historical hex-exact vector: input bytes, NFC and NFD output bytes.
struct Vector {
    input: String,
    nfc: String,
    nfd: String,
}

/// One tier-1 vector: input bytes and all four normalization forms.
struct FormVector {
    input: String,
    nfc: String,
    nfd: String,
    nfkc: String,
    nfkd: String,
}

/// One case folding vector: input bytes, full and simple folding.
struct FoldVector {
    input: String,
    full: String,
    simple: String,
}

/// Parse a semicolon column of hex code points into a `String`.
fn cps(col: &str) -> String {
    col.split_whitespace()
        .map(|h| {
            char::from_u32(u32::from_str_radix(h, 16).expect("corpus hex")).expect("corpus scalar")
        })
        .collect()
}

/// The data lines of the corpus, stripped of comments and directives.
fn corpus_lines() -> impl Iterator<Item = String> {
    CORPUS.lines().filter_map(|raw| {
        let line = raw.split('#').next().unwrap_or("").trim();
        (!line.is_empty() && !line.starts_with('@')).then(|| line.to_owned())
    })
}

/// The deterministic historical vector set: every [`STRIDE`]-th corpus data
/// line that canonical normalization actually changes, plus the Vietnamese
/// pair. Frozen — the sampled rows must never change.
fn vectors() -> Vec<Vector> {
    let mut out: Vec<Vector> = Vec::new();
    for (index, line) in corpus_lines().enumerate() {
        if index.rem_euclid(STRIDE) == 0 {
            let cols: Vec<&str> = line.split(';').map(str::trim).collect();
            let input = cps(cols[0]);
            let v = Vector {
                nfc: nfc(&input),
                nfd: nfd(&input),
                input,
            };
            if v.nfc != v.input || v.nfd != v.input {
                out.push(v);
            }
        }
    }
    // The spec-called-out Vietnamese pair, always present regardless of the
    // stride sampling.
    let decomposed = "Ta\u{0302}\u{0300}ng";
    out.push(Vector {
        input: decomposed.to_owned(),
        nfc: nfc(decomposed),
        nfd: nfd(decomposed),
    });
    out
}

/// Curated tier-1 inputs: the Hangul jamo/syllable ladder, quick-check
/// edge cases, compatibility families, and the Vietnamese pair.
fn curated_form_inputs() -> Vec<String> {
    vec![
        "Ta\u{0302}\u{0300}ng".to_owned(),
        "T\u{1EA7}ng".to_owned(),
        "\u{AC00}".to_owned(),                 // 가 LV syllable
        "\u{1100}\u{1161}".to_owned(),         // L+V jamo
        "\u{AC01}".to_owned(),                 // 각 LV+T syllable
        "\u{1100}\u{1161}\u{11A8}".to_owned(), // L+V+T jamo
        "q\u{0300}".to_owned(),                // NFC_Maybe, no composition
        "a\u{0300}".to_owned(),                // NFC_Maybe, composes
        "\u{0344}".to_owned(),                 // Full_Composition_Exclusion
        "\u{FB01}".to_owned(),                 // ﬁ ligature (compat)
        "\u{FDFA}".to_owned(),                 // 18-character compat chain
        "\u{00C5}".to_owned(),                 // Å ring above
        "q\u{0323}\u{0301}".to_owned(),        // ordering: ccc 220 then 230
    ]
}

/// The tier-1 normalization vectors: every [`FORM_STRIDE`]-th corpus data
/// line whose compatibility normalization changes, plus curated cases.
fn form_vectors() -> Vec<FormVector> {
    let mut out: Vec<FormVector> = Vec::new();
    for (index, line) in corpus_lines().enumerate() {
        if index.rem_euclid(FORM_STRIDE) == 0 {
            let input = cps(line.split(';').next().unwrap_or(""));
            let v = FormVector {
                nfc: nfc(&input),
                nfd: nfd(&input),
                nfkc: nfkc(&input),
                nfkd: nfkd(&input),
                input,
            };
            if v.nfkc != v.input || v.nfkd != v.input {
                out.push(v);
            }
        }
    }
    for input in curated_form_inputs() {
        out.push(FormVector {
            nfc: nfc(&input),
            nfd: nfd(&input),
            nfkc: nfkc(&input),
            nfkd: nfkd(&input),
            input,
        });
    }
    out
}

/// Curated case folding inputs: the ß/ẞ family, dotted/dotless I, Greek
/// sigma, title-case digraphs, and ligatures.
fn curated_fold_inputs() -> Vec<String> {
    vec![
        "\u{DF}".to_owned(),          // ß → ss full, ß simple
        "\u{1E9E}".to_owned(),        // ẞ → ss full, ß simple
        "\u{130}".to_owned(),         // İ → i + combining dot
        "I\u{131}".to_owned(),        // I and dotless ı
        "\u{FB03}".to_owned(),        // ﬃ ligature
        "\u{1C5}".to_owned(),         // ǅ title-case digraph
        "\u{1F2}".to_owned(),         // ͅ-carrying Greek
        "\u{1FBC}".to_owned(),        // Greek prosgegrammeni
        "ΣΊΣΥΦΟΣ".to_owned(),         // capital Greek
        "σίσυφος".to_owned(),         // final sigma folds to σ
        "ABC def".to_owned(),         // plain ASCII
        "ÀÉÎÕÜ a\u{0300}".to_owned(), // precomposed + decomposed
    ]
}

/// The tier-1 case folding vectors: every [`FORM_STRIDE`]-th corpus data
/// line whose folding changes, plus curated cases.
fn fold_vectors() -> Vec<FoldVector> {
    let mut out: Vec<FoldVector> = Vec::new();
    for (index, line) in corpus_lines().enumerate() {
        if index.rem_euclid(FORM_STRIDE) == 0 {
            let input = cps(line.split(';').next().unwrap_or(""));
            let v = FoldVector {
                full: casefold(&input),
                simple: casefold_simple(&input),
                input,
            };
            if v.full != v.input || v.simple != v.input {
                out.push(v);
            }
        }
    }
    for input in curated_fold_inputs() {
        out.push(FoldVector {
            full: casefold(&input),
            simple: casefold_simple(&input),
            input,
        });
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(char::from_digit(u32::from(b >> 4), 16).expect("hex digit"));
        s.push(char::from_digit(u32::from(b & 0xf), 16).expect("hex digit"));
    }
    s
}

fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Serialize the vector set as stable JSON: fixed field order, LF newlines,
/// trailing newline, lowercase hex. Byte-stable across runs and platforms.
/// The `vectors` section layout is frozen (see the module docs).
fn render(vs: &[Vector], fs: &[FormVector], fds: &[FoldVector]) -> String {
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str("  \"format\": 1,\n");
    out.push_str("  \"generator\": \"cargo run --bin gen-reference -- gen\",\n");
    out.push_str("  \"description\": \"Hex-exact NFC/NFD vectors for pith-unicode: UTF-8 bytes of the input string and of its NFC and NFD forms. Inputs are every 16th line of the UCD 15.1.0 NormalizationTest corpus (keeping only lines whose normalization changes) plus the Vietnamese decomposed/precomposed pair.\",\n");
    out.push_str("  \"vectors\": [\n");
    for (i, v) in vs.iter().enumerate() {
        out.push_str("    [");
        out.push_str(&json_string(&hex(v.input.as_bytes())));
        out.push_str(", ");
        out.push_str(&json_string(&hex(v.nfc.as_bytes())));
        out.push_str(", ");
        out.push_str(&json_string(&hex(v.nfd.as_bytes())));
        out.push(']');
        if i + 1 < vs.len() {
            out.push(',');
        }
        out.push('\n');
    }
    // Tier-1 sections. Appended after the frozen `vectors` section; their
    // own rows are new, and their presence never rewrites the rows above.
    out.push_str("  ],\n");
    out.push_str("  \"form_vectors\": [\n");
    for (i, v) in fs.iter().enumerate() {
        out.push_str("    [");
        for (j, field) in [&v.input, &v.nfc, &v.nfd, &v.nfkc, &v.nfkd]
            .iter()
            .enumerate()
        {
            if j > 0 {
                out.push_str(", ");
            }
            out.push_str(&json_string(&hex(field.as_bytes())));
        }
        out.push(']');
        if i + 1 < fs.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ],\n");
    out.push_str("  \"casefold_vectors\": [\n");
    for (i, v) in fds.iter().enumerate() {
        out.push_str("    [");
        for (j, field) in [&v.input, &v.full, &v.simple].iter().enumerate() {
            if j > 0 {
                out.push_str(", ");
            }
            out.push_str(&json_string(&hex(field.as_bytes())));
        }
        out.push(']');
        if i + 1 < fds.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ]\n");
    out.push_str("}\n");
    out
}

fn run(args: &[String]) -> Result<(), String> {
    let (mode, path) = match args {
        [m] if m == "gen" || m == "verify" => (m.as_str(), DEFAULT_PATH),
        [m, p] if m == "gen" || m == "verify" => (m.as_str(), p.as_str()),
        _ => {
            return Err("usage: gen-reference <gen|verify> [path]".to_owned());
        }
    };
    let fresh = render(&vectors(), &form_vectors(), &fold_vectors());
    if mode == "gen" {
        std::fs::write(path, fresh.as_bytes()).map_err(|e| format!("cannot write {path}: {e}"))?;
        println!("wrote {path} ({} bytes)", fresh.len());
        return Ok(());
    }
    let committed = std::fs::read(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    if committed == fresh.as_bytes() {
        println!("verify OK: reference.json is current");
        Ok(())
    } else {
        Err(format!(
            "{path} is stale: run `cargo run --bin gen-reference -- gen` and inspect the diff"
        ))
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_stable_json_with_expected_shape() {
        let vs = vectors();
        let fs = form_vectors();
        let fds = fold_vectors();
        assert!(
            vs.len() > 500,
            "stride sampling must keep a real corpus slice"
        );
        assert!(!fs.is_empty() && !fds.is_empty());
        let json = render(&vs, &fs, &fds);
        assert!(json.starts_with("{\n  \"format\": 1,\n"));
        assert!(json.ends_with("  ]\n}\n"));
        // 5 header lines + rows + the closing and section lines around the
        // three arrays (see render).
        assert_eq!(json.lines().count(), vs.len() + fs.len() + fds.len() + 11);
        // The Vietnamese pair is always the last historical vector.
        let last = &vs[vs.len() - 1];
        assert_eq!(last.input, "Ta\u{0302}\u{0300}ng");
        assert_eq!(last.nfc, "T\u{1EA7}ng");
        // The curated edge cases close each tier-1 section.
        assert_eq!(fs[fs.len() - 1].input, "q\u{0323}\u{0301}");
        assert_eq!(fds[fds.len() - 1].input, "ÀÉÎÕÜ a\u{0300}");
    }

    /// The frozen historical section: the legacy sampling logic reproduces
    /// the committed rows byte-for-byte — the tier-1 sections must never
    /// rewrite them.
    #[test]
    fn historical_vectors_are_frozen() {
        let vs = vectors();
        assert_eq!(vs.len(), 952, "the historical row count is frozen");
        assert_eq!(hex(vs[0].input.as_bytes()), "e1b88a");
        assert_eq!(hex(vs[0].nfc.as_bytes()), "e1b88a");
        assert_eq!(hex(vs[0].nfd.as_bytes()), "44cc87");
        assert_eq!(hex(vs[vs.len() - 1].input.as_bytes()), "5461cc82cc806e67");
        let json = render(&vs, &form_vectors(), &fold_vectors());
        // The frozen prefix of the file: header + the full historical
        // section, byte-identical since tier 0.
        let prefix = format!(
            "{{\n  \"format\": 1,\n  \"generator\": \"cargo run --bin gen-reference -- gen\",\n  \"description\": \"Hex-exact NFC/NFD vectors for pith-unicode: UTF-8 bytes of the input string and of its NFC and NFD forms. Inputs are every 16th line of the UCD 15.1.0 NormalizationTest corpus (keeping only lines whose normalization changes) plus the Vietnamese decomposed/precomposed pair.\",\n  \"vectors\": [\n{}\n",
            vs.iter()
                .map(|v| format!(
                    "    [{}, {}, {}]",
                    json_string(&hex(v.input.as_bytes())),
                    json_string(&hex(v.nfc.as_bytes())),
                    json_string(&hex(v.nfd.as_bytes()))
                ))
                .collect::<Vec<_>>()
                .join(",\n")
        );
        assert!(
            json.starts_with(&prefix),
            "the frozen vectors section prefix must stay byte-identical"
        );
    }

    /// The committed `reference.json` must be byte-current with a fresh
    /// regeneration — the in-process form of the `verify` mode CI runs.
    #[test]
    fn committed_reference_is_current() {
        assert_eq!(
            render(&vectors(), &form_vectors(), &fold_vectors()),
            COMMITTED
        );
    }

    #[test]
    fn hex_encoding_roundtrip_marker() {
        assert_eq!(hex("T\u{1EA7}ng".as_bytes()), "54e1baa76e67");
        assert_eq!(hex(b""), "");
    }

    #[test]
    fn tier1_vectors_recompute_consistently() {
        for v in form_vectors() {
            assert_eq!(hex(v.nfc.as_bytes()), hex(nfc(&v.input).as_bytes()));
            assert_eq!(hex(v.nfkd.as_bytes()), hex(nfkd(&v.input).as_bytes()));
        }
        for v in fold_vectors() {
            assert_eq!(hex(v.full.as_bytes()), hex(casefold(&v.input).as_bytes()));
            assert_eq!(
                hex(v.simple.as_bytes()),
                hex(casefold_simple(&v.input).as_bytes())
            );
        }
    }
}
