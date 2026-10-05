# 0076: Render imported furniture instead of footprint placeholders

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0074

## Problem

Placed assets currently show identical labelled rectangles. The owner expects
the actual imported objects to appear in the plan, including desk2 and sandpit.

## Acceptance criteria

- [x] OBJ/ZIP-OBJ and GLB meshes, including Draco-compressed GLBs, produce
      top-down previews of their geometry with diffuse material colors.
- [x] Previews respect placement dimensions, rotation, movement and level
      visibility and regenerate from embedded bytes after reopen.
- [x] Decoding happens off the UI thread and previews are reused across views
      and edits; failures retain an explicitly diagnosed fallback.
- [x] Furniture-only plans do not display the empty-plan wall-drawing message.
- [x] Input/resource limits, original conformance fixtures, rendered-output
      regressions, benchmarks, provenance/dependencies and verification recorded.

## Provenance

Standard OBJ/MTL and glTF formats only; no application catalog/plan importer.
References: Khronos glTF 2.0.1 specification (2021-10-11),
https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html; public OBJ format
documentation hosted by Paul Bourke, https://paulbourke.net/dataformats/obj/.
No reference source or example fixtures are copied. Tests generate original
meshes. Owner-selected assets are local smoke-test inputs only, not bundled.

## Closure — 2026-10-03

Implemented in working tree, no commit requested. `domowik-models` supplies
bounded mesh decoding and derived PNG previews; native decoding releases the
plan lock, and Dart worker-isolate generation feeds a digest-keyed image cache.
All 66 owner-selected files rendered, including desk2/sandpit and 31 compressed
GLBs. Original contract and actual canvas-pixel tests pass; decoded gaps remain
transparent and rotation reuses the image. Embedded previews rebuild after
source removal/save/reopen. New dependencies and notices are recorded.

See [evidence](../../docs/evidence/0076-model-previews.md). Complete Flutter
suite: 271 passed; Rust/native checks and Linux release build passed. Canonical
verification printed `=== VERIFICATION FAILED ===`, `STAGE: P13 clean tree`,
exit 1 after all prior executed stages passed, because work remains uncommitted.
macOS/Android bundle execution was not performed on this Linux host.
