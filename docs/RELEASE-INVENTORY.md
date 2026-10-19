# Release inventory: SBOMs, licences and notices

**CLI validation is active; GUI bundle coverage is staged and incomplete.**
A successful source build or inventory check does not by itself authorize a
binary release. Follow the [third-party policy](THIRD-PARTY-POLICY.md).

## Tools and inputs

| Tool | Purpose |
| --- | --- |
| `scripts/release-sbom.py` | Deterministic CycloneDX 1.6 source/artifact SBOMs |
| `scripts/release-validate.py` | Fail-closed artifact, licence, embed and inventory validation |
| `scripts/releaselib.py` | Shared graph, digest and determinism implementation |
| `scripts/ci/release-inventory-test.sh` | Negative controls for validation refusals |

Inputs are `release/rust-manifest.json`, the toolchain records in
`docs/third-party/`, the reviewed licence allow-list and
`docs/third-party/assets.json`. Output belongs in ignored `build/release/`,
not committed host-specific artifact inventories. The tools are original
Apache-2.0 Python code using the standard library.

## Coverage and refusal behavior

Artifact graphs use target-filtered Cargo metadata and the linked dependency
closure. Build dependencies and proc macros are excluded from runtime scope.
Statically linked `rust-std` comes from its toolchain record, since Cargo
metadata does not describe it. The frontend has its own Cargo workspace.

Explicit embed/copy records cover fonts and other files that package metadata
cannot see. The validator checks source embeds, file digests, licence text and
whether declared records are still used. It also regenerates NOTICE and the
Rust dependency inventory unless explicitly instructed to rely on earlier
verification stages.

| Refusal tag | Condition |
| --- | --- |
| `empty-artifact-set` | No artifacts declared |
| `undeclared-embed` | Compiled-in file missing from the asset record |
| `stale-asset-row` | Missing, changed or unused asset record |
| `unreviewed-licence` / `undeclared-licence` | Linked dependency lacks reviewed terms |
| `toolchain-drift` / `target-gap` | Compiler/record mismatch or missing target record |
| `notice-stale` / `inventory-stale` | Generated files disagree |
| `missing-artifact` | Required built binary is absent |
| `incomplete-coverage` | Required artifact's coverage remains staged |

The allow-list is shared with `scripts/rust-deps-inventory.py`. Negative tests
change one input at a time and require the expected refusal, not merely any
nonzero exit.

## Determinism and toolchains

SBOM output depends on source/artifact bytes and an epoch: valid non-negative
`SOURCE_DATE_EPOCH`, or the HEAD commit time. Components are sorted and the
serial number is content-derived. Verification compares repeated output.
This does **not** establish that different hosts produce identical binaries.

Rust records pin channel, upstream commit and distribution-archive hashes.
Verification checks the installed compiler and target coverage; it does not
download and hash every archive against those recorded digests. Do not describe
recorded hashes alone as verification of a fetched toolchain.

## Staged GUI bundles

The macOS application inventory includes resolved Rust and Dart graphs but still
needs licence review of actual Flutter engine/platform frameworks and bundled
assets. Dart lockfiles contain no package licence texts; archives need review.
The upstream Rust COPYRIGHT obligations must also be accounted for.

The browser bundle declares two staged artifacts sharing `frontend/build/web`:
the core (`wasm32-wasip1`) and WebGPU renderer (`wasm32-unknown-unknown`).
CanvasKit, compiled Dart, generated JavaScript and fonts also ship in that
bundle. Scanning a built bundle lists files/digests; it does not resolve their
licences. `--release-grade` refuses to call incomplete coverage complete.
Reviewed font and generator records remain in `docs/third-party/`.

Use the Rust `wgpu` dependency directly. A similarly named package is not
automatically the same implementation, version or licence.

## Before publishing binaries

- Complete staged GUI licence/notice coverage and review all distributed assets.
- Include required licences and NOTICE content with each artifact; generating
  notices in the source repository alone does not package them into archives.
- Validate the actual built artifacts and generate their SBOMs.
- Review platform identifiers, signing/notarization and any release claims.
- State reproducibility and platform limits honestly; neither follows merely
  from a successful local build.

`scripts/verify.sh` exercises inventory and release validation. The unsigned
developer-build workflow is a separate publication path, not proof that these
remaining release obligations are satisfied.
