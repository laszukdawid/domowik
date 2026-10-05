# 0037: Rust core foundation — supersede ADR 0002, scaffold, and verification contract

- **Phase:** 1
- **Priority:** High
- **Status:** Done
- **Depends on:** none (foundation)
- **Blocks:** every Rust port issue

Activated 2026-08-31 under ED-0086 and its amendment (a): the project owner
decided the core moves to Rust, full-core scope. This issue is the foundation
that every port depends on. It runs SINGLE-THREADED per ED-0085 because nothing
can be ported until it exists.

## Problem

The Go core is being replaced. Before any behaviour is ported, the project needs
a governing decision record, a Rust workspace, a third-party posture, and a
verification contract — because `scripts/verify.sh` and the three `cmd/*` tools
are Go-specific and currently constitute the only definition of "correct".

## Scope

1. **Supersede ADR 0002.** A new ADR records the Rust core decision, the
   evidence it was taken against (ED-0073 and its amendments), its own
   validation gates, and its failure triggers. ADR 0002 is marked superseded,
   not deleted; its reasoning stays readable.
2. **Re-assess ADR 0003.** Its Flutter-bridge argument assumed a Go core. State
   what changes and what still holds; do not silently inherit it.
3. **Cargo workspace scaffold.** Crate layout mirroring the ported boundaries
   (format types, container, document, geometry, CLI), edition and toolchain
   pinned, reproducible builds.
4. **Third-party posture.** `docs/THIRD-PARTY-POLICY.md` applies to every
   crate. Rust's geometry ecosystem is the stated advantage of the move and is
   also its largest licensing surface. No crate is adopted without licence,
   provenance and notice review recorded. The clean-room policy is unchanged.
5. **Verification contract.** Rebuild the equivalent of `verify.sh`: build,
   test, format, lint, dependency audit, SBOM, reproducibility, clean tree.
   Decide per Go tool — `domowik-sbom`, `domowik-jsoncheck`,
   `domowik-tracker-check` — whether to reimplement or retain as build-time
   utilities, and record the decision.

## What this issue does NOT do

No behaviour is ported. No format semantics are re-decided. The specification,
the corpus and every ruling stand as written; the Rust implementation is built
against them.

## Acceptance criteria

Checked against the tree on 2026-09-12; see `Checklist reconciled — 2026-09-12`
below for why they were unchecked while the issue read `Done`.

- [x] ADR 0002 superseded by a new ADR with evidence, validation gates, and
      failure triggers stated. `docs/decisions/0002-go-core-cli-first.md` line
      3 reads `Status: Superseded by ADR 0008`; ADR 0008 is
      `0008-rust-core-implementation.md`.
- [x] ADR 0003 re-assessed; what changed and what holds recorded.
      `docs/decisions/0003-flutter-gui-wgpu-native.md` carries
      `## Re-assessment after ADR 0008 — 2026-09-01` and **stays Proposed** —
      a language change is not spike evidence.
- [x] Cargo workspace builds, with edition and toolchain pinned.
      `rust-toolchain.toml` pins 1.98.0 with `rustfmt` and `clippy`; the
      workspace is eight crates.
- [x] Every adopted crate has licence, provenance and notice recorded per
      `docs/THIRD-PARTY-POLICY.md`; the dependency set is justified, not
      inherited. `docs/RUST-DEPENDENCIES.md` is regenerated from
      `cargo metadata` and diffed, so adding a dependency fails the build
      until its licence is reviewed. `NOTICE` is generated across 83
      distributed components (ED-0094), and `tiny-skia`'s five licences were
      reviewed on adoption (ED-0093).
- [x] A verification entry point exists, is documented, exits non-zero on any
      failure, and NAMES the failing stage. `scripts/verify.sh` traps at line
      24 and prints `=== VERIFICATION FAILED ===` with STAGE, line and exit
      status; a pass prints `=== VERIFICATION PASSED === all stages`. This is
      the ED-0077 repair, and it is why a run without a banner is not a result.
- [x] Disposition recorded for each Go tool: reimplemented, retained, or
      dropped with reason. ED-0087 rules all three. `domowik-jsoncheck` and
      `domowik-tracker-check` were REIMPLEMENTED in `domowik-devtools`
      behaviour-for-behaviour including exit codes; `domowik-sbom` was dropped
      with the Go product and the replacement obligation sits on issue 0036.
