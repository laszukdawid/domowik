# 0036: Generate release SBOM, scan licenses, and assemble notices

- **Phase:** 1
- **Priority:** High
- **Status:** In Progress (partial scope)
- **Depends on:** 0008

## Problem

Before the first public binary release, turn the scaffold's dependency and
runtime inventory into complete release artifacts: machine-readable SBOMs,
automated license checks, and required third-party notices. A stdlib-only
module graph at scaffold time does not prove the contents or obligations of
future distributed artifacts.

## Scope

Cover source and each distributed binary separately. Select and pin the SBOM
and license-scanning tools under the third-party policy, record their own
licenses and provenance, and integrate reproducible checks into release CI.
Do not redefine issue 0018's format, migration, or publication decisions.

## Acceptance criteria

- [x] A documented SPDX or CycloneDX SBOM is generated reproducibly for the
      source tree and every distributed binary artifact.
- [x] SBOM component sets are checked against resolved Go modules, embedded
      build information, bundled files, the Go toolchain/stdlib/runtime, and
      CI or packaging tools that are distributed with the release.
- [x] Automated license scanning fails closed on missing, unknown, or
      incompatible licenses and records direct/transitive notice obligations.
- [x] `NOTICE` and any required third-party notice bundle are generated,
      reviewed, and included before the first public binary release.
- [x] Tool versions, licenses, source URLs, hashes, platform support, and
      maintenance/revisit triggers are recorded under the third-party policy.
- [x] Issue 0018's release checks consume these artifacts without treating a
      source-only SBOM as proof of a cross-built binary's contents.

## Closure

- **Implementation/spec commits:** branch range `d296ddb..b04c997`, integrated
  by main-repository merge `ab2d847`; exact integrated-tree SBOM refresh
  `71e8544`.
- **Verification:** paired adversarial review approved; clean-clone tests and
  `./scripts/verify.sh` passed on the branch and the final combined main tree.
- **Follow-up issues:** 0018 consumes the generated release manifest and must
  rerun the fail-closed inventory for the actual release contents.

## Status corrected by ED-0096 — 2026-09-01

**In Progress (partial scope).** Restored from `done/`. The SBOM, licence scanning and notice machinery was built for the Go product ADR 0008 froze, and this issue's criteria are about "every distributed binary artifact" — of which there are now none, and none of them Go. What SURVIVES and is enforced fail-closed: the Rust dependency inventory regenerated from `cargo metadata` and diffed, so adding a dependency fails the build until its licence is reviewed, and `NOTICE` generated from the dependency graph across 83 distributed components. Neither is a release process or a binary SBOM.

## Rust release machinery built — 2026-09-12

The criteria above were written against the Go product and are **superseded,
not deleted**. They say "resolved Go modules" and "the Go toolchain/stdlib/
runtime" about a toolchain this project no longer ships, so leaving them as the
live checklist would make the issue unclosable by anyone reading it literally,
while deleting them would erase what was actually validated in August. The
restatement below maps each one onto the Rust product. The obligation is
unchanged; only the toolchain it names is.

| # | Restated for Rust | State |
| --- | --- | --- |
| 1 | A deterministic CycloneDX SBOM is generated for the source tree and for each declared artifact. | Met |
| 2 | Component sets are checked against the resolved Cargo graph, the standard library, embedded files, and the pinned toolchain. | Met for the CLI. Staged for the GUI bundle. |
| 3 | Licence scanning fails closed on missing, unknown or incompatible licences. | Met |
| 4 | `NOTICE` and the notice bundle are generated, reviewed, and included before the first public binary release. | **Unmet, and not closable here.** |
| 5 | Tool versions, licences, source URLs, hashes, platform support and revisit triggers are recorded. | Met |
| 6 | Issue 0018's release checks can consume these without treating a source-only SBOM as proof of a binary's contents. | Met |

Criterion 4 stays open on purpose. The machinery exists and `NOTICE` is
generated and diffed on every run, but "included before the first public binary
release" is a claim about a release, and no release is authorised. It closes
when one happens, not before.

### What landed

`scripts/release-sbom.py` emits CycloneDX 1.6 for the source tree or one
artifact; `scripts/release-validate.py` refuses a release on eleven distinct
grounds; `scripts/releaselib.py` holds the graph, digest and determinism code
they share. Declarative inputs are `release/rust-manifest.json`,
`docs/third-party/toolchain-rust.json` and the existing
`docs/third-party/assets.json`. All three scripts are first-party, Apache-2.0
and Python-3-standard-library only, so describing the dependency surface adds
nothing to it. Design and rationale are in
[docs/RELEASE-INVENTORY.md](../docs/RELEASE-INVENTORY.md); the frozen Go
pipeline moves to an appendix there rather than being dropped.

