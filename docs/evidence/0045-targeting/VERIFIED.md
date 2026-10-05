# Shared native editor targeting — 2026-09-05

Issues 0045, 0047 and the separately discovered shell-layout defect 0048.
Implementation follows Domowik's normative behavior specification; no external
application source, copied geometry, new dependency or document-format change.

## Implementation and review

Three models worked in the same Herdr workspace with exclusive file ownership:
Claude Opus 5 implemented the Rust/C/Dart bridge and its probes; Codex supplied
canonical region ordering, Flutter integration and real-native tests; Claude
Sonnet 5 independently reviewed the semantics and implemented the shell fix.
Codex reviewed both implementations, and Opus cross-reviewed Sonnet's layout.
Independent review findings were checked against live code: a claimed missing
level cache key was withdrawn after rereading the actual implementation.

ABI 9 returns ordered join-node, free-endpoint and wall-region candidates.
Flutter converts 12 mouse/stylus or 24 touch logical pixels through the current
zoom and consumes this order for press and click. Openings choose the first
wall-region candidate, then project onto that wall's axis. No fixed 150/400 mm
interaction slack or centre-line-capsule hit approximation remains in Flutter.

Join identity remains the selection identity. A deterministically chosen member
endpoint is only the transport target for the existing native co-move command;
it never becomes the selected wall implicitly. Join deletion does not guess at
wall deletion or detachment. Switching levels cancels a pending grab.

One index is cached for the latest level and active-session log generation.
Reads and saves retain it; edits and history changes invalidate it; alternating
levels rebuild it. The query rejects malformed/nonfinite input without writing
the output buffer. Finite points beyond the **derived** geometry domain widened
by the radius return an empty result before index arithmetic. A wall centred on
the stored coordinate limit can still be hit beyond that limit.

## Executed scoped checks

- All 114 geometry library tests, including a regression proved failing before
  the canonical ring-order correction.
- All 44 Rust bridge tests: order fidelity (including equal-distance unequal
  regions), level filtering, input/radius boundaries, cache reuse/invalidation,
  saved/opened sessions, output ownership and refusal sentinels.
- C harness with AddressSanitizer and UBSan: 112 checks, zero failures.
- Dart native harness: 83 checks, zero failures.
- All 32 real-native editor lifecycle tests. New cases cover both zoom clamps,
  mouse/stylus/touch reach, inclusive ordinary-view boundaries, butt-cap misses,
  selection/press consistency, opening targeting, opening-tool drag isolation,
  joined-corner identity/co-movement/cancel/undo and level-switch cancellation.
- All 57 Flutter tests, including 11 shell tests. Status allocation uses one
  resolved text style, forced strut, locale/direction/scaler and a measured
  two-line paragraph; the temporary painter is disposed. Tests check compact
  and desktop layouts, message/error transitions, enlarged and tiny scaling,
  Unicode, tooltip access and actual rendered line boxes inside their bounds.

The first C run failed because new malformed-request probes set the sticky
error on the fixture used by an older empty-buffer test. The hit-test probes now
own a separate plan, preserving both real error persistence and the existing
empty-buffer token test. The first Flutter run exposed issue 0048: status text
changed canvas height by 17 px, moving its camera by 8.5 px. The targeting tests
pass with stable status allocation, without recomputing away that regression.

## Limits

This verifies frontend consistency with the native geometry implementation,
not full numerical conformance to R7.1.3. Distances and projections still use
binary64; exact-real predicates remain issue 0012. Full selection cycling,
marquee, placeholders and snapping remain separate work. Tests inject pointer
events, not physical stylus/touchscreen/trackpad signals. Linux/Android execution
and whole-application performance budgets are still open.

Native build/query costs are recorded separately in
[the ordering measurements](../0047-hit-ordering/MEASURED.md); they exclude
FFI serialization, Flutter layout and presented-frame timing.

Full `./scripts/verify.sh` passed against implementation commit `8fad110` with
`=== VERIFICATION PASSED === all stages`, including the clean macOS build,
bundled-library ownership/panic checks, rendered first-frame probe, benchmark
compilation, inventory and tracker checks. The tracker closure is committed
separately and the full verification is rerun against that final tree.
