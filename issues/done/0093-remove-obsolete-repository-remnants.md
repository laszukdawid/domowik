# 0093: Remove obsolete repository remnants

- **Phase:** 0
- **Priority:** Medium
- **Status:** Done
- **Depends on:** 0062, 0092

## Scope

Remove the obsolete Go tracker handoff and unimplemented wgpu placeholder
workspace. Remove local `backend/` remnants after confirming that they contain
only bytecode, test caches and virtual environments, not application source or
user data. Preserve current frontend/core behavior, imported assets, credentials,
historical evidence and the proposed portable-renderer requirements.

## Acceptance criteria

- [x] `build/HANDOFF.md` and the remaining placeholder `spikes/` tree are absent.
- [x] Local `backend/` remnants are removed recoverably after inventory review.
- [x] Workspace configuration, fixture tooling and current documentation no
      longer depend on the removed files/directories.
- [x] Issue 0025 retains its unfulfilled requirements without claiming a renderer
      implementation or cancelling the proposed platform gate.
- [x] Full mandatory-Linux verification and release-inventory regression checks
      pass locally in a clean source snapshot; tracker and whitespace checks pass.

## Closure — 2026-10-05

Removed the obsolete Go tracker handoff from `build/`; its durable failure
evidence and active Rust checker remain. Removed the unused surface-spike
manifest, lockfile and placeholder, plus its generated compiler cache from the
root layout. Removed the local-only `backend/` directory after inventory found
only Python bytecode, pytest caches and two virtual environments, with no
application source, plans or databases. Existing generated `build/`/`target/`
outputs, imported assets, credentials and agent configuration are untouched.

Removed the dead Cargo exclusion and the release fixture's forced-staging
exception for the handoff. Updated current guides and ADR location notes;
historical evidence remains unchanged. Issue 0025 returns to Open with all
acceptance criteria intact: deleting scaffolding does not implement or cancel
the proposed renderer.

Recovery: tracked removals remain in Git history. The full backend directory
was renamed on its original filesystem into ignored
`build/cleanup-recovery-AqiuZN/backend` because some nobody-owned bytecode could
not be read for a cross-filesystem copy. The former spike directory is backed up
at `/tmp/domowik-cleanup-recovery.AqiuZN/spikes`. A partial readable backend copy
also remains in that temporary recovery directory. These are local recovery
copies, not tracked application or release inputs.

Local validation: full `DOMOWIK_REQUIRE_LINUX_EDITOR=1 ./scripts/verify.sh`
exits 0 with `=== VERIFICATION PASSED === all stages` in a clean source
snapshot, including 305 widget tests, the three frontend CPU harnesses, Linux
release packaging and both runtime probes. macOS runtime stages are explicitly
skipped on Linux. Release-inventory regression passes all 28 checks without the
forced-staging exception. Shell declaration checks (eight files), changed-guide
links, tracker consistency and whitespace checks pass. The cleanup was initially
left uncommitted; the owner subsequently authorized its local commit. No push is
authorized by that follow-up.
