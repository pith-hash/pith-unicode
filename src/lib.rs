//! Unicode normalisation (NFD · NFC · NFKD · NFKC), UAX #15 quick-check
//! detection, and case folding — driven by tables generated from the UCD.
//!
//! Part of the `pith` suite: the library itself builds with an empty
//! `[dependencies]` table — the whole suite resolves without a single
//! registry package.
//!
//! The tables are generated from the Unicode Character Database 15.1.0 by
//! `cargo run --bin gen-ucd` (sources pinned in `data/ucd/`) and embedded
//! as `data/ucd.bin` via `include_bytes!` — no `build.rs`.
//! `data/UNICODE-LICENSE.txt` is the Unicode License V3 the UCD files and
//! the derived tables ship under.
//!
//! # `ucd.bin` layout (format 2) — all integers little-endian
//!
//! | offset | contents |
//! |--------|----------|
//! | 0      | magic `UCD2` (4 B) |
//! | 4      | UCD version `u16` major / minor / update (6 B) |
//! | 10     | max decomposition length `u16`, always 3 (2 B) |
//! | 12     | record counts `u32` × 6: ccc/qc, canonical decomposition, pair, compat index, compat elements, casefold (24 B) |
//! | 36     | ccc/qc table: `cp u32, ccc u8, nfc_qc u8, nfd_qc u8, nfkc_qc u8, nfkd_qc u8` — 9 B/record |
//! | …      | canonical decomposition table: `cp u32, len u8, elems[3] u32` — 17 B |
//! | …      | composition table: `first u32, second u32, composite u32` — 12 B |
//! | …      | compat decomposition index: `cp u32, offset u32, len u16` — 10 B |
//! | …      | compat decomposition elements: `u32` each |
//! | …      | casefold table: `cp u32, full_len u8, full[3] u32, simple u32` — 21 B |
//!
//! Every table is sorted by key and searched by binary search. Hangul
//! syllables and jamo are handled algorithmically (UAX #15 §3.12) and never
//! appear in the tables. The four `*_qc` columns are the UAX #15
//! quick-check properties encoded as 0=Yes, 1=No, 2=Maybe:
//!
//! - [`is_normalized`] answers `true` without allocating when every
//!   character is `Yes` and canonical order holds;
//! - `No` proves the form would change the text — instant `false`;
//! - `Maybe` (possible composition against a preceding starter) forces the
//!   slow path, the only sound answer for that string.
//!
//! Case folding stores the full mapping (`C`/`F` statuses of
//! `CaseFolding.txt`, up to 3 elements) and the simple mapping (`S`, or the
//! common single-character `C` mapping); the Turkic `T` entries are locale
//! data and are not folded by [`casefold`] or [`casefold_simple`].

// `unsafe` is denied everywhere except `ffi`, the C ABI surface the
// language SDKs bind through: raw pointers exist only at that boundary,
// and every exported function is a documented `unsafe extern "C"` fn.
#![deny(unsafe_code)]
#![deny(missing_docs)]

/// The C ABI surface the Python (ctypes), Node (koffi) and Go (cgo)
/// SDKs bind through.
pub mod ffi;

/// The embedded UCD tables (UCD 15.1.0, licensed per
/// `data/UNICODE-LICENSE.txt`).
const UCD: &[u8] = include_bytes!("../data/ucd.bin");

// Hangul algorithmic constants, UAX #15 §3.12.
const SBASE: u32 = 0xAC00;
const LBASE: u32 = 0x1100;
const VBASE: u32 = 0x1161;
const TBASE: u32 = 0x11A7;
const LCOUNT: u32 = 19;
const VCOUNT: u32 = 21;
const TCOUNT: u32 = 28;
const NCOUNT: u32 = VCOUNT * TCOUNT;
const SCOUNT: u32 = LCOUNT * NCOUNT;

const HEADER: usize = 36;
const MAX_DECOMP: usize = 3;
/// Record sizes of the six fixed-width tables.
const CCC_RECORD: usize = 9;
const DECOMP_RECORD: usize = 17;
const PAIR_RECORD: usize = 12;
const COMPAT_RECORD: usize = 10;
const CASEFOLD_RECORD: usize = 21;

