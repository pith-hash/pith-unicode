//! `gen-ucd` — regenerate `data/ucd.bin`, the embedded UCD table blob of
//! `pith-unicode`, from the committed Unicode Character Database source
//! files in `data/ucd/`.
//!
//! Sources (pinned UCD 15.1.0, downloaded from unicode.org — the exact
//! URLs and file SHA-256s are recorded in `data/ucd/PROVENANCE.txt`):
//!
//! - `UnicodeData.txt` — canonical combining classes, canonical
//!   decompositions, compatibility decompositions;
//! - `CompositionExclusions.txt` — Full_Composition_Exclusion (the
//!   composition pair table is canonical decompositions minus this list);
//! - `DerivedNormalizationProps.txt` — the four QC properties
//!   (`NFC_QC` / `NFD_QC` / `NFKC_QC` / `NFKD_QC`) of UAX #15;
//! - `CaseFolding.txt` — case folding mappings (statuses C, F, S; the
//!   Turkic T entries are locale data and are skipped).
//!
//! Modes:
//!
//! - `gen-ucd [SRC_DIR] [OUT_BIN]`  parse + render (defaults:
//!   `<repo>/data/ucd` and `<repo>/data/ucd.bin`)
//!
//! The output is byte-stable across runs and platforms: every table is
//! sorted by key, all integers are little-endian, and nothing depends on
//! iteration order. `tests/gen_ucd.rs` re-runs the generator over the
//! committed sources and byte-compares against the committed blob, so a
//! drifted `ucd.bin` fails CI exactly like a drifted `reference.json`.
//!
//! # `ucd.bin` layout (format 2) — all integers little-endian
//!
//! | offset | contents |
//! |--------|----------|
//! | 0      | magic `UCD2` (4 B) |
//! | 4      | UCD version `u16` major / minor / update (6 B) |
//! | 10     | max decomposition length `u16`, always 3 (2 B) |
//! | 12     | table counts `u32` × 6: ccc/qc, canonical decomposition, pair, compat index, compat elements, casefold (24 B) |
//! | 36     | ccc/qc table: `cp u32, ccc u8, nfc_qc u8, nfd_qc u8, nfkc_qc u8, nfkd_qc u8` — 9 B/record |
//! | …      | canonical decomposition table: `cp u32, len u8, elems[3] u32` — 17 B/record |
//! | …      | composition pair table: `first u32, second u32, composite u32` — 12 B/record |
//! | …      | compat decomposition index: `cp u32, offset u32, len u16` — 10 B/record |
//! | …      | compat decomposition elements: `u32` each, indexed by the offsets above |
//! | …      | casefold table: `cp u32, full_len u8, full[3] u32, simple u32` — 21 B/record |
//!
//! Every table is sorted by its key and searched by binary search in
//! `src/lib.rs`. Hangul syllables and jamo are handled algorithmically
//! (UAX #15 §3.12) and never appear in any table. QC values are encoded
//! 0 = Yes, 1 = No, 2 = Maybe (the UAX #15 quick-check detection logic
//! lives on the consumer side). `full_len == 0` in a casefold record
//! means the full mapping is the identity; `simple` always carries the
//! single-character simple mapping (the code point itself when it is
//! unchanged — e.g. ß, whose full mapping is `ss` but whose simple
//! mapping is ß).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// QC value encodings, matching the `src/lib.rs` accessors.
const QC_YES: u8 = 0;
const QC_NO: u8 = 1;
const QC_MAYBE: u8 = 2;

/// The four UAX #15 quick-check properties this generator consumes.
const QC_PROPS: [&str; 4] = ["NFC_QC", "NFD_QC", "NFKC_QC", "NFKD_QC"];

/// One parsed `UnicodeData.txt` line, narrowed to what the tables need.
struct Character {
    cp: u32,
    ccc: u8,
    /// Raw field 5: `None` when empty, `Some((tag, elems))` when present,
    /// `tag == None` for canonical decompositions.
    decomp: Option<(Option<String>, Vec<u32>)>,
}

/// One parsed `CaseFolding.txt` line.
struct Folding {
    cp: u32,
    status: u8,
    mapping: Vec<u32>,
}

/// One parsed `DerivedNormalizationProps.txt` QC entry: an inclusive
/// code point range carrying one value letter.
struct PropEntry {
    lo: u32,
    hi: u32,
    letter: String,
}