Four things the existing guards could not see are now covered. Artifact
documents are target-filtered, so the CLI's 58 linked third-party crates are
reported instead of the workspace-wide 123 that `docs/RUST-DEPENDENCIES.md`
lists on purpose. `rust-std` is recorded as a distributed component, which
`cargo metadata` cannot report. Proc-macro and build-dependency crates are
marked `excluded` scope, because their code runs in the compiler rather than
shipping. And tracked `.rs` sources are scanned for `include_bytes!` and
`include_str!`, which closes a real hole: `assets.json` is hand-maintained, so
until now an undeclared embed changed no generated file and failed no stage.

`scripts/ci/release-inventory-test.sh` proves the refusals fire. It copies the
tree, breaks one thing, and asserts the matching tag appears, for all eleven
grounds plus determinism and the licence-guard cases. Eighteen checks, exit 0.

### The GUI bundle is staged, not excused

Root's direction on 2026-09-12: a spike is not the same as unshipped for a
licence inventory, and permanent refusal of the artifact kind is not an
acceptable answer. The issue-0024 macOS bundle runs locally and redistributes a
Flutter engine, an AOT Dart runtime, plugin frameworks and a Material icon
font.

Covered and running: the bridge crate's graph read from its own workspace, the
standard library, and all 26 packages in `flutter_app/pubspec.lock`. Not
covered without a built bundle: the engine framework and its version, the
embedded plugin and platform frameworks with digests, and the `flutter_assets`
contents. Those gaps are named in the emitted document, and `--release-grade`
exits non-zero rather than footnoting them. Closing them belongs with issue
0024.

Root also asked whether the Noto font reaches the bridge through a CLI
dependency. Checked against the graph rather than the asset path, and it does
not: `cargo tree -e normal` on `spikes/0024-flutter-rust-bridge` contains no
`domowik-cli`, `tiny-skia`, `ttf-parser` or `png`. The GUI's font obligation
comes from Flutter's own MaterialIcons instead.

### Two defects found in existing code

The licence guard compared declared expressions by exact string, so the same
dual licence in a different word order failed the build. This is not
hypothetical: seven of the seventeen entries in `ALLOWED` exist only to spell a
set already listed, and `MIT OR Apache-2.0`, `Apache-2.0 OR MIT` and
`MIT/Apache-2.0` are one licence written three ways. Found by the issue-0025
worker against the wgpu graph, where `raw-window-handle` and `rustc-hash` were
refused for word order alone. Expressions joined by a single operator now
compare by licence set; anything parenthesised or mixing `AND` with `OR` still
compares exactly. `docs/RUST-DEPENDENCIES.md` is byte-identical afterwards, so
nothing new is admitted.

**Resolved by root on 2026-09-12 (R6).** `libloading` 0.8.9 declares `ISC`,
which was genuinely absent from `ALLOWED` and was a real review rather than a
spelling problem. Root read the pinned upstream `LICENSE` and `Cargo.toml` at
`raw.githubusercontent.com/nagisa/rust_libloading/0.8.9/` and admitted ISC as a
bounded dependency review: a permissive grant whose only condition is that the
copyright notice and permission notice appear in all copies.

Root was explicit about the framing and it is recorded verbatim in the
allow-list: this is **technical project-policy acceptance, not legal clearance,
and not the owner personally reviewing the licence**. Nothing in this
repository should describe it as either.

The graph delta is **zero**. No crate in the current resolved graph declares
ISC, `libloading` is absent from all four ship triples, and
`docs/RUST-DEPENDENCIES.md` regenerates byte-identical. ISC is admitted ahead
of the dependency, which reaches a distributed closure only through `wgpu` in
the issue-0025 spike.

The obligation is dischargeable, and the mechanism is tested rather than
assumed. `scripts/rust-notice.py` reproduces each runtime crate's own licence
text verbatim, and the suite exercises that path against a synthetic ISC crate.
It also checks the real one when the local registry holds it: `libloading`
0.8.9 ships a single `LICENSE`, declares `license = "ISC"`, and its text
carries the permission notice. That check announces a skip when the crate is
absent rather than passing in silence.

