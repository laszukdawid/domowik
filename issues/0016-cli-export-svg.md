# 0016: Implement CLI SVG export

- **Phase:** 1
- **Priority:** High
- **Status:** In Progress (partial scope)
- **Depends on:** 0013, 0026

## Current disposition — 2026-10-05

Keep partial for shared semantic paint and visual conformance.

SVG export and shared room-label generation exist in
`crates/domowik-cli/src/{svg,label}.rs`. Extract the remaining
occupied-region/aperture/background paint inputs into a renderer-neutral API,
then add tolerant rasterized comparisons and omission coverage. Do not remove
the working exporter.

This disposition supersedes older progress/status claims below; original
criteria and dated evidence are retained, not waived.

## Original problem

Export the v0.1 renderable model through a semantic core paint model. Define
supported entities and deliberate omissions instead of implying all entities
render automatically.

## Acceptance criteria

- [ ] SVG profile, `viewBox`, units, Y transform, clipping, draw order, stable
      IDs, escaping, and deterministic numeric/element ordering are specified.
- [ ] Renderer-neutral paint primitives/styles, stable resource references,
      clipping, text intent, and error behavior have a core API with semantic
      conformance tests independent of SVG serialization.
- [ ] Level output semantics (one file, files, groups, or pages) are explicit.
- [ ] Text/font and embedded/external image policy is defined.
- [ ] Unsupported/missing assets produce documented placeholders or warnings.
- [ ] Tests use structural validation and tolerant rasterized visual comparison,
      not an unspecified byte hash.

## Progress — 2026-08-30 (Rust core)

Partially delivered by the Rust rewrite. See
`docs/decisions/0008-rust-core-implementation.md`.

Delivered: `export-svg` renders a level's walls as filled polygons with
openings cut out of them; doors draw a sill line, windows a band. Output is a
standalone SVG that opens in a browser, and is byte-deterministic for a given
document.

Not delivered: the semantic paint model the architecture describes, room fills,
dimensions, labels, and scale. PNG export (0017) is untouched.

## Status set by ED-0095 — 2026-09-01

**In Progress (partial scope).** SVG export renders the occupied region, rooms, apertures and background images deterministically, with structural tests over each, escaping, level scoping and the missing-blob case. NOT delivered, and the reason this is not Done: there is no renderer-neutral paint-primitive API — `svg.rs` and `png.rs` each render straight from the `Document`, so the second criterion's "semantic conformance tests independent of SVG serialization" have nothing to test against, and the two exporters can drift. The sixth criterion's tolerant rasterized visual comparison is also absent; the tests are structural only.

## Progress — 2026-09-12 (room labels, issue 0060)

Room names now render in SVG and PNG through ONE module,
`crates/domowik-cli/src/label.rs`, which turns a document and its derived faces
into placed glyph contours in plan millimetres. Each exporter draws those
contours with the path primitive it already had, so this is not the third
renderer this issue's scope note forbids — but it is not the renderer-neutral
paint API either.

What that means for this issue's second criterion, stated plainly: labels are
the first thing both exporters take from a shared semantic source, and
`crates/domowik-cli/tests/room_labels.rs` asserts the two agree on what a label
says, where it sits and which names they refuse. Everything else — the occupied
region, apertures, background images — still goes from the `Document` into each
exporter separately. The criterion stays UNMET.

The fifth criterion moves: an unsupported room name now produces a documented
warning rather than silence (`label::LabelWarning`), reported on the success
message of both `export-svg` and `export-png`. The sixth, tolerant rasterized
visual comparison, is still absent; the label tests sample raster luminance at
known plan points, which catches "the label is missing" and not "the label
moved slightly".

Text and font policy for both exports is now defined and recorded: Noto Sans
Regular 2.015 pinned by hash under OFL-1.1 (`docs/third-party/assets.json`,
`NOTICE`), outlines rather than `<text>` so the two exports agree by
construction, and an explicit repertoire outside which names are refused rather
than drawn incorrectly. Issue 0017 carries the shaping gap that refusal leaves
open.