/// The complete derived table set rendered into `ucd.bin`.
struct Tables {
    /// cp -> [ccc, nfc_qc, nfd_qc, nfkc_qc, nfkd_qc]
    ccc: BTreeMap<u32, [u8; 5]>,
    /// cp -> canonical decomposition elements (every mapping length <= 3).
    canonical: BTreeMap<u32, Vec<u32>>,
    /// (first, second) -> composite; canonical decompositions minus the
    /// Full_Composition_Exclusion set.
    pairs: BTreeMap<(u32, u32), u32>,
    /// cp -> raw compatibility decomposition elements (variable length).
    compat: BTreeMap<u32, Vec<u32>>,
    /// cp -> (full mapping, simple mapping); `full` empty means identity.
    casefold: BTreeMap<u32, (Vec<u32>, u32)>,
}

fn parse_cp(hex: &str) -> u32 {
    u32::from_str_radix(hex, 16).unwrap_or_else(|e| panic!("bad code point {hex:?}: {e}"))
}

/// Parses one file, skipping blank lines, `#` comments and `@` directives.
fn data_lines(source: &str) -> impl Iterator<Item = &str> {
    source.lines().filter_map(|line| {
        let line = line.split('#').next().unwrap_or("").trim();
        (!line.is_empty() && !line.starts_with('@')).then_some(line)
    })
}

/// Expands one `hex` or `hex..hex` field into its inclusive code point range.
fn cp_range(field: &str) -> (u32, u32) {
    match field.split_once("..") {
        Some((lo, hi)) => (parse_cp(lo), parse_cp(hi)),
        None => {
            let cp = parse_cp(field);
            (cp, cp)
        }
    }
}

fn parse_unicode_data(source: &str) -> Vec<Character> {
    let mut out = Vec::new();
    for line in data_lines(source) {
        let fields: Vec<&str> = line.split(';').collect();
        assert!(fields.len() >= 15, "short UnicodeData line: {line}");
        // Surrogate-pair range bookkeeping lines carry no decomposition.
        if fields[1].ends_with(", First>") {
            continue;
        }
        let decomp_field = fields[5].trim();
        let decomp = if decomp_field.is_empty() {
            None
        } else {
            let mut parts = decomp_field.split_whitespace();
            let first = parts.next().expect("non-empty decomposition field");
            let tag = first.starts_with('<').then(|| first.to_owned());
            let rest: Vec<u32> = parts.map(parse_cp).collect();
            // A tagged field's first token is the tag; an untagged
            // (canonical) field's first token is already an element.
            let elems: Vec<u32> = if tag.is_some() {
                rest
            } else {
                std::iter::once(parse_cp(first)).chain(rest).collect()
            };
            assert!(!elems.is_empty(), "empty decomposition: {line}");
            Some((tag, elems))
        };
        out.push(Character {
            cp: parse_cp(fields[0]),
            ccc: fields[3]
                .parse()
                .unwrap_or_else(|e| panic!("bad ccc on line {line}: {e}")),
            decomp,
        });
    }
    out
}

fn parse_composition_exclusions(source: &str) -> BTreeSet<u32> {
    let mut set = BTreeSet::new();
    for line in data_lines(source) {
        let (lo, hi) = cp_range(line);
        for cp in lo..=hi {
            set.insert(cp);
        }
    }
    set
}

/// Extracts one property's entries (range + value letter) by name.
fn prop_entries<'a>(props: &'a BTreeMap<String, Vec<PropEntry>>, name: &str) -> &'a [PropEntry] {
    props
        .get(name)
        .map(Vec::as_slice)
        .unwrap_or_else(|| panic!("{name} missing from DerivedNormalizationProps"))
}

fn parse_derived_props(source: &str) -> BTreeMap<String, Vec<PropEntry>> {
    let mut out: BTreeMap<String, Vec<PropEntry>> = BTreeMap::new();
    for line in data_lines(source) {
        let fields: Vec<&str> = line.split(';').map(str::trim).collect();
        assert!(
            !fields.is_empty() && !fields[0].is_empty(),
            "malformed props line: {line}"
        );
        // The file carries many properties; only the four QC ones are
        // consumed, and some of the others have no value column at all.
        assert!(
            fields.len() >= 2,
            "short DerivedNormalizationProps line: {line}"
        );
        // Full_Composition_Exclusion drives the composition pair table;
        // everything else except the four QC properties is ignored.
        if !QC_PROPS.contains(&fields[1]) && fields[1] != "Full_Composition_Exclusion" {
            continue;
        }
        if fields[1] != "Full_Composition_Exclusion" {
            assert!(fields.len() >= 3, "QC line without a value: {line}");
        }
        let (lo, hi) = cp_range(fields[0]);
        out.entry(fields[1].to_owned())
            .or_default()
            .push(PropEntry {
                lo,
                hi,
                letter: fields.get(2).copied().unwrap_or("").to_owned(),
            });
    }
    out
}

