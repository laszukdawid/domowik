# 0082: Preserve model dimensions and expose object resizing

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0076, 0078

## Acceptance criteria

- [x] Placement has no fixed-width/depth inputs. GLB dimensions use metres after
  scene-node transforms, converted to millimetres.
- [x] Models without defined units preserve decoded XYZ proportions, assigning
  1000 mm to the longest extent; unreadable/unsupported bounds fall back to a
  1000 mm cube with a visible diagnostic. Flat height uses a 1 mm minimum.
- [x] Right-click an object offers Select and Resize. Resize edits width, depth and
  height with proportions locked by default, optionally independently.
- [x] Resize is atomic, undoable, survives save/reopen, and updates 2D and 3D.
- [x] Existing plans retain their footprint and previous inferred height until
  explicitly resized; no user assets become tracked test fixtures.
- [x] Native, model/scene, and GUI regression tests and a sizing benchmark exist.

## Provenance

Original user direction on 2026-10-04. Public glTF units/transforms:
https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html.
Existing independently implemented Domowik decoders supply geometry. User
assets are read-only smoke inputs, never copied into tests or distributed.

## Closure — 2026-10-04

Implemented automatic sizing, explicit persisted height and right-click
Select/Resize with locked or independent XYZ dimensions. Inspector resizing is
also available. Original user asset bytes are untouched. Existing instance
sizes are not migrated; reinsert to derive source sizing or use Resize.

All 66 local owner assets measured successfully. Native asset tests, model/scene
regressions, Flutter analysis and all 279 Flutter tests pass. Linux release build
passes. Canonical verification passed all executed stages through tracker
consistency, then printed `=== VERIFICATION FAILED ===`, `STAGE: P13 clean tree`,
exit 1 because this workspace contains uncommitted changes. macOS application
checks were explicitly skipped on Linux. No macOS/Android execution is claimed.

See [sizing, compatibility and benchmark evidence](../../docs/evidence/0082-model-size-and-resize.md).
