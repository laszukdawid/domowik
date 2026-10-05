# Plan-local directory assets — issue 0074

## Use

In the editor, click **Assets**, then **Add directory** to open the desktop
system folder chooser. **Enter path** remains available for manual entry, and
each attached path has edit/browse controls (issue 0075). GLB, OBJ and ZIP model files in its subdirectories appear in the
searchable list. Choose a model, enter its footprint width/depth in mm, click
**Place in plan**, then click the canvas to position its center.

Drag the footprint to move it. Its inspector exposes **Edit model footprint**
(X/Y, width/depth and counter-clockwise rotation) and **Delete model**. These
edits support Undo/Redo. Escape cancels armed placement.

Directories are remembered by the current `.domo` plan. Browsing copies no
bytes. Each placed model embeds its exact bytes by SHA-256 identity, preserving
the model bundle intact. Reopening offers embedded models even if the source
directory is unavailable. New plans start with an empty library.

This increment renders named user-sized 2D rectangles. It does not decode model
geometry, infer model dimensions, resolve loose OBJ material/texture references,
or add 3D rendering. ZIP bundles are opaque payloads, not parsed catalogs.
The CLI carries their entities and bytes; its SVG/PNG exporters do not yet draw
these editor footprints.

**Superseded display/persistence scope (issues 0076/0077):** the editor now draws
actual top-down mesh previews instead of those rectangles where decoding
succeeds, and newly added library paths are user-level preferences available
across plans/restarts. Placed bytes remain portable plan content. The paragraph
above records the original issue-0074 handoff.

## Provenance and compatibility

Original owner-requested workflow for owner-created user content. Implementation
uses only Domowik APIs, Dart filesystem enumeration and the standard library.
No external application source or catalog format consulted; no third-party
assets or dependencies added. Tests generate original inert model content.

Optional manifest and entity metadata extensions are described in FORMAT-SPEC
§2.3 and BEHAVIOR-SPEC §10.1. Existing readers preserve both without having to
interpret them. Native ABI 15 adds `domowik_plan_assets` with existing
request/response ownership, locks and close/drain semantics. Directory scans
are asynchronous and bounded to 10,000 entries/64 levels without symlink
following. Model reads run on a Dart worker isolate, capped at the container's
256 MiB per-entry ceiling, including a size check during reading. ZIP contents
are never expanded.

Footprint click queries use a revision/level-keyed spatial index and binary64
rotation/nearest-rectangle calculations, without a topology-tolerance promise.
No timing is a pass/fail gate. An original 1,000-rotated-footprint benchmark
includes native JSON serialization and response-buffer ownership. On the local
Linux host, `cargo bench --manifest-path
spikes/0024-flutter-rust-bridge/Cargo.toml --bench asset_footprints` measured
**2,068 ns/query** over 100,000 warm queries. This is host-specific, not an
editor frame-time or rendering measurement.

## Verification

Native regression tests cover bounded reads; rejected imports preserving
history/content; deduplication; rotated hit geometry; edit/delete undo/redo;
save/reopen after source removal; embedded model reuse; and new-plan isolation.
Flutter tests cover recursive discovery, mixed-case extensions, names with
spaces, ignored symlinks, inaccessible-directory diagnostics and the real
native-backed editor placement/edit/move/delete/save/reopen workflow.

### Recorded results — 2026-10-03, Linux

- Native bridge regression suite, clippy and release build passed.
- `flutter analyze` passed; complete `flutter test` suite: **265 passed**.
- A temporary enumeration-only test against the owner-specified directory
  found **66 models**, with no diagnostics. No user model content was copied
  into the repository or used as a committed fixture.
- `./scripts/verify.sh` passed every executed stage preceding its final
  clean-tree assertion, including Rust/Go, inventories, native C/Dart probes,
  Flutter analysis/tests/CPU harnesses and tracker consistency. The macOS
  application stage was explicitly skipped on Linux.
- The final banner was **`=== VERIFICATION FAILED ===`**,
  **`STAGE: P13 clean tree`**, exit **1**: both pre-existing owner work and the
  new implementation are uncommitted. This is not a full verification pass.
