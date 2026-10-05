# Model dimensions and resizing — issue 0082

## Behavior and compatibility

The asset library no longer asks for fixed insertion width/depth. GLB scene
geometry is decoded with static node transforms and its metre extents converted
to millimetres. OBJ/ZIP-OBJ preserves XYZ proportions at a 1000 mm longest axis;
unsupported bounds produce a labelled 1000 mm cube fallback. Flat height has a
1 mm stored minimum; this does not extrude a flat source mesh. Known dimensions
outside the editor's existing upper bound reject placement instead of silently
changing physical scale. Embedded reuse measures the original model again.

Right-click offers Select and Resize on the foremost active object. Resize
defaults to locked proportions and accepts independent width/depth/height when
unlocked. The inspector also provides Resize and height/property editing.
Apply is one native setFields batch; Cancel writes nothing. Rotation, center,
level, source bytes and unrelated metadata remain owned by their existing
contracts. Wall-chain right-click keeps its finish behavior.

Width/depth uses existing `editorFootprintMm`; new optional `editorHeightMm`
stores positive unscaled height. Scene generation applies explicit height to Y
using the cached mesh's original height/width ratio, including very thin meshes.
Legacy instances without height keep their previous width-derived height until
edited, and the resize dialog resolves it off the UI thread. Older readers
preserve the extension but may display independently resized height incorrectly.
No C ABI signature, required format feature, dependency or container version
changes. The JSON bridge adds `size` and optional height; legacy explicit
footprint callers remain accepted. GUI placement always requests automatic size.

## Provenance

Original owner request on 2026-10-04, specified in BEHAVIOR-SPEC §10.4 and
FORMAT-SPEC's optional editor extensions. Physical unit and node-transform
rules come from Khronos glTF 2.0:
https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html.
The consulted specification describes a public format; no specification code or
third-party implementation is incorporated. Existing independent Domowik
GLB/OBJ decoders and their earlier provenance record (issue 0076) are reused.
Bounding-extents accumulation and sizing are original implementation. Tests
construct original triangle/OBJ/ZIP content, never copies of user assets.

## Validation

- Model tests cover transformed GLB dimensions, unitless XYZ proportions, ZIP,
  flat height and unsupported/degenerate geometry.
- Native tests cover physical/proportional/fallback import, rejected oversized
  physical geometry, invalid resize atomicity, Undo/Redo, save/reopen and
  embedded-model reuse. Existing placement/ownership tests remain passing.
- Scene tests cover independent height, uniform instance scaling, legacy size,
  missing-model fallback and thin mesh ratios below 0.001.
- Native-backed editor tests cover absence of insertion size fields, actual
  right-click Select/Resize, locked sizing, Cancel, independent three-axis
  resize, Undo/Redo and saved dimensions. Widget tests also reject zero,
  negative, non-finite and oversized values and re-lock updated proportions.
- Read-only local smoke: all 66 owner-provided files in `assets/` measured;
  32 physical-unit GLBs, 34 unitless models, zero failures. No content or art
  from that directory is added to test fixtures. UPPLAND's decoded size is
  approximately 2239.41 × 920.07 × 920.07 mm.
- Original quad bounded decode/sizing benchmark: 791.34–839.23 ns locally
  (10 samples, one-second warmup/measurement). This is a synthetic cold sizing
  cost, not a whole-model, UI-frame or reference-device performance claim.

Canonical verification and the Linux release build are recorded at closure in
the issue tracker.