One test had to change, and the change is worth recording. The canonicalisation
block asserted ISC was refused, which was correct until R6. It failed on the
day ISC was approved, which is the suite working: an allow-list change that no
test noticed would be the defect. The assertion moved to the ISC block as a
positive rather than being deleted, so admitting a licence is still stated
somewhere a reader will find it.

`scripts/ci/verify-job-summary.sh` claimed an exit-status file that was
"absent, empty, multiline, or carrying a single non-digit character" became
`unknown`, and did not have that strictness: `raw=$(cat ...)` strips every
trailing newline, so `0\n\n\n` was accepted and reported a verified pass.
Measured before and after. This never produced a wrong verdict, because command
substitution can only hide newlines and the digit run was always the file's
sole non-empty line; `0\n1\n` and `\n\n0\n` were already `unknown`. The
defect was the claim, not the verdict. The read is now byte-exact and accepts
digits plus at most one final newline. Seven regression cases were added
through a new raw-bytes harness, because the existing `case_check` writes with
`printf '%s\n'` and therefore cannot express any of these shapes, which is why
the hole survived this long.

### The licence guard does not see spike workspaces

Raised by the issue-0025 worker on 2026-09-12 and verified here.
`scripts/rust-deps-inventory.py` runs `cargo metadata` with no
`--manifest-path`, so it resolves the root workspace only. Spike crates are
their own workspaces, excluded from the root one by ED-0097 so a spike cannot
join the distributed build graph by accident, and that same exclusion hides
them from the guard.

The structural point is right. The impact claim that came with it is not, and
the difference matters for how urgently this needs fixing. All 54 third-party
crates in the issue-0024 bridge spike's closure are already in
`docs/RUST-DEPENDENCIES.md`: the set difference is empty, because the spike
depends only on the core crates plus serde, serde_json and uuid, every one of
which the root workspace already pulls. So no crate is currently unaudited.
The gap is latent, and it opens the moment a spike adds something the root does
not have, which is the wgpu case exactly.

`scripts/release-validate.py` already covers it. Licences are audited per
artifact, and `release/rust-manifest.json` declares the Flutter bundle with the
spike as its workspace root, so the spike's linked closure (34 crates) is
checked under the same allow-list. The real remaining gap is that the validator
is not a `verify.sh` stage yet, which is the pending cost decision, and this
adds a reason to take it.

Extending the dependency inventory itself to spikes was considered and
rejected. Its two kinds are `runtime` (distributed) and `dev` (test and
benchmark tooling). A spike crate is neither, so it would need a third
classification and would blur the distinction ED-0097 exists to keep sharp. The
scope statement in the generator now records the gap and names what covers it,
rather than leaving it silent.

### The verify.sh stages, costed rather than argued

Root asked for a cost and duplication review before wiring anything. The patch
is `docs/evidence/0036-verify-stages.patch`, prepared against a copy so this
branch's `scripts/verify.sh` stays untouched while another worktree is running
verification. It adds two stages and not four.

Measured on this host with a warm cargo cache, which is the honest caveat: the
`release inventory` stage takes 0.25s and `release sbom determinism` takes
0.37s, so the pair costs under a second against a run already dominated by
`cargo build --workspace --all-targets` and a fat-LTO release build. Cold, both
are a `cargo metadata` invocation slower. This is not a cost worth trading a
gate for.

The duplication root flagged is real and is removed rather than accepted. The
validator regenerates `NOTICE` and `docs/RUST-DEPENDENCIES.md` by default, and
two stages earlier `verify.sh` has already done exactly that. The new
`--skip-generated` flag drops those two checks for that one caller. It is
wrong anywhere else, and standalone the default stands, because a release gate
trusting files it had not regenerated is the ED-0094 failure. The flag prints
what it gave up, so a log cannot hide which checks ran, and two tests pin its
edges: it still refuses an undeclared embed, and it does drop the NOTICE check.

`scripts/ci/release-inventory-test.sh` is deliberately NOT a `verify.sh` stage.
It copies the tree fourteen times and runs `cargo metadata` on each copy, which
is around ninety seconds for something that tests the harness rather than the
product. It belongs beside `verify-job-summary-test.sh` in the "Test the CI
reporting scripts" step of `ci.yml`, which already runs the tests of the
reporting code before the thing it reports on. That edit is in `ci.yml`, which
is not mine to make unilaterally, so it is named here for root.

