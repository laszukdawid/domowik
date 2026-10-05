# Release Inventory: SBOMs, Licenses, and Notices

Status: **Active for the CLI binary. Staged for the GUI bundle. No release is
authorised.** (Issue 0036, rebuilt for Rust 2026-09-12.)

Domowik generates a deterministic CycloneDX SBOM for its source tree and for
each declared artifact, and validates a release fail-closed before one could be
cut. That covers `domowik`, the command-line binary, completely. It covers the
Flutter application bundle's Rust and Dart halves and refuses to call the rest
covered. Nothing here publishes, tags, uploads, or reserves anything, and the
first public binary release remains ungated by this document alone.

The Go pipeline that used to do this froze with the Go product it described
(ED-0091); its source archive was removed on 2026-10-05 and remains in earlier
Git commits. Its design is preserved at the end of this page, because a
replacement should be argued against it rather than written as though the
problem were new.

## What runs

| Script | Role |
| --- | --- |
| `scripts/release-sbom.py` | Emits CycloneDX 1.6 JSON for the source tree or one artifact. |
| `scripts/release-validate.py` | Refuses a release on any unreviewed, undeclared, stale, or unbuilt component. |
| `scripts/releaselib.py` | The graph, digest, and determinism code both of the above share. |
| `scripts/ci/release-inventory-test.sh` | Breaks a copy of the tree eleven ways and checks each refusal fires. |

The declarative inputs are `release/rust-manifest.json` (what is distributed),
`docs/third-party/toolchain-rust.json` (the pinned compiler and standard
library), and `docs/third-party/assets.json` (files compiled into a binary that
no build tool can see). Generated documents go to `build/release/`, which is
not committed.

All three scripts are first-party, Apache-2.0, and use only the Python 3
standard library. No third-party SBOM or licence-scanning tool is linked or
vendored. Adopting one would mean reviewing a new dependency in order to
describe dependencies, and it must be recorded here and in
`docs/third-party/tools.json` under
[THIRD-PARTY-POLICY.md](THIRD-PARTY-POLICY.md) before use.

## Why this is not the dependency inventory again

`docs/RUST-DEPENDENCIES.md` and `NOTICE` already fail the build when a
dependency changes, so the obvious objection is that this adds a second layer
of paperwork over the same facts. It does not, and the difference is worth
being precise about, because four of the things a release needs to know are
invisible to those two files.

The dependency inventory lists every crate on every platform on purpose:
over-reporting is the safe direction for a licence guard, which is why it
names `r-efi` and `linux-raw-sys` on a macOS build where neither is ever
compiled. For a document claiming to say what is *inside* a binary,
over-reporting is a false statement. Artifact SBOMs use `cargo metadata
--filter-platform`, which is 100 packages against 123 unfiltered on
`aarch64-apple-darwin`, and the linked closure of the CLI binary is 58
third-party crates.

`rust-std` is statically linked into every binary Domowik produces and `cargo
metadata` cannot see it. It comes from the toolchain record instead, carrying
the rustc version, the upstream commit, and the SHA-256 of the official
distribution archive for that target.

Proc-macro and build-dependency crates are marked `excluded` scope and are not
shipped. Their code runs inside the compiler; only its output reaches the
artifact. Listing `syn` as a distributed component would put it in a notice
bundle it has no claim on.

`docs/third-party/assets.json` is maintained by hand, so before this nothing
checked it against the source. An `include_bytes!` added without a matching
row changed no generated file and failed no stage. The validator scans tracked
`.rs` sources for embeds and refuses an undeclared one, and refuses a declared
row whose digest has moved or that nothing embeds any more.

The validator also regenerates `NOTICE` and `docs/RUST-DEPENDENCIES.md` and
diffs them, duplicating two `verify.sh` stages deliberately. A release
validator that trusted files it had not regenerated would pass a tree whose
notice went stale after the last full run, which is the failure ED-0094 built
the diff to prevent.

## What it refuses

Every check refuses rather than warns. `scripts/ci/release-inventory-test.sh`
breaks one thing at a time in a copy of the tree and asserts the matching tag
appears, because a gate nobody has watched reject anything is not evidence that
it can reject anything.

| Tag | Refused when |
| --- | --- |
| `empty-artifact-set` | the manifest declares nothing to ship |
| `undeclared-embed` | a file is compiled in with no row in the assets record |
| `stale-asset-row` | a declared asset is missing, unembedded, or its digest moved |
| `unreviewed-licence` | a linked crate declares a licence off the reviewed allow-list |
| `undeclared-licence` | a linked crate declares no licence at all |
| `toolchain-drift` | installed rustc, `rust-toolchain.toml`, and the record disagree |
| `target-gap` | a declared target has no `rust-std` row |
| `notice-stale` | `NOTICE` no longer matches its generator |
| `inventory-stale` | `docs/RUST-DEPENDENCIES.md` no longer matches its generator |
| `missing-artifact` | `--require-built` was asked for and the binary is absent |
| `incomplete-coverage` | `--require-built` on an artifact whose inventory is staged |

