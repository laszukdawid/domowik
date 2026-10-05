# Multi-level view state and rendering — 2026-09-05

Issue 0051 completes the visibility-transition closure gate of 0049. This is
original Domowik work against BEHAVIOR-SPEC sections 2.2–2.6 and 8.7; no
third-party application source, dependencies, ABI changes or file fields.

## Behavior and ownership

Levels sort by elevation, then stable ID. Ghosted passive levels paint first,
normally visible passive levels next, and the active level last. Hidden levels
are absent. Only the active level enters native hit requests, selection,
opening-host targeting, available endpoint snapping and edit commands.
Visibility controls for the active level are disabled. Activating another
level clears selection and ghosts the former active level; activating the
current level is a no-op. Every real flag transition immediately observes a
new selection context, including a hide/show round trip before another click.

Reopen chooses the lowest level, ghosts the others, restores grid/rulers and
fits all drawable content. Save preserves local preferences without serializing
them; view changes emit no document operation or dirty/history change.
Newly appearing authoritative levels start ghosted, including redo restoration.
Add Storey explicitly activates its new level only after accepted creation.
The A/B/C/D transition trace in section 2.4 records this original choice;
exclusion from undo history alone did not determine that default.

Rust still derives every occupied polygon, room and aperture. Flutter caches
one current payload per rendered level, keyed by exact JSON equality of the
level entity, its walls, complete relevant joins and hosted openings. Entire
entities conservatively cover field changes; serialization-order changes can
cause extra work, not false reuse. Hidden/deleted payloads are evicted and a
handle replacement clears the cache. Failed derivation is not cached as empty
or displayed as stale geometry; a later refresh retries it. Future native
derivation inputs must be added to this key when introduced.

Global accepted-document revision still resets selection independently of
per-level geometry reuse. Reads, saves and rejected edits skip snapshot and
geometry work when revision and handle are unchanged. This relies on the
current private UI-isolate handle ownership, not an invented atomic snapshot
guarantee for future concurrent writers.

Geometry collections are borrowed read-only after publication and replaced
on change; repaint comparison does not walk every polygon. Ghosting uses
per-paint alpha 0.32, retaining room labels and all normal content. This avoids
per-level offscreen group compositing but overlapping shapes can darken within
a ghosted level. Grid and rulers paint once, below and above the scene.

## Cross-model work and verification

The Herdr skill coordinated Claude Sonnet 5's level controller and tests,
Claude Opus 5's extracted painter, raster tests and measurement harness, and
Codex's editor integration, native-backed tests and acceptance review. Opus
reviewed the controller and original lifecycle clarification; Sonnet reviewed
cache integration. Root verified findings rather than adopting them blindly:
the editor filters background layers by ID, not an assumed drop-last rule,
and disabling active visibility controls does not require a tri-state setter
result. Defensive missing-flag fallback now agrees with the ghosted default.

Scoped Flutter analysis is clean and 129 tests pass: 49 native lifecycle,
18 selection-controller, 17 level-controller, nine painter, 11 inspector,
11 shell, 13 viewport and one grid test. Native lifecycle teardown checks
handle/buffer ownership; only file-picker responses are substituted.

New cases use actual saved multi-level native documents with deliberately
unordered elevations and tied IDs. They cover paint tiers, visibility menus,
inactive hit/opening/snap isolation, active no-op selection preservation,
immediate cycle resets, per-level payload reuse after wall/opening edits,
hidden payload eviction, save/new/reopen defaults and Add/Undo/Redo transitions.
Pure controller tests also exercise deletion fallback, defensive zero-level
state, atomic input rejection and immutable exported collections.

Raster tests compare distinguishable opaque fills for layer order and visible
ink for ghosting/area labels. Their initial fixture hung because image
rasterization was awaited inside fake async; `tester.runAsync`, explicit
60-second timeouts and disposed image/picture resources fix the test harness.
The owned hung test engine was terminated, not the agent or application.
Pixel probes are inside the enlarged canvas; identical-color overlap was not
accepted as proof of order.

Full `./scripts/verify.sh` exited 0 on implementation commit `72b415e`, ending
with `=== VERIFICATION PASSED === all stages`. This includes the Rust/Go
checks, native sanitizer/FFI harnesses, 129 Flutter tests and clean macOS
bundle/signature/runtime/first-frame probes. Tracker closure is committed
separately and the complete verification rerun on that final tree. The
level-render harness is now a named verification stage so it cannot silently
rot; its timing values are not a host-independent pass/fail threshold.

## Quiet-window renderer measurement

Command: `flutter test --timeout 120s benchmark/level_render_benchmark.dart`, exit 0, one
harness test passed. Opus measured while other agents performed read-only work
and no other project build/test ran; Codex inspected the raw JSON output.

Host: Apple M4 Max, 16 logical processors, 48 GiB RAM; macOS 26.6.2 build
25G83, arm64. Runtime: Dart 3.11.4 stable, 2026-03-24, Flutter-test JIT.
Canvas: 1200 × 800 logical pixels. Each case warms 20 samples and records 200;
percentiles use sorted sample index `ceil(p/100 * n) - 1`.

Each level contains 48 wall records, 48 occupied rings, 24 room rings and
12 apertures. The rectangles are an artificial renderer workload, NOT a valid
enclosed native plan or a claimed representative document. Cases have zero,
one and three ghosted passive levels respectively; other passive levels are
normally visible. Grid/rulers are enabled and drawn once.

| Levels | Mean µs | p50 µs | p95 µs | p99 µs |
|---|---:|---:|---:|---:|
| 1 | 688.456 | 596.000 | 1061.500 | 1328.375 |
| 3 | 921.152 | 862.208 | 1280.125 | 1474.916 |
| 8 | 1880.857 | 1776.042 | 2326.084 | 2474.666 |

These measure CPU display-list recording and picture disposal only. They
exclude Rust derivation, FFI/JSON, widget layout, GPU/raster work and presented
frames; no device frame-budget acceptance or before/after speedup is claimed.
Increasing sample workload also changes JIT/GC conditions, so the observed
sublinear level-count scaling is not an asymptotic performance guarantee.

## Remaining scope

0053 owns authoritative baseline and deletion/validation discrepancies:
the current default is named Ground instead of Level 1. A defensive empty
controller state does not make a zero-level document normatively valid.
Marquee, full snapping, room/level metadata controls, native exact-distance
conformance, Linux/Android execution, physical-input checks, 3D and measured
whole-application frame/memory budgets remain separate unfinished work.
