# OpenUSD model reading and export

Decision/review, 2026-10-27: use pinned `openusd` 0.8.0 with default features
disabled to read `.usda`, `.usdc` and `.usdz` furniture models. It is a pure-Rust
implementation with no C++ toolchain, builds for native and `wasm32-wasip1`, and
implements USD layer composition (references, payloads, variants, instancing).
Writing a binary `.usdc` reader and composition engine internally would add a
large, security-sensitive parser for no Domowik-specific gain. The crate is
pre-1.0 with no API-stability promise, so it is pinned exactly; upgrades are
reviewed like a new adoption.

Plan export (`.usdz`, `domowik-scene`) generates its scene description itself
and uses the same crate only to encode it as a binary `.usdc` layer and to
write the uncompressed, 64-byte-aligned USDZ package. Byte-identical
re-export is a tested property of the pinned version. `image` (already
adopted, PNG encoder) writes packaged textures. Exports were also checked with
Pixar's `usdchecker --arkit` (macOS system tools); that tool is not a
dependency.

Reviewed exact crates.io package manifests and packaged licence files (not a
legal opinion). Newly resolved distributed packages:

| Package | Version | Declared licence | Selected branch |
|---|---|---|---|
| openusd | 0.8.0 | MIT (plus TOST-1.0 file, below) | MIT |
| derive_more, derive_more-impl | 2.1.1 | MIT | MIT |
| logos, logos-codegen, logos-derive | 0.16.1 | MIT OR Apache-2.0 | Apache-2.0 |
| lz4_flex | 0.14.0 | MIT | MIT |
| strum, strum_macros | 0.28.0 | MIT | MIT |
| regex-lite | 0.1.9 | MIT OR Apache-2.0 | Apache-2.0 |
| twox-hash | 2.1.5 | MIT | MIT |
| typed-path | 0.12.4 | MIT OR Apache-2.0 | Apache-2.0 |
| zip | 8.6.0 | MIT | MIT |
| fnv | 1.0.7 | Apache-2.0 / MIT | Apache-2.0 |
| libm | 0.2.16 | MIT | MIT |

Already-reviewed crates (`half`, `crunchy`, `zerocopy`, `aho-corasick`,
`regex-automata`, `regex-syntax`) move from dev-only to distributed.
`rustc_version` and `semver` are build-time only. `zip` 8.6.0 resolves beside
the workspace's existing `zip` 2.x; the duplicate is accepted for this pin.
`openusd` requires `zip` 8's `deflate` feature, which enables `flate2`'s
`zlib-rs` backend; Cargo feature unification makes every `flate2` user in the
build (`.domo` package writing, PNG encoding) compress with zlib-rs instead of
miniz_oxide. Output stays valid DEFLATE and FORMAT-SPEC §7 makes the deflate
implementation writer policy, so packages remain compatible, but compressed
bytes differ from earlier builds.

Exact source packages: `https://crates.io/api/v1/crates/<package>/<version>/download`.

`logos` 0.16.1 is also a direct dependency of `domowik-models` (no new crate):
its trait drives openusd's own USDA tokenizer for the pre-parse nesting bound.

## Known limitation: binary layers

Domowik checks package entry count, nesting, compression and text-layer
nesting before openusd parses anything, then bounds prims, depth, geometry and
textures while reading. It cannot bound openusd's binary `.usdc` decoding from
outside the crate: each LZ4 block may expand up to 4 GiB
(`MAX_DECOMPRESSED_BYTES` in `src/usdc/reader.rs`), so a crafted binary layer
can demand far more memory than its file size. Ordinary files decompress only
to their real size. Configurable decode budgets in openusd would remove this
limitation.

## Licence obligations

- `openusd` declares MIT (copyright 2024 Maksym Pavlenko) but its crates.io
  package ships no root licence file. The upstream `LICENSE` at tag `v0.8.0`
  (https://raw.githubusercontent.com/mxpv/openusd/v0.8.0/LICENSE, SHA-256
  `ed864f20ae0294c6b61b3c88bed43bedf46d93f90c8cbc0bb4597c5131c6bb6f`) is
  recorded at `docs/third-party/notices/openusd-0.8.0/LICENSE`, and
  `scripts/rust-notice.py` reproduces recorded texts for crates whose package
  omits them.
- `src/usd/core_schemas.rs` is generated from OpenUSD's schema definitions and
  is covered by the Tomorrow Open Source Technology License 1.0, which is
  Apache-2.0 with a different trademark section. The package's
  `third-party/LICENSE` and `third-party/NOTICE.txt` (Copyright 2016 Pixar) are
  reproduced in `NOTICE`; `scripts/rust-notice.py` reads a crate's
  `third-party/` directory for this reason.

No copyleft or proprietary code is adopted. No code from these packages is
copied into Domowik; they are linked dependencies.

## Public-format provenance

Model interpretation and the export scene description are implemented from
public documentation only:

- Alliance for OpenUSD, Core Specification 1.0 (December 2025):
  https://aousd.org/ (layers, prims, attributes, composition).
- OpenUSD documentation at https://openusd.org/release/ for `UsdGeom`
  (`Mesh`, `Xform`, `metersPerUnit`, `upAxis`, `primvars:displayColor`),
  `UsdShade`/`UsdPreviewSurface` (`diffuseColor`, `opacity`, `UsdUVTexture`)
  and the USDZ package rules (uncompressed ZIP, 64-byte data alignment,
  first file is the root layer).

Source consulted: the `openusd` 0.8.0 crates.io package (MIT), read for its
public API and for the contract of its `ar::Resolver` trait, including its
`DefaultResolver` as an example. Domowik's in-memory resolver and USD readers
are independently written against that API; no code was copied or translated.
No Pixar OpenUSD C++ source was consulted.

Test models are generated by Domowik's own tests; no third-party USD samples
are committed.