The allow-list is `ALLOWED` in `scripts/rust-deps-inventory.py`, imported
rather than copied. `allowedLicenses` in `docs/third-party/licenses.json` is
the Go-era registry and is consulted by nothing in the Rust path;
`assets.json` already says so about its own Noto entry.

## Determinism

Output is a function of the tree and an epoch, never of the clock. The epoch is
`SOURCE_DATE_EPOCH` when it is set to valid non-negative unix seconds, and the
HEAD commit time otherwise; anything else fails closed. Components are sorted,
and the CycloneDX `serialNumber` is derived by hashing the document's own
content rather than generated randomly. Two runs over one tree are
byte-identical, which the test script asserts with `cmp` for both the source
and the artifact document.

Reproducibility of the *binary* is a separate claim and is not made here.
Nothing in this repository has demonstrated that two hosts building
`domowik` produce identical bytes, so artifact SBOMs are generated rather than
committed: a committed document would carry a digest true only on the machine
that wrote it.

## The GUI bundle, and what "staged" means

The actively developed frontend is not yet a release, but its locally runnable
macOS bundle redistributes a Flutter engine, an AOT-compiled
Dart runtime, plugin frameworks, and a Material icon font. Declaring the
artifact `staged` in `release/rust-manifest.json` makes that obligation
countable instead of invisible.

Implemented and running today: the bridge crate's resolved graph, read from its
own Cargo workspace because it is excluded from the root one; the standard
library from the toolchain record; and all 26 packages in
`frontend/pubspec.lock`, classified by dependency class and source.

Not covered without a built bundle: the Flutter engine framework and its
version, the plugin and platform frameworks actually embedded with their
digests, and the fonts and other files shipped in `flutter_assets`. Running
`release-sbom.py artifact --id domowik-gui-macos` emits a document explicitly
marked incomplete and naming those gaps; adding `--release-grade` turns the
admission into a non-zero exit. When a bundle is present, `--bundle` scans the
frameworks and `flutter_assets` and records what is shipped with digests. It
resolves no licences for them, which is the next staged step and belongs with
issue 0024.

Two licence gaps are named rather than papered over. `pubspec.lock` records no
licence for any Dart package, and resolving them needs the package archives,
which this generator does not fetch. The upstream Rust `COPYRIGHT` file, which
is where `compiler-builtins` carries its `Apache-2.0 WITH LLVM-exception`
terms, is not committed here; it ships inside the `rust-src` archive pinned in
the toolchain record.

A supply-chain trap worth recording, found by the issue-0025 worker: the
crates.io package literally named `wgpu-native` is version 0.5.1 from 2020 and
is MPL-2.0, which is weak copyleft and on no allow-list here. ADR 0003 names
"wgpu-native" by name, so a reader taking that literally would pull it into the
distributed graph. The live project of that name is a separate C-API wrapper on
GitHub under Apache-2.0. Depend on `wgpu` directly.

## Toolchain record policy

`docs/third-party/toolchain-rust.json` pins the toolchain by channel, by the
`rustc -vV` version string, and by the upstream git commit, with SHA-256 hashes
for the `rustc`, `cargo`, `rust-std`, and `rust-src` distribution archives
across the four supported targets. Those targets are the ones CI actually
builds a binary for: `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu`
in the cross-build job, and both darwin triples on the macOS runner.

The hashes were captured on 2026-09-12 by a read-only fetch of
`https://static.rust-lang.org/dist/channel-rust-1.98.0.toml`, authorised by
root for that purpose, and transcribed by script rather than by hand. They pin
the record to exact upstream artifacts. **Nothing downloads or verifies an
archive against them**, exactly as `toolchain-go.json` says of its own. Add an
explicit download-and-hash step before relying on them to verify a fetched
toolchain. What is verified on every run is that the installed rustc,
`rust-toolchain.toml`, and this record agree, and that every declared target
has a `rust-std` row.

## What is still owed

Current integration (2026-10-04): `scripts/verify.sh` runs release validation
against the built CLI and then the full declared artifact set, followed by
deterministic SBOM comparisons. Its `--skip-generated` validator calls rely on
the earlier notice/inventory regeneration stages; standalone validation checks
generated files by default. Harness regression tests also run in CI. This
integration does not close staged GUI bundle coverage or authorize a release.

Remaining obligations:

- **Issue 0036 criterion 4 stays unmet.** The notice bundle is generated and
  reviewable; "included before the first public binary release" cannot be
  closed until a release is authorised and happens.
- **Licences for Dart packages and bundle frameworks.** Named above, owned with
  issue 0024.
