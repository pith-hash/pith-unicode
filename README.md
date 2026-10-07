<p align="center">
  <img src="https://pith-unicode.n24q02m.com/logo.svg" alt="pith-unicode" width="120">
</p>

<h1 align="center">pith-unicode</h1>

<p align="center">
  <strong>pith foundation: unicode (zero-dep Rust)</strong>
</p>

<p align="center">
  <a href="https://github.com/pith-hash/pith-unicode/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/pith-hash/pith-unicode/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://github.com/pith-hash/pith-unicode/actions/workflows/cd.yml"><img alt="CD" src="https://github.com/pith-hash/pith-unicode/actions/workflows/cd.yml/badge.svg"></a>
  <a href="https://github.com/pith-hash/pith-unicode/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/pith-hash/pith-unicode?display_name=tag&sort=semver"></a>
  <a href="https://github.com/n24q02m/better-semantic-release"><img alt="semantic-release" src="https://img.shields.io/badge/semantic--release-e10079?logo=semantic-release&logoColor=white"></a>
  <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/License-MIT-blue.svg"></a>
</p>

<p align="center">
  <a href="#install">Install</a> ·
  <a href="#quick-start">Quick start</a> ·
  <a href="#the-pith-suite-contract">Suite contract</a>
</p>

<!-- BEGIN: AUTO-GENERATED-CROSS-PROMO -->
<!-- END: AUTO-GENERATED-CROSS-PROMO -->

## The pith suite contract

pith-unicode is part of the **pith** suite (pith-hash). Every suite repository
follows the same rules; CI enforces them mechanically:

- **Naming**: a library is always `pith-<domain>` (`pith-image`, `pith-audio`,
  `pith-zip`, ...). The curator/repository of repositories is the bare
  `pith-hash`. Never invent a second naming scheme inside the suite.
- **Version pinning**: cross-library dependencies pin `~0.1` (e.g.
  `pith-image = { version = "~0.1", path = "../pith-image" }`). The whole suite
  moves together inside 0.1.x; breaking changes require a suite-wide version
  bump, never a silent minor drift.
- **Zero third-party dependencies**: every crate depends only on other
  `pith-*` crates plus `std`. `scripts/check-zero-deps.py` (run in CI) fails
  the build on any other crate, for normal, build and dev dependencies alike.
- **No unsafe**: every crate root carries `#![forbid(unsafe_code)]`.
- **Hex-exact vectors**: `reference.json` at the repo root is the
  cross-language source of truth. The `gen-reference` binary regenerates it;
  CI verifies the committed copy is current (`gen-reference verify`), and CD
  ships the regenerated file with every SDK artifact. Python, Node and Go SDKs
  MUST test against the same bytes.

## Repository layout

```
src/               the pith-unicode library (NFC/NFD over embedded UCD tables)
data/ucd.bin       the embedded UCD 15.1.0 tables (include_bytes!, no build.rs)
data/UNICODE-LICENSE.txt  Unicode License V3 for the UCD and derived tables
tools/gen-reference  the vector generator binary (bin name: gen-reference)
tests/             the UCD conformance corpus and the reference-vector tests
reference.json     hex-exact cross-SDK test vectors
```

## Install

Rust (the core library):

```bash
cargo add pith-unicode
```

Python / Node / Go SDKs are published from the same cdylib on every release;
see the release assets or the package registries for the matching version.

## Quick start

```rust
use pith_unicode::{nfc, nfd};

// Decomposed and precomposed Vietnamese converge on the same NFC bytes:
assert_eq!(nfc("Ta\u{0302}\u{0300}ng"), "T\u{1EA7}ng");
assert_eq!(nfd("T\u{1EA7}ng"), "Ta\u{0302}\u{0300}ng");
```

Regenerate/verify the cross-SDK vectors:

```bash
cargo run --locked --bin gen-reference -- verify   # CI runs this
cargo run --locked --bin gen-reference -- gen      # rewrite reference.json
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## Security

See [SECURITY.md](SECURITY.md).

## License

[MIT](LICENSE) © pith-hash