const fn u32_at(b: &[u8], i: usize) -> u32 {
    (b[i] as u32) | ((b[i + 1] as u32) << 8) | ((b[i + 2] as u32) << 16) | ((b[i + 3] as u32) << 24)
}

// Fail at compile time if the embedded blob is not the UCD 15.1.0 table the
// decoder was written for, or if its records are not the fixed shape below.
// One u32 compare per field group: bytes 4..8 = major|minor, 8..12 =
// update|maxlen, all little-endian.
const _: () = {
    assert!(
        u32_at(UCD, 0) == 0x32_44_43_55,
        "ucd.bin magic must be UCD2"
    );
    assert!(
        u32_at(UCD, 4) == 15 | (1 << 16),
        "ucd.bin must be generated from UCD 15.1.0"
    );
    assert!(
        u32_at(UCD, 8) == (MAX_DECOMP as u32) << 16,
        "ucd.bin must be UCD update 0 with 3-element decomposition records"
    );
};

const CCC_COUNT: usize = u32_at(UCD, 12) as usize;
const DECOMP_COUNT: usize = u32_at(UCD, 16) as usize;
const PAIR_COUNT: usize = u32_at(UCD, 20) as usize;
const COMPAT_COUNT: usize = u32_at(UCD, 24) as usize;
const COMPAT_ELEM_COUNT: usize = u32_at(UCD, 28) as usize;
const CASEFOLD_COUNT: usize = u32_at(UCD, 32) as usize;
const CCC_OFF: usize = HEADER;
const DECOMP_OFF: usize = CCC_OFF + CCC_COUNT * CCC_RECORD;
const PAIR_OFF: usize = DECOMP_OFF + DECOMP_COUNT * DECOMP_RECORD;
const COMPAT_OFF: usize = PAIR_OFF + PAIR_COUNT * PAIR_RECORD;
const COMPAT_ELEM_OFF: usize = COMPAT_OFF + COMPAT_COUNT * COMPAT_RECORD;
const CASEFOLD_OFF: usize = COMPAT_ELEM_OFF + COMPAT_ELEM_COUNT * 4;

/// Binary search over one of the key-sorted tables; `record` is its row
/// size and `read` decodes the value out of a matched row.
fn lookup<const N: usize>(
    count: usize,
    offset: usize,
    record: usize,
    key: [u32; N],
    read: impl Fn(usize) -> [u32; N],
) -> Option<usize> {
    let (mut lo, mut hi) = (0usize, count);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let i = offset + mid * record;
        match key.cmp(&read(i)) {
            core::cmp::Ordering::Less => hi = mid,
            core::cmp::Ordering::Greater => lo = mid + 1,
            core::cmp::Ordering::Equal => return Some(i),
        }
    }
    None
}

/// The raw ccc/qc record of `cp` as `[ccc, nfc_qc, nfd_qc, nfkc_qc,
/// nfkd_qc]`, or `[0; 5]` when the table has no entry (ccc 0, every
/// quick-check property Yes).
fn ccc_qc_record(cp: u32) -> [u8; 5] {
    match lookup::<1>(CCC_COUNT, CCC_OFF, CCC_RECORD, [cp], |i| [u32_at(UCD, i)]) {
        Some(i) => [UCD[i + 4], UCD[i + 5], UCD[i + 6], UCD[i + 7], UCD[i + 8]],
        None => [0; 5],
    }
}

/// The canonical combining class of `cp`; 0 when the table has no entry.
fn ccc(cp: u32) -> u8 {
    ccc_qc_record(cp)[0]
}

/// The NFC_Quick_Check flag of `cp`: 0=Yes, 1=No, 2=Maybe.
fn nfc_qc(cp: u32) -> u8 {
    ccc_qc_record(cp)[1]
}

/// The NFD_Quick_Check flag of `cp`: 0=Yes, 1=No (NFD has no Maybe).
fn nfd_qc(cp: u32) -> u8 {
    ccc_qc_record(cp)[2]
}

/// The NFKC_Quick_Check flag of `cp`: 0=Yes, 1=No, 2=Maybe.
fn nfkc_qc(cp: u32) -> u8 {
    ccc_qc_record(cp)[3]
}

/// The NFKD_Quick_Check flag of `cp`: 0=Yes, 1=No (NFKD has no Maybe).
fn nfkd_qc(cp: u32) -> u8 {
    ccc_qc_record(cp)[4]
}

