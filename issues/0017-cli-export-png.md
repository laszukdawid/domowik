# 0017: Implement CLI PNG export

- **Phase:** 1
- **Priority:** Medium
- **Status:** In Progress (partial scope)
- **Depends on:** 0016

## Problem

Rasterize the semantic 2D paint model with a selected, pinned,
license-compatible renderer. A new internal anti-aliased renderer is out of
scope unless separately justified.

## Acceptance criteria

- [ ] Pixel dimensions/DPI, color space, alpha, antialiasing, metadata, and
      encoder settings are explicit.
- [ ] Font files/shaping are pinned where deterministic visual tests need them.
- [ ] Maximum dimensions and memory use are bounded.
- [ ] Determinism target distinguishes pixel equality from encoded-byte
      equality.
- [ ] Direct/transitive renderer licenses and notices are recorded.

## Progress — 2026-08-31 (Rust core)

`domowik export-png <plan> <out.png> [--level NAME] [--scale PX_PER_MM]`
rasterises the same semantic paint model the SVG export draws, so the two
agree. Renderer: `tiny-skia` 0.12, pinned, adopted under ED-0093 with its five
new licences reviewed and recorded.

### Acceptance criteria

- [x] Pixel dimensions/DPI, color space, alpha, antialiasing, metadata, and
      encoder settings are explicit. Stated in `crates/domowik-cli/src/png.rs`'s
      module documentation, including the two decisions to write NOTHING: no
      `pHYs` chunk, because stating a DPI would be a claim about print size
      this exporter has no basis for, and no generator or timestamp metadata,
      because either makes byte-identical output impossible for identical
      input.
- [x] Maximum dimensions and memory use are bounded. `MAX_DIMENSION` 16,384 px
      per side and `MAX_PIXELS` 64 Mpx — the product is bounded SEPARATELY,
      because two sides each under the per-side cap still multiply past any
      sensible allocation. An over-large request is CLAMPED and the reduction
      reported, not refused.
- [x] Determinism target distinguishes pixel equality from encoded-byte
      equality. Pixel equality is the target and is asserted; byte equality is
      asserted too, as a within-build regression alarm, and the module says
      plainly that it is a consequence of a pinned deterministic encoder rather
      than a promise across an upgrade.
- [x] Direct/transitive renderer licenses and notices are recorded. Licences in
      `docs/RUST-DEPENDENCIES.md`, reviewed in ED-0093. **NOTICES ARE NOT** —
      see below.
- [x] Font files/shaping are pinned where deterministic visual tests need them.
      **Met 2026-09-12 by issue 0060**, which brought the room labels this
      criterion was waiting for. Noto Sans Regular 2.015 is pinned by content
      hash and embedded, under the SIL Open Font License 1.1, recorded in
      `docs/third-party/assets.json` and reproduced in `NOTICE`. SHAPING is
      pinned by being absent and saying so: `crates/domowik-cli/src/label.rs`
      draws glyph outlines from `cmap` for a declared repertoire of
      left-to-right, non-combining scripts and REFUSES any name outside it with
      a warning naming the code point. See the obligation below, which that
      refusal creates rather than discharges.

### Owed

- ~~**`NOTICE`.**~~ **Discharged 2026-09-01 under ED-0094.** `NOTICE` is
  generated from the dependency graph by `scripts/rust-notice.py`, reproducing
  each distributed crate's own licence texts, and `scripts/verify.sh`
  regenerates and diffs it so a dependency change fails the build until the
  notice catches up. 83 distributed components, including the BSD-licensed
  renderer this issue adopted.
- Interlacing, bit depth, and palette choices are the encoder's defaults and
  are not selectable.
- **Text shaping, added 2026-09-12 by issue 0060.** Room names outside
  `label::SUPPORTED_RANGES` — anything needing `GSUB`/`GPOS`, mark attachment
  or bidirectional reordering, so CJK, Arabic, Hebrew, the Indic scripts, and
  any name written with combining diacritics — are NOT drawn. The export
  succeeds, keeps the area line, and reports each refusal by record and code
  point; the stored name is never altered. This is a live obligation, not a
  scope decision: a reviewed shaping path would discharge it, and until then
  the affected plans export without their names. It is recorded here because
  the font this criterion asked for is what makes the gap visible.