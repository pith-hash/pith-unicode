//! `gen-reference` — regenerate and verify the hex-exact NFC/NFD vectors in
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
//! directory dependence): every 16th data line, keeping only lines whose
//! normalization changes, plus the Vietnamese decomposed/precomposed pair
//! the suite calls out. Expected outputs are recomputed by `pith_unicode`
//! itself; `tests/conformance.rs` keeps the external-oracle role by checking
//! the full 19 074-line corpus directly.

use std::process::ExitCode;

use pith_unicode::{nfc, nfd};

const CORPUS: &str = include_str!("../../tests/NormalizationTest.txt");
#[cfg(test)]
const COMMITTED: &str = include_str!("../../reference.json");
const DEFAULT_PATH: &str = "reference.json";
const STRIDE: usize = 16;

/// One hex-exact vector: input bytes, NFC output bytes, NFD output bytes.
struct Vector {
    input: String,
    nfc: String,
    nfd: String,
}

/// Parse a semicolon column of hex code points into a `String`.
fn cps(col: &str) -> String {
    col.split_whitespace()
        .map(|h| {
            char::from_u32(u32::from_str_radix(h, 16).expect("corpus hex")).expect("corpus scalar")
        })
        .collect()
}

/// The deterministic vector set: every [`STRIDE`]-th corpus data line that
/// normalization actually changes, plus the Vietnamese pair.
fn vectors() -> Vec<Vector> {
    let mut out: Vec<Vector> = Vec::new();
    let mut index = 0usize;
    for raw in CORPUS.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() || line.starts_with('@') {
            continue;
        }
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
        index += 1;
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
fn render(vs: &[Vector]) -> String {
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
    let fresh = render(&vectors());
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
        assert!(
            vs.len() > 500,
            "stride sampling must keep a real corpus slice"
        );
        let json = render(&vs);
        assert!(json.starts_with("{\n  \"format\": 1,\n"));
        assert!(json.ends_with("  ]\n}\n"));
        assert_eq!(json.lines().count(), vs.len() + 7);
        // The Vietnamese pair is always the last vector.
        let last = &vs[vs.len() - 1];
        assert_eq!(last.input, "Ta\u{0302}\u{0300}ng");
        assert_eq!(last.nfc, "T\u{1EA7}ng");
    }

    /// The committed `reference.json` must be byte-current with a fresh
    /// regeneration — the in-process form of the `verify` mode CI runs.
    #[test]
    fn committed_reference_is_current() {
        assert_eq!(render(&vectors()), COMMITTED);
    }

    #[test]
    fn hex_encoding_roundtrip_marker() {
        assert_eq!(hex("T\u{1EA7}ng".as_bytes()), "54e1baa76e67");
        assert_eq!(hex(b""), "");
    }
}
