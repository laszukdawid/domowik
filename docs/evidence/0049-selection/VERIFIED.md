# Selection cycling and inspector resilience — 2026-09-05

Issues 0049 and 0052; native feature inventory is separately tracked in 0050.
Original Domowik decisions and implementation, no external application source,
new dependency or persisted document-format change.

## Contract and implementation

Click selection projects the native feature order to first-occurrence entity
IDs: one entity is one cycle stop, even when several endpoints/regions hit.
Plain repeated clicks cycle; Ctrl/Meta/Shift-click toggles the first entity
without cycling. Alt alone remains plain. Selection stores a set and optional
primary; removing the primary never silently appoints a survivor.

The cycle anchor stays at the first logical-pixel position. Finite excursions
beyond 12 px (mouse/stylus) or 24 px (touch), and observed view/modifier/pointer
transitions, reset immediately even if they return before the next click.
Native accepted-batch revision changes also reset, including distant edits
with identical local candidates. Save, refusal and no-op refresh do not.

Plain press on the current primary preserves the whole set for a potential
drag; another hit tentatively becomes sole primary. Click release evaluates
the pre-press selection, while cancellation restores it without reviving an
invalidated cycle. Classified drags that return to their start do not become
clicks. Commands remain primary-only. Modifier drags on non-primary entities
or empty space emit no edit; additive marquee remains unimplemented.

`domowik_plan_state` adds `document_revision`, the active session's accepted
batch count. Reads and saves do not advance it; undo/redo append compensating
batches. Reopen continues the saved log count; new sessions start at zero.
It is comparable only within one handle. ABI 9 signatures and other response
objects remain unchanged; this is an additive JSON field, not a file field.
The hit cache also keys the requested level, independently of this revision.

The inspector now shows the selection count and a no-primary state, colors
all selected objects and distinguishes the primary. Diagnostics expansion
uses page-storage identity so conditional selection controls cannot collapse
it by shifting list indices. Metric labels/values wrap instead of overflowing
the 248 px panel; long content remains available in its scrollable inspector.

## Cross-model review

The Herdr skill coordinated Claude Opus 5 and Claude Sonnet 5 alongside Codex.
Opus owned native revision/feature changes and performance measurements;
Sonnet reviewed the original contract, implemented the pure controller and
later the isolated metric widget. Codex ratified the original choices before
integration, wrote actual-native widget tests and reviewed both implementations.
Opus cross-reviewed the editor and Sonnet's inspector implementation.

Review corrected substantive process and implementation gaps: comparing only
local candidate IDs misses distant accepted edits; singular modifier wording
did not settle overlap cycling; an assertion-only public snapshot guard would
accept impossible states in release. Explicit R7.2 rules and L9–L12 fixtures,
native revision observations and a release-safe ArgumentError address them.

Review claims were checked, not adopted automatically. The pinned Flutter
RenderWrap constrains horizontal children by the parent's maximum width;
an alleged unbounded-child defect was withdrawn after inspecting it.
PageStorage is bucket/path scoped, not process-global, and this key stores only
the diagnostics expansion preference. Native lifecycle tests deliberately
invoke actual Save/diagnostic callbacks without pointer movement to isolate
revision resets; separate tests exercise real taps and pending-dialog exclusion.

## Scoped verification

- 118 geometry library tests and 48 Rust bridge tests pass independently.
- C AddressSanitizer/UBSan harness: 114 checks, zero failures. Dart FFI
  harness: 85 checks, zero failures; both exercise native revision fields.
- Flutter analysis is clean and all 96 tests pass: 42 lifecycle, 18 selection
  controller, 11 inspector metric, 11 shell, 13 viewport and one grid test.
  Metric tests check visible text-run boxes in global parent coordinates,
  including 120 px at 3× text scale, unbroken 40-character tokens and Arabic
  RTL. The fixture uses an unbounded-height scrollable parent like production;
  hanging whitespace boxes are not mistaken for visible glyph overflow.
- 42 actual-native editor lifecycle tests pass, with no leaked native handles
  or buffers after each test. Only file-picker responses are substituted.
- New editor cases cover feature-to-entity cycling, separate join/member-wall
  stops, toggles, primary-unset deletion/undo, primary-only drag, cancellation,
  non-destructive reserved modifier drags, observed round-trip resets, save and
  refusal stability, distant accepted edits/undo, classified return-to-start
  drags, and selection clearing on file and active-level replacement.

The first runs caught test-setup mistakes (tooltip versus text finder, lazy
inspector scrolling, and the incorrect assumption that undo clears dirty).
After those corrections, the native revision test exposed the real diagnostics
row overflow and lost expansion, now tracked and repaired under 0052.

Full `./scripts/verify.sh` exited 0 against implementation commit `ff1214d`
with `=== VERIFICATION PASSED === all stages`. It included benchmark
compilation, inventories, tracker/clean-tree checks, all 96 Flutter tests,
and the clean macOS build with bundled-library and rendered first-frame
probes. Tracker closure is committed separately and the same full verification
is rerun against the final tree.

## Original unclosed scope and subsequent closure

Update: issue 0051 (`72b415e`) supplies the controls and immediate visibility
observations described below. All 129 Flutter tests and full repository/macOS
verification pass; [level evidence](../0051-level-visibility/VERIFIED.md)
records the native round-trip regression. That resolves 0049's closure gate.
The following paragraph records the earlier increment's boundary, not the
current application state.

0049 remains partial until 0051 supplies real level visibility/ghost controls
and wires their transitions into the tested controller context. Its current
visibility revision is explicitly zero because no such controls exist yet.
This is not a claim of full R7 GUI conformance. Marquee, placeholders, full
snapping, native exact-distance conformance (0012), Linux/Android validation
and whole-application performance budgets remain open. Input tests inject
events, not physical mouse/stylus/touch/trackpad hardware signals. Native query
measurements in the 0050 report exclude FFI, layout and presented-frame costs.