- [x] The Go tree still builds and verifies until the port replaces it, or its
      retirement is explicitly staged and recorded. Both arms hold:
      `internal/corpusgen` still builds, tests, and passes `gofmt`, `go vet`
      and the corpus gates, and retirement is staged as issue 0062, whose open
      question is whether to port or freeze the hand-rolled ZIP writer that
      emits deliberately malformed archives.

## Carried forward — four conformance defects the corpus already has

Found by the last Go audit (ED-0086 amendment (a)). Three are CORPUS defects and
would defeat a Rust implementation identically:

- **L12** — fixture holds exactly 64 characters; guard tests `> 64`. On the
  boundary, not over it.
- **L19** — `uuid-*`/`digest-*` fixtures test field FORMAT, never OVERLENGTH.
- **L4** — no fixture exercises the guard.
- **L10** — container nesting depth has NO GUARD; `CatLimitNestingDepth` serves
  only JSON nesting (L11).

Do not fix them here. Record them where the port will meet them.

## Progress — 2026-08-30

Delivered:

- Go core frozen at `_reference/go/` with history preserved (164 renames). The
  underscore prefix removes it from the Go toolchain's default patterns.
- Cargo workspace, five crates, toolchain pinned, performance profile
  (`lto = "fat"`, `codegen-units = 1`, `panic = "unwind"` retained for FFI).
- **L6 conformance defect fixed and mutation-proven** — 65,536 → 65,535, with a
  test that pins both sides of the boundary and cites §9.3.
- Four remaining crates declared as scaffolds in their own module docs rather
  than carrying tautological tests labelled "substantive" (ED-0089).
- `verify.sh`: Rust stages, benchmarks compiled every run, fail-closed
  third-party licence guard. Five gates mutation-proven
  (`docs/evidence/rust-foundation/MUTATION-PROOFS.md`).
- CI builds Rust and cross-compiles for x86_64 and aarch64.
- ADR 0002 marked Superseded; ADR 0008 written and its own earlier
  "gate 1 satisfied" claim corrected.

Still open, and why this issue is not Done:

1. **Gate 1 is not closed.** Four crates are scaffolds. Gate 1 closes when each
   crate has behaviour worth testing and a test that fails without it.
2. **A Rust release/SBOM process is owed.** The Go one froze with its product.
   The interim guard reports declared licences only — no LICENSE reading, no
   `NOTICE` generation.
3. **CLI probes P1-P3 and artifact inspection are withdrawn** and must be
   reinstated against the Rust binary as part of the CLI port.
4. **ADR 0003 re-assessment** — its Flutter-bridge reasoning assumed a Go core.
5. **Toolchain age.** Pinned at 1.82.0 (Aug 2024). For a rewrite justified on
   performance, a current compiler is worth having; installing one is an owner
   decision because it changes the machine.

## Status set by ED-0095 — 2026-09-01

**Done.** The Rust workspace, its 35 verification stages and the ADR 0002 supersession are all in force. ADR 0003 is re-assessed against ADR 0008 (2026-09-01): the bridge is now a Rust FFI, panics must be caught at the boundary and ownership transported explicitly, while Flutter, wgpu-native and every validation gate hold unchanged — the ADR stays Proposed, because a language change is not spike evidence.

## Checklist reconciled — 2026-09-12

**Location.** The status line has read `Done` since ED-0095 (2026-09-01) while
the file stayed in `issues/` and `issues/README.md` linked it there. Moved to
`issues/done/` with the index updated in the same commit.
`domowik-tracker-check` now fails a terminal status whose file is in the wrong
directory, and an index row whose link does not name the file's actual
location; neither check existed when this drifted.

**The boxes.** All seven stayed `- [ ]` through the ED-0095 closure and the
ED-0096 audit, which wrote their verdicts in prose and left the checklist
alone. All seven are now ticked against a named file or decision. Unlike issue
0015, nothing here turned out to be unmet — the `Still open, and why this issue
is not Done` list above was superseded item by item: gate 1 by the four crates
acquiring real behaviour, ADR 0003 by its 2026-09-01 re-assessment, the toolchain
by the pin moving to 1.98.0. The two that were NOT closed here were routed
rather than dropped — the Rust release/SBOM process to issue 0036, and CLI
probes P1-P3 to the CLI port — and both are still owed against those issues.

**The four carried-forward corpus defects (L12, L19, L4, L10) are untouched by
this reconciliation.** They were recorded here to be met by the port, and
issue 0028 owns them. L10 in particular acquired a second site on 2026-09-12:
issue 0015's `apply` path bounds nesting only at `serde_json`'s 128-level
default.