The stage is placed after the CLI probes rather than beside the licence stages
because it needs `target/release/domowik` to exist. Earlier, the validator
refuses `missing-artifact`, correctly and uselessly.

### What the verification run actually attests

Recorded precisely because a passing banner is worth only the tree it was run
on, and this branch's history now makes that easy to overstate.

`./scripts/verify.sh` produced `=== VERIFICATION PASSED === all stages` with a
captured exit status of 0 and zero skipped stages against commit **`6f58337`**,
at a moment when `finalize/0036` and `main` were the same tree. It attests that
commit and nothing else.

It does **not** attest current `main`. Main advanced to `5a5c7a8` afterwards,
62 commits beyond the verified tree, carrying work from issues 0026, 0029,
0038, 0059 and 0064. `6f58337` remains an ancestor of `main`, so the verified
state is in its history, but an ancestor being green says nothing about its
descendants. Any statement pairing that banner with a later `main` head is
historical and should be read as such.

It also does not cover `536b61d`, the ISC admission on this branch, which
postdates the run. What `536b61d` has is its own evidence: 24 checks in
`scripts/ci/release-inventory-test.sh` at exit 0 with no skips, and byte-
identical regeneration of `NOTICE` and `docs/RUST-DEPENDENCIES.md`. Integrating
it needs a fresh merge of `main` and a fresh full run, under whoever holds the
verification slot.

### Two review findings from root, both upheld (R18, R19)

**The ISC notice test proved nothing.** It called `licence_files()` and then
read the file back itself, never invoking the generator's output path. Root
caught it. Demonstrated before repairing: with the licence-text emission
deleted from `scripts/rust-notice.py`, the old assertion still passed.

It now asserts on real generator stdout. The crate path is mutation-proven —
removing the emission drops the output from 879,068 to 23,947 bytes and the
anchor disappears — and the asset path carries genuine ISC text through the
real generator, asserting the copyright line and "this permission notice
appear", with omission proven red and reported rather than silent. The
cached-crate grep is retained, labelled as provenance only, which is all it
ever established.

Two defects in the repair itself, both found by running it. The first anchor,
"Permission is hereby granted, free of charge", opens the MIT licence *and* the
OFL, so the bundled Noto asset kept the phrase alive through the mutation and
the test passed where it should have failed. The anchor is now derived from the
output: the first body line under the first licence header. The mutation also
lacked a `finally`, so a failed assertion left the generator gutted and
poisoned every later check in that fixture.

**`--artifact domowik-cli` never audits the spike.** The artifact list is
filtered before the licence loop, so the proposed stage claimed coverage it did
not have. The stage now makes a second, unfiltered call, which is the one that
reaches the issue-0024 spike workspace. It omits `--require-built` on purpose:
the Flutter bundle is `staged`, and requiring a built artifact would refuse
`incomplete-coverage` on something nobody asked this run to build. Measured
cost: 0.245s filtered, 0.30s unfiltered.

Both invocations are mutation-proven against a spike-only unreviewed
dependency. `ryu` 1.0.23 declares `Apache-2.0 OR BSL-1.0`, is on no allow-list,
and is in no closure here because `serde_json` moved to `zmij`. Added to the
spike workspace alone, it fails the unfiltered call and passes the filtered
one.

**A blind spot found while building that mutant, and not fixed here.**
`check_licences` skips any package whose `source` is null, so a PATH dependency
outside the workspace is treated as first-party and is never licence-audited.
Today every path dependency in this repository is first-party Apache-2.0, so
nothing is currently unaudited, and the first version of the mutant above
proved nothing precisely because it used one. A vendored copyleft crate added
as a path dependency would be invisible to the guard. Raised with root rather
than fixed mid-review, because changing what the guard admits is not a repair
to make while a review is open.

### Not done, deliberately

No stage was added to `scripts/verify.sh`. The four proposed stages need a
cost and duplication review from root first, and another worktree was running
verification. Nothing was published, tagged, uploaded or reserved; no namespace
was touched; no paid service was used. The `wgpu-native` MPL-2.0 trap that ADR
0003 names by name is recorded in
[docs/RELEASE-INVENTORY.md](../docs/RELEASE-INVENTORY.md) with attribution to
the issue-0025 worker, but ADR 0003 itself is not this issue's to amend.

Status stays **In Progress (partial scope)**: criterion 4 needs a release,
criterion 2 is staged for the GUI, and the ISC decision is open.