- **Binary reproducibility.** Unproven, and therefore unclaimed.

## Appendix: the frozen Go pipeline

**Read every "is" below as "was".** This is the issue-0036 pipeline as it stood
before ADR 0008, kept because the Rust process above should be argued against a
worked design rather than written as though the problem were new. The code is
`internal/release`, `cmd/domowik-sbom` and `release/`, all of which moved to
`_reference/go/` with the Go core. That archive was removed on 2026-10-05;
this appendix describes a historical design, not runnable source in the
current checkout. Earlier committed sources remain in Git history.
Its `verify.sh` stage was withdrawn: the
manifest enforces `artifacts must not be empty` and describes two linux
binaries built from `./cmd/domowik`, which no longer exists. Pointing it at a
build tool would have been a lie and relaxing the guard would have weakened a
fail-closed check, so the machinery was frozen with its subject.

### Purpose and scope

Before any public binary release, Domowik must know exactly what it
distributes and what obligations accompany it. The pipeline described here:

- discovers the component set dynamically from live Go metadata at every run,
- audits that set fail-closed against a reviewed license registry,
- emits deterministic SPDX 2.3 SBOMs for the source tree and every declared
  binary artifact,
- generates `NOTICE` from verbatim committed third-party license texts,
- produces a stable consumption manifest (`build/release/manifest.json`) with
  exact artifact hashes for issue 0018's release checks.

Issue 0018 consumes these artifacts; publication/upload automation stays out
of scope here.

### Declarative inputs (reviewed, committed)

Reviewed policy and provenance inputs live under `docs/third-party/`;
the declarative distribution manifest lives under `release/`. All change
only through review, and generated outputs are never written into the
policy-input tree:

| File | Role |
| --- | --- |
| `release/manifest.json` | Distribution source of truth: artifacts, output locations, epoch policy, bundled-file declaration. |
| `docs/third-party/licenses.json` | Reviewed license registry: allow-list plus one entry per known component. |
| `docs/third-party/toolchain-go.json` | Pinned Go toolchain record: exact version, download URLs, SHA-256 hashes, license, platforms, revisit triggers. |
| `docs/third-party/tools.json` | Pinned CI/dev tool records (GitHub Actions SHAs, first-party generator status) with revisit triggers. |
| `docs/third-party/notices/` | Verbatim upstream license texts embedded into NOTICE, hash-bound to registry entries. |

### Bundled files

Every file discovered via go:embed — first-party or third-party — must be
declared in `manifest.json` under `bundled` as a file path plus the ARTIFACT
IDs whose binaries must contain it (never bare os/arch targets: artifacts can
share a platform while having different dependency closures), AND carry a
reviewed registry row (`kind: "bundled"`, content-hash pinned). The generator
cross-checks go:embed discovery against this declaration per artifact;
missing declarations and stale declarations both fail closed. Staleness is
judged exactly once, over the union of all targets' discoveries, so a
target-specific row is never misreported by another target's pass.

### Dynamic discovery and fail-closed audit

Nothing is assumed from a previous run. Each generation:

1. resolves modules via `go list -m -json all`, cross-checked against
   `go list -m all` (replacement-aware);
2. inspects embedded files per artifact via `go list -json -deps <package>`
   with the target's GOOS/GOARCH/CGO_ENABLED=0 environment, plus a repo-wide
   per-target hidden-embed guard scan;
3. builds each declared artifact statically (`CGO_ENABLED=0 -trimpath
   -buildvcs=false`) and reads its build metadata;
4. compares the binary's component identities exactly against the resolved
   source graph (extras, omissions, duplicates, and version conflicts fail;
   `(devel)` and empty versions compare equal);
5. audits every discovered module, the toolchain, and bundled files against
   the registry — missing, unreviewed, kind-mismatched, incompatible-license,
   or stale rows all fail closed.

Go invocations are pinned offline and read-only: `GOPROXY=off`,
`GOFLAGS=-mod=readonly`, `GOWORK=off`, `GOTOOLCHAIN=local`. Generation cannot
fetch modules or mutate go.mod/go.sum. File access is root-safe by construction. Writes reject symlinked parent
components and symlinked final entries outright. Reads (embedded files,
tracked sources, NOTICE texts) resolve symlinked parents but require the
resolved location to stay inside the repository and the final entry to be a
regular non-symlink file; absolute paths and any upward traversal are always
rejected. Fixed policy inputs load through the same root-contained reader,
so both symlinked parent directories and symlinked final entries fail closed
at production load time.

### Determinism and SOURCE_DATE_EPOCH

Generation output is a pure function of (tree state, epoch):

- If `SOURCE_DATE_EPOCH` is set it must be valid non-negative unix seconds;
  anything else fails closed.
