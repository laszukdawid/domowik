# 0074: Use a directory as a plan-local furniture library

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0024, 0026

## Problem

The owner wants to point at a directory of their own models, browse its assets,
and use them in the current plan without bundling them with Domowik.

## Scope

Recursive directory browsing for GLB, OBJ and ZIP model bundles; lazy embedding
on placement; named 2D footprints with explicit user dimensions; selection,
movement, rotation, deletion and session undo/redo. No model decoding or 3D
rendering is required for this increment. ZIP payloads remain opaque user
content, not an importer for another application's catalogs or plan format.

## Acceptance criteria

- [x] A user can enter a directory, see supported models in its subdirectories,
      and place any of them on the active level.
- [x] Library directories belong to the plan, survive save/reopen, and do not
      leak into a new or different plan. Unused model bytes are not copied.
- [x] Placed assets are content-addressed, embedded and deduplicated; source
      deletion does not break saved placed instances.
- [x] Footprints have editable dimensions, position and rotation; individually
      selectable, movable and deletable instances participate in undo/redo.
- [x] Failed import/placement preserves the document; bounded reads and
      missing-directory diagnostics are tested.
- [x] Original behavior/provenance and additive compatibility are documented;
      relevant native and Flutter tests and canonical verification are run.

## Provenance and compatibility

Owner clarification: assets are owner-created user content. Implementation uses
Domowik's own entity, descriptor and container APIs; no external application's
source or catalogs are consulted. Library locations are optional manifest
extension metadata. Footprint sizes use an optional namespaced entity extension
preserved by existing readers; no required feature or container version changes.

## Closure — 2026-10-03

Implemented in the working tree; no commit requested. Native ABI 15, original
directory scanner/browser, lazy worker-isolate import, native indexed footprint
queries and editor rendering/edits. No external model bytes added to this repo.
See [usage and evidence](../../docs/evidence/0074-plan-local-assets.md).

Native import/history/persistence regressions pass. The complete Flutter suite
passes 265 tests. Owner-directory enumeration smoke test discovers all 66 model
files without reading/copying their content. Original 1,000-footprint benchmark
measured 2,068 ns/query on this Linux host, not a frame-time acceptance gate.

Canonical `./scripts/verify.sh` reached its last stage and printed
`=== VERIFICATION FAILED ===`, `STAGE: P13 clean tree`, exit 1. All preceding
executed checks passed; macOS bundle execution was explicitly skipped on Linux.
The worktree already contained uncommitted owner work at the start and now also
contains this implementation. No clean-tree/full-verification pass is claimed.
