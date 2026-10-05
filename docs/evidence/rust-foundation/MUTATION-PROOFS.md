# Rust foundation — mutation proofs

**Artifact under test:** `scripts/verify.sh` and the Rust workspace, issue 0037
**Date:** 2026-08-30
**Standard applied:** ED-0041 (a check that cannot fail), ED-0053, ED-0087 R3,
ED-0089 (a tautological test named for a spec row).

Every gate below was mutated, the full `scripts/verify.sh` run, the exit status
captured with `$?` directly (never through a pipe — ED-0077), and the mutation
reverted. The tree was confirmed clean after each.

## Verification stages

| # | Mutation | Captured exit | Stage named |
|---|----------|---------------|-------------|
| 1 | appended `pub fn  badly_formatted( ) ->u8{ 1 }` to `domowik-format` | 1 | `cargo fmt` |
| 2 | set `L6_ENTRY_COUNT` back to `65_536` | **101** | `cargo test` |
| 3 | added an unrecorded runtime dependency to `domowik-geometry` | **101** | `cargo build` |
| 4 | removed the `serde` row from `docs/RUST-DEPENDENCIES.md` | 1 | `rust dependency inventory` |
| 5 | removed `MIT` from the generator's reviewed allow-list | 1 | generator (stderr names each crate) |

Two results are worth more than a tick in a table.

**Proof 2 and 3 captured exit 101, not 1.** That is cargo's own status, carried
through. The superseded `run_stage` wrapper in the abandoned branch ended every
failure with a literal `exit 1`, which would have destroyed exactly this
information. The distinction is not cosmetic: the withdrawn CLI probes assert
**exit code 2** semantics specifically, and a harness that flattens every
failure to 1 cannot tell a contract violation from a crash.

**Proof 3 failed EARLIER than designed.** The intent was to prove the licence
inventory catches an unrecorded dependency; `cargo build --locked` rejected it
first, because adding a dependency changes `Cargo.lock`. The guard is therefore
stricter than the one deliberately built — worth recording, because it means
proof 4 was needed to exercise the inventory stage on its own.

## What proof 4 does and does not establish

Proof 4 diverged the RECORDED inventory from reality (deleting a row) rather
than adding a real dependency, because `--locked` makes the latter unreachable
at that stage. It establishes that the regenerate-and-diff mechanism fires and
names its stage. It does not, on its own, establish the end-to-end
"developer adds a crate" path; proof 3 covers that, at a different gate.

## Scope limits of the licence guard

`scripts/rust-deps-inventory.py` reports what crates **declare** in their
manifests. It is not a licence audit: it does not read LICENSE files, cannot
detect a crate whose declared expression is wrong, and does not build `NOTICE`.
Those obligations are owed by the Rust release process that replaces the frozen
`cmd/domowik-sbom`, and are recorded in
`docs/decisions/0008-rust-core-implementation.md`.

Current surface: 47 third-party crates — 11 runtime, 36 dev — all permissive
(MIT, Apache-2.0, BSD-2-Clause, Unlicense, Unicode-3.0). No copyleft.

## The L6 conformance test

`l6_boundary_admits_the_ceiling_and_rejects_one_past_it` pins both sides of
FORMAT-SPEC §9.3 row L6 — 65,535 accepted, 65,536 rejected. Proof 2 shows it
fails when the original defect is reintroduced, with a diagnostic naming the
spec row and the reason (the 16-bit ZIP end-of-central-directory field).

This replaced a test named `limits_default_matches_l6` that asserted
`65536 == 65536` against a hardcoded default — a test that certified the defect
while carrying the spec row's name (ED-0089).

## Known gap, declared rather than closed

The `P13 clean tree` stage remains unreachable by pre-run mutation: any dirt
introduced before the run is caught by earlier stages. It guards dirt created
BY the run — stray build output, a tool writing outside `build/ci` — which
would require injecting a fault mid-script. Recorded, not worked around.
