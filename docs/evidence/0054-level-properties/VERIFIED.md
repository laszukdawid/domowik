# Level properties and advisory validation — issue 0054

Status: **Verified**, 2026-09-05. Implementation/spec commit `125734d`.

## Delivered boundary

The shared Rust edit builder produces one `setFields` operation containing
only changed level fields. It validates supplied numeric values without a
document clone, preserves unknown fields/background references and does not
rewrite existing walls or openings. Unknown IDs and invalid values reject
before admission. Unchanged values (including signed zero) produce no command.
The session remains the final admission/history authority.

The CLI exposes `set-level --rename --elevation --floor-thickness
--wall-height --ceiling-height`; ambiguous names refuse rather than choosing
the first match. CLI display order and the unnamed default target derive from
elevation/UUID, including after an edit; this corrects older insertion-order
behavior without reordering stored entities. GUI controls submit `setLevel`
through the existing bridge.
New interactive/CLI walls with no height override resolve the target level's
current default once, then store the explicit height. Existing explicit-height
callers and previously saved walls retain their values.

Validation warnings are distinct from errors: ceiling below default wall
height, duplicate elevation, and upper-slab intrusion below the preceding
finished-floor plane. They never invalidate a plan or prevent saving. Human
and JSON CLI validation expose them; JSON and bridge state include codes,
level UUIDs and messages. The bridge caches warnings by accepted revision,
including across saves of the same session. Execute/history response objects
and ABI 9 are unchanged. Empty successful `setLevel` output means no batch.

## Specification and review

The old slab formula used the lower slab's thickness upwards, contradicting
the normative downward convention. Section 2.1 now defines strict upper-slab
intrusion below a plane, not positive slab-volume intersection. Zero thickness,
touching, equal elevations, warning order and invalid operands are explicit.
The level-owned default-resolution rule hands an explicit height to R3.0.6;
no wall entity gains implicit inheritance. The section 2.1 study log records
the original decision and compatibility assessment. No schema/replay migration
or dependency was added.

Claude Opus 5 implemented the core and benchmark; Claude Sonnet 5 implemented
the Flutter controls/tests. Codex implemented CLI/bridge integration and
independently reviewed both. Review corrected signed-zero change detection,
a non-transitive NaN comparator, premature residual evaluation, a tautological
residual test, unbounded warning overlays and modal lifetime handling.
Review also rejected an unnecessary cache flush on save: moving the same
session does not change either the document or its revision.

## Numerical correctness

For bounded stored binary64 operands, the rounded subtraction cannot cross a
representable comparison plane. A strict rounded result therefore decides the
sign; equality alone needs the error-free subtraction residual. This is an
exact comparison of stored values, without an epsilon or arbitrary-precision
allocation. Domowik's existing independent `two_diff` implementation and its
recorded mathematical provenance are reused; no third-party source was adapted.

Tracked tests assert residuals directly for subnormal values, cancellation,
negative elevations and a ties-to-even midpoint, as well as touching/one-ULP,
zero slab and deterministic duplicate/invalid-order fixtures. A separate local
Python `Fraction` diagnostic checked 88,649 bounded binary64 cases (seed 54)
against exact rational differences; this supplemental experiment is not a
replacement for the tracked regression tests or a formal proof.

## Named-host performance

Apple M4 Max, 16 physical/logical cores, 48 GiB RAM; macOS 26.6.2 (25G83),
arm64. Rust 1.98.0 (`88d9e12ae`, LLVM 22.1.8), Cargo 1.98.0. Measured in a
coordinated project build/test quiet window; the operating system and unrelated
processes were not quiesced.

```sh
cargo bench -p domowik-geometry --bench predicates -- \
  --sample-size 20 --warm-up-time 1 --measurement-time 2 'warnings/levels'
```

| Traversal | Criterion time estimate | Reported interval |
|---|---:|---:|
| Clean, 8 levels | 73.511 ns | 72.316–74.527 ns |
| Clean, 64 levels | 285.64 ns | 282.27–288.95 ns |
| Clean, 512 levels | 1.6189 µs | 1.6008–1.6347 µs |
| All 511 adjacent pairs warn | 2.4622 µs | 2.4483–2.4745 µs |
| All 511 adjacent pairs touch exactly | 1.7293 µs | 1.7129–1.7385 µs |

The fixture inserts descending elevations, spaced 3000 mm apart. Clean slabs
are 200 mm; warning slabs are 4000 mm; touching slabs are 3000 mm. Counts are
asserted before measurement. Every elevation is distinct: this benchmark does
**not** exercise UUID tie-breaking or arbitrary unsorted input. It includes
sorting and result allocation/drop, but does not isolate their individual
costs. No sort-dominance, allocation-count, median/p95, FFI, raster/GPU or
whole-app frame-time conclusion follows from these results.

Raw local log: `/tmp/domowik-plan-w211zo/evidence-0054-warnings-bench.txt`.
Subsequent changes to this measured core path were comments and tests only.

## Verification

- Rust workspace tests: exit 0; subsequent targeted runs passed the final
  131 CLI, 64 format and 8 edit tests.
- Bridge Rust tests: exit 0, 52 tests, including field history/save/open and
  implicit/explicit wall-height resolution.
- Root workspace and bridge Clippy checks: exit 0 with warnings denied.
- CLI process probe P17 is included in `scripts/verify.sh` for actual flags,
  warning-only human/JSON success, height resolution and refused-edit bytes.
- Flutter analysis: clean. The independent full Flutter run passed 151 tests,
  including 55 native lifecycle tests, against the rebuilt release library.
  Coverage includes all five fields, actual elevation reordering, preserved
  local flags/camera, Undo/Redo, save/reopen, no-op/cancel/refused edits, modal
  close blocking/unmount, default-height drawing and bounded warning details.
- Full `scripts/verify.sh` on implementation commit `125734d`: exit 0,
  `=== VERIFICATION PASSED === all stages`, no skipped stages. This includes
  the CLI process probes, dependency/tracker hygiene, Rust benchmark
  compilation, native C/Dart harnesses, 151 Flutter tests, renderer/viewport
  measurement harnesses, and the clean universal macOS bundle/signature and
  built-application probes. Local log: `/private/tmp/domowik-0054-verify.log`.

These results do not close the broader GUI platform/performance gates, room
metadata, selection/snapping, asset placement or 3D work. Domowik is still an
experimental macOS 2D editor, not a complete application or binary release.