/// The canonical decomposition of `cp` as `(length, elements)`, or `None`.
/// Table records hold up to [`MAX_DECOMP`] elements; unused slots are 0.
fn decomp(cp: u32) -> Option<(u8, [u32; MAX_DECOMP])> {
    lookup::<1>(DECOMP_COUNT, DECOMP_OFF, DECOMP_RECORD, [cp], |i| {
        [u32_at(UCD, i)]
    })
    .map(|i| {
        let len = UCD[i + 4];
        let elems = [u32_at(UCD, i + 5), u32_at(UCD, i + 9), u32_at(UCD, i + 13)];
        (len, elems)
    })
}

/// The compatibility decomposition index of `cp`: `(element offset,
/// length)` into the flat element array, or `None`.
fn compat_lookup(cp: u32) -> Option<(usize, usize)> {
    lookup::<1>(COMPAT_COUNT, COMPAT_OFF, COMPAT_RECORD, [cp], |i| {
        [u32_at(UCD, i)]
    })
    .map(|i| {
        let base = u32_at(UCD, i + 4) as usize;
        let len = u16::from_le_bytes([UCD[i + 8], UCD[i + 9]]) as usize;
        (base, len)
    })
}

/// The case folding record of `cp` as `(full mapping, simple mapping)`,
/// with an empty full mapping meaning identity.
fn casefold_record(cp: u32) -> (Vec<u32>, u32) {
    match lookup::<1>(CASEFOLD_COUNT, CASEFOLD_OFF, CASEFOLD_RECORD, [cp], |i| {
        [u32_at(UCD, i)]
    }) {
        Some(i) => {
            let len = UCD[i + 4] as usize;
            let elems = [u32_at(UCD, i + 5), u32_at(UCD, i + 9), u32_at(UCD, i + 13)];
            (elems[..len].to_vec(), u32_at(UCD, i + 17))
        }
        None => (Vec::new(), cp),
    }
}

/// The primary composite of `first` + `second`, or `None` — Hangul jamo pairs
/// are resolved algorithmically by the caller, not by this table.
fn composite(first: u32, second: u32) -> Option<u32> {
    lookup::<2>(PAIR_COUNT, PAIR_OFF, PAIR_RECORD, [first, second], |i| {
        [u32_at(UCD, i), u32_at(UCD, i + 4)]
    })
    .map(|i| u32_at(UCD, i + 8))
}

/// Recursion mode of [`decompose_with`]: canonical-only (NFD) or full
/// (NFKD — compatibility mappings expand too).
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    /// NFD: only canonical mappings expand.
    Canonical,
    /// NFKD: compatibility mappings expand first, then canonical ones.
    Full,
}

/// Recursive decomposition of one code point (UAX #15 §3.7): table
/// decompositions (compatibility-aware in `Full` mode), algorithmic
/// Hangul, and the identity for everything else. Both mapping kinds
/// recurse through this same function, so nested canonical/compatibility
/// chains resolve in one pass.
fn decompose_with(cp: u32, mode: Mode, out: &mut Vec<u32>) {
    if (SBASE..SBASE + SCOUNT).contains(&cp) {
        let i = cp - SBASE;
        out.push(LBASE + i / NCOUNT);
        out.push(VBASE + (i % NCOUNT) / TCOUNT);
        let t = i % TCOUNT;
        if t > 0 {
            out.push(TBASE + t);
        }
        return;
    }
    if mode == Mode::Full
        && let Some((base, len)) = compat_lookup(cp)
    {
        for k in 0..len {
            decompose_with(u32_at(UCD, COMPAT_ELEM_OFF + (base + k) * 4), mode, out);
        }
        return;
    }
    if let Some((len, elems)) = decomp(cp) {
        for &e in &elems[..len as usize] {
            decompose_with(e, mode, out);
        }
    } else {
        out.push(cp);
    }
}

/// Canonical ordering (UAX #15 §3.11): stable insertion sort of each run of
/// non-starters by combining class; starters (ccc 0) are never reordered.
fn canonical_order(seq: &mut [u32]) {
    for i in 1..seq.len() {
        let c = ccc(seq[i]);
        if c == 0 {
            continue;
        }
        let mut j = i;
        while j > 0 {
            let pc = ccc(seq[j - 1]);
            if pc == 0 || pc <= c {
                break;
            }
            seq.swap(j - 1, j);
            j -= 1;
        }
    }
}