fn parse_case_folding(source: &str) -> Vec<Folding> {
    let mut out = Vec::new();
    for line in data_lines(source) {
        let fields: Vec<&str> = line.split(';').map(str::trim).collect();
        assert!(fields.len() >= 4, "short CaseFolding line: {line}");
        let status = fields[1].as_bytes()[0];
        if status == b'T' {
            continue; // Turkic-only mappings are locale data, not folding
        }
        out.push(Folding {
            cp: parse_cp(fields[0]),
            status,
            mapping: fields[2].split_whitespace().map(parse_cp).collect(),
        });
    }
    out
}

fn qc_value(letter: &str) -> u8 {
    match letter {
        "Y" => QC_YES,
        "N" => QC_NO,
        "M" => QC_MAYBE,
        other => panic!("unknown QC value {other:?}"),
    }
}

/// Builds the full table set from the four parsed sources.
fn build(
    characters: &[Character],
    file_exclusions: &BTreeSet<u32>,
    props: &BTreeMap<String, Vec<PropEntry>>,
    foldings: &[Folding],
) -> Tables {
    let mut tables = Tables {
        ccc: BTreeMap::new(),
        canonical: BTreeMap::new(),
        pairs: BTreeMap::new(),
        compat: BTreeMap::new(),
        casefold: BTreeMap::new(),
    };

    // The composition exclusion set is the normative derived property
    // `Full_Composition_Exclusion`: `CompositionExclusions.txt`'s active
    // lines (sections 1–3) plus the non-starter decompositions, which that
    // file documents instead of listing (its section 4 is commented data —
    // the rule is "canonically decomposes to a sequence whose first
    // character is a non-starter"). Both halves are validated below
    // against the property, so a UCD drift cannot silently re-enable an
    // excluded composition.
    let mut fce = BTreeSet::new();
    for entry in prop_entries(props, "Full_Composition_Exclusion") {
        for cp in entry.lo..=entry.hi {
            fce.insert(cp);
        }
    }

    // QC maps: prop -> cp -> encoded value. Ranges must not overlap within
    // one property; a property without file entries behaves as all-Yes.
    let mut qc: BTreeMap<&str, BTreeMap<u32, u8>> = BTreeMap::new();
    for (prop, entries) in props {
        if !QC_PROPS.contains(&prop.as_str()) {
            continue; // Full_Composition_Exclusion is consumed separately
        }
        let map = qc.entry(prop.as_str()).or_default();
        for entry in entries {
            let value = qc_value(&entry.letter);
            for cp in entry.lo..=entry.hi {
                assert!(
                    map.insert(cp, value).is_none(),
                    "duplicate {prop} for {cp:04X}"
                );
            }
        }
    }
    let empty = BTreeMap::new();
    let get = |map: &BTreeMap<u32, u8>, cp: u32| map.get(&cp).copied().unwrap_or(QC_YES);
    let (nfc, nfd) = (
        qc.get("NFC_QC").unwrap_or(&empty),
        qc.get("NFD_QC").unwrap_or(&empty),
    );
    let (nfkc, nfkd) = (
        qc.get("NFKC_QC").unwrap_or(&empty),
        qc.get("NFKD_QC").unwrap_or(&empty),
    );

    let mut by_cp: BTreeMap<u32, &Character> = BTreeMap::new();
    for ch in characters {
        let record = [
            ch.ccc,
            get(nfc, ch.cp),
            get(nfd, ch.cp),
            get(nfkc, ch.cp),
            get(nfkd, ch.cp),
        ];
        if record != [QC_YES; 5] {
            assert!(
                tables.ccc.insert(ch.cp, record).is_none(),
                "duplicate code point {:04X} in UnicodeData",
                ch.cp
            );
        }
        by_cp.insert(ch.cp, ch);
        match &ch.decomp {
            None => {}
            Some((None, elems)) => {
                assert!(
                    elems.len() <= 3,
                    "canonical decomposition of {:04X} exceeds 3 elements",
                    ch.cp
                );
                tables.canonical.insert(ch.cp, elems.clone());
            }
            Some((Some(_tag), elems)) => {
                let prev = tables.compat.insert(ch.cp, elems.clone());
                assert!(
                    prev.is_none(),
                    "duplicate compat decomposition for {:04X}",
                    ch.cp
                );
            }
        }
    }

    // QC properties also cover code points UnicodeData lists only as
    // range bookkeeping — in 15.1.0 the 11 172 Hangul syllables appear
    // solely as `<Hangul Syllable, First/Last>` markers — so sweep every
    // QC-covered code point into the table with ccc 0.
    for map in [nfc, nfd, nfkc, nfkd] {
        for &cp in map.keys() {
            if tables.ccc.contains_key(&cp) {
                continue;
            }
            tables.ccc.insert(
                cp,
                [0, get(nfc, cp), get(nfd, cp), get(nfkc, cp), get(nfkd, cp)],
            );
        }
    }

    // Composition pairs: two-element canonical decompositions outside the
    // exclusion set, cross-validated against the documented derivation.
    let ccc_of = |cp: u32| by_cp.get(&cp).map(|ch| ch.ccc).unwrap_or(0);
    for (cp, elems) in &tables.canonical {
        if elems.len() != 2 || !fce.contains(cp) {
            continue;
        }
        let in_file = file_exclusions.contains(cp);
        let non_starter = ccc_of(elems[0]) != 0;
        assert!(
            in_file || non_starter,
            "excluded composite {cp:04X} is neither listed nor a non-starter decomposition"
        );
    }
    for (cp, elems) in &tables.canonical {
        if elems.len() == 2 && !fce.contains(cp) {
            tables.pairs.insert((elems[0], elems[1]), *cp);
        }
    }

    // Case folding: one record per code point. C gives both mappings; F
    // gives the full mapping and S its simple counterpart. A cp with only
    // F folds to itself simply (e.g. U+00DF -> U+00DF). A C mapping equal
    // to the code point itself is stored as the identity.
    let mut fold_by_cp: BTreeMap<u32, Vec<&Folding>> = BTreeMap::new();
    for folding in foldings {
        fold_by_cp.entry(folding.cp).or_default().push(folding);
    }
    for (cp, group) in fold_by_cp {
        assert!(group.len() <= 2, "more than two foldings for {cp:04X}");
        let mut full: Vec<u32> = Vec::new();
        let mut simple = cp;
        for folding in &group {
            match folding.status {
                b'C' => {
                    assert_eq!(
                        folding.mapping.len(),
                        1,
                        "multi-element C mapping for {cp:04X}"
                    );
                    full = folding.mapping.clone();
                    simple = folding.mapping[0];
                }
                b'F' => {
                    assert!(
                        folding.mapping.len() <= 3,
                        "F mapping for {cp:04X} exceeds 3 elements"
                    );
                    full = folding.mapping.clone();
                }
                b'S' => {
                    assert_eq!(
                        folding.mapping.len(),
                        1,
                        "multi-element S mapping for {cp:04X}"
                    );
                    simple = folding.mapping[0];
                }
                other => panic!("unexpected casefold status {}", other as char),
            }
        }
        if full == [cp] {
            full.clear(); // identity full mapping is stored as the empty mapping
        }
        tables.casefold.insert(cp, (full, simple));
    }
    tables
}

