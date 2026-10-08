# 0078: Add a live 3D plan view with orbit and walk-through navigation

- **Phase:** 3
- **Priority:** High
- **Status:** In Progress (partial scope)
- **Depends on:** 0076, 0024
- **Deferred-closure gates:** 0025, 0033

## Current disposition — 2026-10-05

Keep partial: Linux live 3D is delivered; portable/hardware/performance gates
remain.

`domowik-scene`, `domowik-models` and the frontend Linux GL texture runner
implement the usable view. Subsequent full Linux verification passed
(0092/0093), superseding the old dirty-tree-only failure as the latest
evidence. Retain 0025/0033 gates and the documented non-PBR/LOD/platform
limitations; do not recreate a renderer in spikes/.

This disposition supersedes older progress/status claims below; original
criteria and dated evidence are retained, not waived.

## Remaining acceptance work — 2026-10-08

The original checked criteria below describe delivered scope and dated
evidence. These later residuals explain the current nonterminal status.

- [ ] Close or explicitly re-scope 0025 portable-backend/platform gates and
      obtain 0033 named-device whole-app frame/input/resource evidence.

## Original problem

The owner wants an actual navigable 3D view comparable in use to public
floor-plan editors, not top-down furniture previews. The current host/editor is
Linux; platform/performance gates remain independently tracked.

## Acceptance criteria

- [x] A live perspective 3D view shares the Rust plan and reflects edits,
      undo/redo, save/open and visible-level changes without a second document.
- [x] Walls, openings, floor slabs/rooms and imported furniture render at their
      real placement/elevation and dimensions, with depth testing and lighting.
- [x] Orbit, pan, zoom, fit and eye-level walk-through controls are available;
      camera/view changes create no document operations.
- [x] GPU rendering appears in a Flutter texture without per-frame CPU image
      readback; resize, teardown and surface failures are handled explicitly.
- [x] Navigation sends only camera/viewport state: model decode, triangulation,
      scene serialization and geometry uploads are absent from the camera path.
      Repeated instances share meshes; bounded rendering LOD is permitted under
      the owner's performance-first direction.
- [x] Scene/interaction tests, real native-surface evidence, resource budgets,
      benchmark and dependency/provenance/architecture records are present.

## Provenance

Original user-requested 3D workflow. No external application's source, private
schemas, catalog or implementation consulted. Standard Flutter texture/GTK GPU
APIs are public SDK interfaces. ADR 0003 remains Proposed; the interface and
backend experiment are recorded rather than silently declaring its gate closed.

## Delivered handoff — 2026-10-03

Linux editor now has 2D / 3D / split modes, a registered GPU texture, Rust-owned
scene derivation, instanced cached models, 5,000-triangle fast LOD, room slabs,
wall/opening volumes, level elevations/ghosting and orbit/walk controls. The
camera handler has an independent short lock and never waits on GPU drawing.
Actual texture/probe and original editor/scene/LOD tests passed; all 275 Flutter
tests, Rust/native checks and the Linux release build passed. Canonical
verification reached `P13 clean tree` and printed `=== VERIFICATION FAILED ===`
with exit 1 solely because the worktree is uncommitted.

**Status remains partial scope** for the recorded cross-platform and hardware
gates (0025/0033), not because a screenshot discharged them. Native 3D surface
support is currently Linux; other platforms report an unavailable surface.
Rendering is deliberately diffuse/flat and uses approximate transparency/LOD,
not complete textured/PBR or Sweet Home 3D feature parity. Original source
assets and exact 2D geometry are unaffected. No commits requested.

See [the evidence and commands](../docs/evidence/0078-live-3d-view.md).