/// The canonical composition of `first` + `second` if it exists: Hangul
/// L+V and LV+T pairs compose algorithmically, everything else composes only
/// through the pair table (which already excludes composition exclusions).
fn try_compose(first: u32, second: u32) -> Option<u32> {
    if (LBASE..LBASE + LCOUNT).contains(&first) && (VBASE..VBASE + VCOUNT).contains(&second) {
        return Some(SBASE + (first - LBASE) * NCOUNT + (second - VBASE) * TCOUNT);
    }
    // `rem_euclid` is `%` for u32; written as a method because stable clippy
    // suggests `is_multiple_of` here, which the suite MSRV (1.85) lacks.
    let is_lv = (SBASE..SBASE + SCOUNT).contains(&first) && (first - SBASE).rem_euclid(TCOUNT) == 0;
    if is_lv && (TBASE + 1..TBASE + TCOUNT).contains(&second) {
        return Some(first + (second - TBASE));
    }
    composite(first, second)
}

/// Decomposes `input` into canonically ordered code points; `full`
/// selects the NFKD (compatibility-aware) recursion.
fn ordered(input: &str, full: bool) -> Vec<u32> {
    let mode = if full { Mode::Full } else { Mode::Canonical };
    let mut seq = Vec::with_capacity(input.len());
    for ch in input.chars() {
        decompose_with(ch as u32, mode, &mut seq);
    }
    canonical_order(&mut seq);
    seq
}

/// The canonical composition of an ordered decomposition (UAX #15 §3.10):
/// each starter greedily absorbs every following non-starter that can
/// compose with it and is not *blocked* (a character is blocked by a
/// preceding non-composed character whose ccc is >= its own).
fn compose(seq: Vec<u32>) -> String {
    let mut out: Vec<u32> = Vec::with_capacity(seq.len());
    let mut starter: Option<usize> = None;
    let mut last_cc = 0u8;
    for &cp in &seq {
        let cc = ccc(cp);
        let composed = starter.and_then(|s| {
            // A character is blocked from the starter by an intervening
            // non-composed character with ccc >= cc (cc==0 can never block).
            if last_cc == 0 || last_cc < cc {
                try_compose(out[s], cp)
            } else {
                None
            }
        });
        if let Some(c) = composed {
            out[starter.expect("composed implies a starter")] = c;
        } else {
            out.push(cp);
            if cc == 0 {
                starter = Some(out.len() - 1);
            }
            last_cc = cc;
        }
    }
    out.iter()
        .map(|&cp| char::from_u32(cp).expect("the UCD never composes to a surrogate"))
        .collect()
}

/// The Unicode normalization form a normalization / quick-check operation
/// addresses (UAX #15).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum NormalizationForm {
    /// Canonical decomposition (NFD).
    Nfd,
    /// Canonical composition (NFC).
    Nfc,
    /// Compatibility decomposition (NFKD).
    Nfkd,
    /// Compatibility composition (NFKC).
    Nfkc,
}

impl NormalizationForm {
    /// The FFI form code: 1 = NFC, 2 = NFD, 3 = NFKC, 4 = NFKD.
    #[must_use]
    pub fn code(self) -> u32 {
        match self {
            Self::Nfc => 1,
            Self::Nfd => 2,
            Self::Nfkc => 3,
            Self::Nfkd => 4,
        }
    }

    /// The form behind an FFI form code, or `None` when out of range.
    #[must_use]
    pub fn from_code(code: u32) -> Option<Self> {
        match code {
            1 => Some(Self::Nfc),
            2 => Some(Self::Nfd),
            3 => Some(Self::Nfkc),
            4 => Some(Self::Nfkd),
            _ => None,
        }
    }
}

/// Normalizes `input` to the requested Unicode normalization form — the
/// form-dispatched entry point over [`nfc`], [`nfd`], [`nfkc`] and [`nfkd`].
#[must_use]
pub fn normalize(form: NormalizationForm, input: &str) -> String {
    match form {
        NormalizationForm::Nfc => nfc(input),
        NormalizationForm::Nfd => nfd(input),
        NormalizationForm::Nfkc => nfkc(input),
        NormalizationForm::Nfkd => nfkd(input),
    }
}