fn render(tables: &Tables) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let push_u16 = |out: &mut Vec<u8>, v: u16| out.extend_from_slice(&v.to_le_bytes());
    let push_u32 = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_le_bytes());

    out.extend_from_slice(b"UCD2");
    push_u16(&mut out, 15); // UCD major
    push_u16(&mut out, 1); // UCD minor
    push_u16(&mut out, 0); // UCD update
    push_u16(&mut out, 3); // max canonical decomposition length

    push_u32(&mut out, tables.ccc.len() as u32);
    push_u32(&mut out, tables.canonical.len() as u32);
    push_u32(&mut out, tables.pairs.len() as u32);
    push_u32(&mut out, tables.compat.len() as u32);
    push_u32(
        &mut out,
        tables.compat.values().map(Vec::len).sum::<usize>() as u32,
    );
    push_u32(&mut out, tables.casefold.len() as u32);

    for (cp, &[ccc, nfc, nfd, nfkc, nfkd]) in &tables.ccc {
        push_u32(&mut out, *cp);
        out.extend_from_slice(&[ccc, nfc, nfd, nfkc, nfkd]);
    }
    for (cp, elems) in &tables.canonical {
        push_u32(&mut out, *cp);
        out.push(elems.len() as u8);
        for i in 0..3 {
            push_u32(&mut out, elems.get(i).copied().unwrap_or(0));
        }
    }
    for ((first, second), composite) in &tables.pairs {
        push_u32(&mut out, *first);
        push_u32(&mut out, *second);
        push_u32(&mut out, *composite);
    }
    let mut elems: Vec<u32> = Vec::new();
    for (cp, mapping) in &tables.compat {
        push_u32(&mut out, *cp);
        push_u32(&mut out, elems.len() as u32);
        push_u16(&mut out, mapping.len() as u16);
        elems.extend_from_slice(mapping);
    }
    for e in &elems {
        push_u32(&mut out, *e);
    }
    for (cp, (full, simple)) in &tables.casefold {
        push_u32(&mut out, *cp);
        out.push(full.len() as u8);
        for i in 0..3 {
            push_u32(&mut out, full.get(i).copied().unwrap_or(0));
        }
        push_u32(&mut out, *simple);
    }
    out
}