- Otherwise the epoch derives immutably from the HEAD commit time.
- Wall-clock time is never used.
- Two runs on the same input produce byte-identical documents (tested).
- SPDX documents carry a creation timestamp; verification normalizes exactly
  that field and compares everything else byte-for-byte. NOTICE and the
  consumption manifest contain no timestamps and compare as raw bytes.

Binaries are built without VCS stamping so their hashes are stable across
commits; generation additionally requires a clean checkout (verify never
allows a dirty tree), which binds each commit containing inventory artifacts
to the exact source they describe.

### Emitted artifacts

- `release/sbom/source.spdx.json` — complete tracked-source SBOM (every git-
  tracked file except generated outputs, hashed; resolved module components;
  first-party files classified Apache-2.0; verbatim third-party notice texts
  classified with their upstream license).
- `release/sbom/<artifact>.spdx.json` — authoritative per-binary SBOM derived
  from the built binary's own metadata: exact binary SHA-256, main module,
  dependency modules, toolchain (with pinned artifact hashes), and per-target
  embedded files. A source-only SBOM is never treated as proof of a cross-
  built binary's contents.
- `NOTICE` — project notice plus verbatim third-party texts for every
  `noticeRequired` registry row; hash-bound to `docs/third-party/notices/`.
- `build/release/<artifact>` and `build/release/manifest.json` — local
  release evidence: the exact hashed binaries and the stable consumption
  manifest (schema, generator name/version/provenance, toolchain record,
  artifact hashes, SBOM/NOTICE paths+hashes). These are consumed by issue
  0018; verification materializes or hash-checks them but cleanup never
  deletes them.

### Generator provenance and pinning

The generator is **first-party, project-owned code** licensed Apache-2.0:

- Source: `cmd/domowik-sbom` (CLI surface) and `internal/release`
  (pipeline library), inside this repository.
- Identity: `domowik-sbom`, pinned version constant `GeneratorVersion =
  1.1.0` in `internal/release/spdx.go`, mirrored by the tools record row in
  `docs/third-party/tools.json`; LoadInputs fails closed if the two disagree
  or if the row count is not exactly one. Any change that can alter emitted
  bytes must bump both together and update this section.
- It is a development-time tool, not distributed with Domowik artifacts; its
  own identity appears in SBOM creator fields and the consumption manifest.
- No third-party SBOM or license-scanning tool is currently linked or
  vendored; if one is ever adopted, it must be recorded here and in
  `docs/third-party/licenses.json` under docs/THIRD-PARTY-POLICY.md before use.

SPDX document format: SPDX-2.3 JSON, data license CC0-1.0, deterministic
content-hash namespaces. License identifiers are restricted to single SPDX
identifiers (no expression operators) so every obligation maps to one exact
license.

### Toolchain record policy

`docs/third-party/toolchain-go.json` pins the Go toolchain with canonical go.dev URLs and
SHA-256 hashes captured from https://go.dev/dl/?mode=json&include=all,
including the source tarball. Filename, URL, hash, and size are validated to
agree; the hashes are provenance pins binding this record to exact upstream
artifacts — the pipeline verifies the installed Go version against the record
fail-closed on every run but does not download archives to verify the hashes.
Before using these hashes for supply-chain verification of downloaded
toolchains, add an explicit download-and-hash verification step. Revisit triggers are recorded in
the file and include any version change, new target platforms, relevant Go
security releases, and license/channel changes. CI verifies the live
toolchain version against the record on every run.

### Registry governance

- Allowed baseline licenses are the immutable permissive set: Apache-2.0,
  BSD-2-Clause, BSD-3-Clause, CC0-1.0, ISC, MIT, Zlib. Anything else in
  allowedLicenses must be paired with a licenseDecisions entry pointing at an
  existing reviewed decision document (ADR) — validation fails otherwise, so
  copyleft or proprietary admission cannot bypass docs/THIRD-PARTY-POLICY.md.
- The tools record declares no distributed or packaging CI tooling; that
  state is unsupported by design — every nonempty list fails validation
  outright, so enabling distribution requires an explicit schema change plus
  full third-party review rather than ordinary registry admission.
- Toolchain download hashes are provenance pins; the pipeline verifies the
  installed Go version fail-closed but does not download archives to verify
  the hashes (see the toolchain record notes).
- Entries must be `reviewed` and carry complete provenance: exact https
  source URL, version pin (`(devel)` for the moving source tree, resolved
  versions for dependencies/toolchain, `sha256:` content pins for bundled
  files), a human review date (`reviewedAt`), non-empty revisit triggers,
  an added date, and, when `noticeRequired`, verbatim notice files with
  SHA-256 bindings. Audit compares resolved component versions against the
  pin and fails on drift.
- Stale entries (registry rows with no discovered component) fail the audit,
  keeping the registry equal to reality.
- The toolchain record and its registry entry must agree on key, license id,
  and license URL; disagreement fails closed.