/// NFD — canonical decomposition followed by canonical ordering (UAX #15).
#[must_use]
pub fn nfd(input: &str) -> String {
    ordered(input, false)
        .iter()
        .map(|&cp| char::from_u32(cp).expect("the UCD never decomposes to a surrogate"))
        .collect()
}

/// NFKD — compatibility decomposition followed by canonical ordering
/// (UAX #15): full decomposition through the compatibility mappings, then
/// the same ordering pass as [`nfd`].
#[must_use]
pub fn nfkd(input: &str) -> String {
    ordered(input, true)
        .iter()
        .map(|&cp| char::from_u32(cp).expect("the UCD never decomposes to a surrogate"))
        .collect()
}

/// NFC — the canonical composition of [`nfd`]; see [`compose`] and the
/// quick-check fast path in [`is_normalized`].
#[must_use]
pub fn nfc(input: &str) -> String {
    if is_normalized(NormalizationForm::Nfc, input) {
        return input.to_owned();
    }
    compose(ordered(input, false))
}

/// NFKC — the canonical composition of [`nfkd`]: full (compatibility)
/// decomposition, ordering, then the same composition pass as [`nfc`].
#[must_use]
pub fn nfkc(input: &str) -> String {
    if is_normalized(NormalizationForm::Nfkc, input) {
        return input.to_owned();
    }
    compose(ordered(input, true))
}

/// UAX #15 "Detecting Normalization Forms": answers whether [`normalize`]
/// would return the input unchanged.
///
/// The four quick-check properties give allocation-free answers except in
/// one case: a `Maybe` character (possible composition against a preceding
/// starter) forces the slow path for that string, because only computing
/// the form is sound there. The result is exact — never a pessimistic
/// approximation.
/// A quick-check predicate over a whole string paired with its
/// per-code-point QC table getter.
type QcDispatch = (fn(&str) -> bool, fn(u32) -> u8);

/// Answers whether `input` is already in `form` (exact, never a
/// pessimistic approximation).
#[must_use]
pub fn is_normalized(form: NormalizationForm, input: &str) -> bool {
    // The slow path for a Maybe hit must not re-enter the public entry
    // points (nfc/nfkc themselves consult this function), so it computes
    // the form directly from decomposition + composition.
    let (qc_maybe_slow, qc_of): QcDispatch = match form {
        NormalizationForm::Nfc => (|s: &str| compose(ordered(s, false)) == *s, nfc_qc),
        NormalizationForm::Nfd => (|s: &str| nfd(s) == *s, nfd_qc),
        NormalizationForm::Nfkc => (|s: &str| compose(ordered(s, true)) == *s, nfkc_qc),
        NormalizationForm::Nfkd => (|s: &str| nfkd(s) == *s, nfkd_qc),
    };
    let mut last_cc = 0u8;
    for ch in input.chars() {
        let cp = ch as u32;
        let cc = ccc(cp);
        if cc != 0 && cc < last_cc {
            return false; // not in canonical order
        }
        match qc_of(cp) {
            0 => {}                           // Yes: keep scanning
            1 => return false,                // No: the form provably changes it
            _ => return qc_maybe_slow(input), // Maybe: only the slow path is sound
        }
        last_cc = cc;
    }
    true
}

/// Full case folding of `input` (UAX #44 `CaseFolding.txt` statuses `C`
/// and `F`): mappings may expand to several characters (e.g. ß → ss). The
/// Turkic `T` entries are locale data and are not applied.
#[must_use]
pub fn casefold(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        let (full, _simple) = casefold_record(ch as u32);
        if full.is_empty() {
            out.push(ch);
        } else {
            for cp in full {
                out.push(char::from_u32(cp).expect("casefold mappings are scalars"));
            }
        }
    }
    out
}

/// Simple case folding of `input` (`C` and `S` statuses): a strict
/// one-to-one mapping — a code point folds to at most one code point
/// (e.g. ß folds to itself, ẞ folds to ß).
#[must_use]
pub fn casefold_simple(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        let (_full, simple) = casefold_record(ch as u32);
        out.push(char::from_u32(simple).expect("casefold mappings are scalars"));
    }
    out
}