fn default_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("data/ucd")
}

fn default_out() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("data/ucd.bin")
}

/// Reads a source file from the pinned source directory.
fn read_source(dir: &Path, name: &str) -> String {
    std::fs::read_to_string(dir.join(name))
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.join(name).display()))
}

/// Parses the four committed sources and renders the blob. Shared by
/// `main` and the regen tests.
pub fn generate(src: &Path) -> Vec<u8> {
    let characters = parse_unicode_data(&read_source(src, "UnicodeData.txt"));
    let exclusions = parse_composition_exclusions(&read_source(src, "CompositionExclusions.txt"));
    let props = parse_derived_props(&read_source(src, "DerivedNormalizationProps.txt"));
    let foldings = parse_case_folding(&read_source(src, "CaseFolding.txt"));
    render(&build(&characters, &exclusions, &props, &foldings))
}

fn usage() -> String {
    "usage: gen-ucd [src_dir] [out_bin]".to_owned()
}

fn run(args: &[String]) -> Result<(), String> {
    let (src, out) = match args {
        [] => (default_src(), default_out()),
        [s] => (PathBuf::from(s), default_out()),
        [s, o] => (PathBuf::from(s), PathBuf::from(o)),
        _ => return Err(usage()),
    };
    if !src.is_dir() {
        return Err(format!("source directory {} does not exist", src.display()));
    }
    let blob = generate(&src);
    std::fs::write(&out, &blob).map_err(|e| format!("cannot write {}: {e}", out.display()))?;
    println!("wrote {} ({} bytes)", out.display(), blob.len());
    Ok(())
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

    fn src_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("data/ucd")
    }

    #[test]
    fn parses_the_pinned_sources() {
        let src = src_dir();
        let characters = parse_unicode_data(&read_source(&src, "UnicodeData.txt"));
        // UCD 15.1.0: 34 931 data lines minus the 19 `<..., First>`
        // surrogate-pair bookkeeping lines.
        assert_eq!(characters.len(), 34_912);
        // The file's four sections carry 81 active data lines in 15.1.0;
        // its section 4 (non-starter decompositions) is documented instead
        // of listed, so the generator derives the rest (see `build`).
        let file_exclusions =
            parse_composition_exclusions(&read_source(&src, "CompositionExclusions.txt"));
        assert_eq!(file_exclusions.len(), 81);
        let props = parse_derived_props(&read_source(&src, "DerivedNormalizationProps.txt"));
        for prop in QC_PROPS {
            assert!(props.contains_key(prop), "{prop} missing from props file");
        }
        // Only normalization props and Full_Composition_Exclusion are
        // consumed; the file carries many others.
        assert!(!props.contains_key("Changes_When_NFKC_Casefolded"));
        assert!(!props.contains_key("Expands_On_NFD"));
        assert!(props.contains_key("Full_Composition_Exclusion"));
        let foldings = parse_case_folding(&read_source(&src, "CaseFolding.txt"));
        assert_eq!(foldings.len(), 1_561); // C + F + S lines; the 2 T lines are skipped
    }

    #[test]
    fn builds_the_expected_table_shapes() {
        let src = src_dir();
        let characters = parse_unicode_data(&read_source(&src, "UnicodeData.txt"));
        let file_exclusions =
            parse_composition_exclusions(&read_source(&src, "CompositionExclusions.txt"));
        let props = parse_derived_props(&read_source(&src, "DerivedNormalizationProps.txt"));
        let foldings = parse_case_folding(&read_source(&src, "CaseFolding.txt"));
        let tables = build(&characters, &file_exclusions, &props, &foldings);

        // The canonical table sizes of format 1 must be reproduced.
        assert_eq!(tables.canonical.len(), 2_061);
        assert_eq!(tables.pairs.len(), 941);
        // U+00E9 (é): canonical pair outside the exclusions.
        assert_eq!(tables.pairs.get(&(0x65, 0x301)), Some(&0xE9));
        // Full_Composition_Exclusion: U+0958 never composes.
        assert!(!tables.pairs.contains_key(&(0x915, 0x93C)));
        // Hangul never lands in any table.
        assert!(
            !tables
                .canonical
                .keys()
                .any(|&cp| (0xAC00..0xAC00 + 11_172).contains(&cp))
        );
        assert!(
            !tables
                .compat
                .keys()
                .any(|&cp| (0xAC00..0xAC00 + 11_172).contains(&cp))
        );
        // Compat: U+FDFA holds the 18-character isolated-form sequence
        // (19 field tokens minus the `<isolated>` tag); U+01C4 (Ǆ, `DŽ`)
        // is compat-only, U+00C0 (À) canonical.
        assert_eq!(tables.compat.get(&0xFDFA).map(Vec::len), Some(18));
        assert!(tables.canonical.contains_key(&0xC0));
        assert!(!tables.canonical.contains_key(&0x1C4));
        assert_eq!(tables.compat.get(&0x1C4).map(Vec::len), Some(2));

        // Case folding, pinned per UAX #44: ß and ẞ fully fold to `ss`,
        // their simple mapping is ß; İ is present as a C mapping; the
        // T-only code point U+1310 is skipped entirely.
        assert_eq!(tables.casefold.get(&0xDF), Some(&(vec![0x73, 0x73], 0xDF)));
        assert_eq!(
            tables.casefold.get(&0x1E9E),
            Some(&(vec![0x73, 0x73], 0xDF))
        );
        // İ folds fully to `i` + combining dot above and simply to itself.
        assert_eq!(
            tables.casefold.get(&0x130),
            Some(&(vec![0x69, 0x307], 0x130))
        );
        assert!(!tables.casefold.contains_key(&0x1310));
    }

    #[test]
    fn gen_reproduces_the_committed_blob() {
        let committed = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("data/ucd.bin"))
            .expect("committed ucd.bin");
        assert_eq!(
            generate(&src_dir()),
            committed,
            "data/ucd.bin must equal a fresh regeneration"
        );
    }

    #[test]
    fn range_expansion_covers_both_kinds() {
        assert_eq!(cp_range("0300..0304"), (0x300, 0x304));
        assert_eq!(cp_range("030F"), (0x30F, 0x30F));
    }

    #[test]
    fn run_reports_missing_sources_and_usage() {
        let tmp = std::env::temp_dir().join(format!("gen-ucd-missing-{}", std::process::id()));
        let err = run(&[tmp.to_string_lossy().into_owned()]).unwrap_err();
        assert!(err.contains("does not exist"), "{err}");
        assert_eq!(usage(), "usage: gen-ucd [src_dir] [out_bin]");
        let err = run(&["a".into(), "b".into(), "c".into()]).unwrap_err();
        assert!(err.contains("usage"), "{err}");
    }

    #[test]
    fn run_writes_to_an_explicit_out_path() {
        let out = std::env::temp_dir().join(format!("gen-ucd-out-{}.bin", std::process::id()));
        run(&[
            src_dir().to_string_lossy().into_owned(),
            out.to_string_lossy().into_owned(),
        ])
        .expect("gen into temp out");
        let committed = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("data/ucd.bin"))
            .expect("committed ucd.bin");
        assert_eq!(std::fs::read(&out).expect("temp out"), committed);
        std::fs::remove_file(&out).expect("cleanup");
    }
}
