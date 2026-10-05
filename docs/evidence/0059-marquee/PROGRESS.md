# Marquee selection checkpoint

The Rust marquee query now finds walls crossed by extremely large finite
rectangles. The Flutter gesture consumes that query once on release. This
checkpoint does not establish complete R7 conformance or application frame
performance.

## Correctness and remaining work

A one-millimetre vertical band with corners `(2000, -1e308)` and
`(2001, 1e308)` previously missed a horizontal wall from `(0, 0)` to
`(4000, 0)`, thickness 200 mm. Its intersection parameters overflowed. The
query now clips against a limit containing each candidate's computed polygon
bounds before testing intersection. The original equal-corner point branch
runs before clipping.

Three tests fail without the repair: the vertical band, its transpose through
a vertical wall, and reversed-corner equivalence. Eight additional tests guard
ordering, tangency, misses, degeneracy and out-of-envelope eligibility.
Independent `cargo test --locked -p domowik-geometry` passed 176 unit tests and
6 integration oracle tests; `cargo fmt --check` and geometry all-target clippy
with `-D warnings` also exited 0 on 2026-09-07.

The clip preserves the geometric intersection of the computed shapes. It does
not make their rounded construction or subsequent intersection arithmetic
exact. Overflow protection applies when the computed candidate bounds fit
the derived-coordinate envelope; oversized geometry gets no such guarantee.
Issue 0012 retains exactness. The R7.3.2 interpretation for line rectangles
was open when this was written and is settled below by R7.3.2a. Placeholder
selection remains dependent on issues 0012/0026.

The native-backed Flutter suite passed 171 tests against ABI 11. Its coverage
includes additive selection, unchanged plain wall drawing, cross-level
exclusion, preview lifetime and cancellation. The C sanitizer harness passed
127 checks and the Dart FFI harness passed 85. The parallel Rust bridge suite
failed a close/quiescence test; a serial rerun passed, but that is not a fix.
Issue 0063 repaired the tests with controlled admission and wait signals,
including regression checks against a bypassed wait. After integration at
commit `d0be026`, the canonical `./scripts/verify.sh` run on 2026-09-07 ended
with `=== VERIFICATION PASSED === all stages`, with no skipped stages.
It passed 72 native unit tests and 171 Flutter tests and included macOS
packaging. Local log: `/private/tmp/domowik-0059-0063-final-verify.log`.

## Query measurements

Host: Apple M4 Max, 16 cores, 48 GiB RAM, macOS 26.6.2 (25G83),
`aarch64-apple-darwin`. Compiler: rustc 1.98.0 (88d9e12ae).

```sh
cargo bench -p domowik-geometry --bench spatial -- spatial/marquee \
  --sample-size 30 --warm-up-time 0.5 --measurement-time 1.0 --verbose
```

The grid contains 4 m by 3 m rooms with shared wall segments. Enclosing queries
select every wall; a 1 mm horizontal band through the bottom row exercises the
repaired edge-crossing path. Each bounded/huge pair asserts identical ordered
IDs before timing. Document creation and those assertions are outside the
timed section; query scanning, polygon construction, result allocation and
sorting are inside it.

Final run on 2026-09-07, 13:30:25 to 13:31:02 UTC, exit 0. Values below are
Criterion's median confidence intervals, in microseconds, not latency
percentiles.

| Query | Selected / walls | Bounded corners | Huge corners |
|---|---:|---:|---:|
| Enclosing, 4 by 4 | 40 / 40 | 0.52699 to 0.53212 | 0.52632 to 0.53161 |
| Band, 4 by 4 | 5 / 40 | 0.45983 to 0.46944 | 0.46737 to 0.47199 |
| Enclosing, 10 by 10 | 220 / 220 | 2.6646 to 2.7094 | 2.6298 to 2.6762 |
| Band, 10 by 10 | 11 / 220 | 1.3664 to 1.3789 | 1.3546 to 1.3672 |
| Enclosing, 20 by 20 | 840 / 840 | 10.929 to 11.011 | 10.948 to 11.076 |
| Band, 20 by 20 | 21 / 840 | 3.6359 to 3.6730 | 3.6570 to 3.6987 |

Both variants clip, so these comparisons measure sensitivity to query
magnitude, not overhead against the previous implementation. Historical
Criterion cache comparisons in the first exploratory run are not a controlled
baseline. Workers coordinated a build-free timing window; operating-system
quiescence was not established. The final run's load averages were
`{2.34, 3.28, 2.89}` before and `{3.80, 3.52, 2.99}` after. FFI serialization,
Flutter painting, input latency and allocation counts were not measured here.

## Completion, 2026-09-12

The earlier checkpoint carried R7.3.2's "zero-area" wording forward without
resolving it. R7.3.2a now scopes the degeneracy that takes point-hit semantics
to COINCIDENT CORNERS, and fixture L13 states the case that separates the two
readings. Over the F2 join node, coincident corners select `J1` alone, because
R7.1.3 ranks the join above the regions meeting there; a line rectangle from
that same corner selects both wall regions it crosses. The core already
behaved this way. What was missing was the authority for it, and tests that
fail when the reading changes.

The tests fail against a deliberately broken core. Changing `marquee`'s
`rect.width() == 0.0 && rect.height() == 0.0` to `||` makes
`a_line_rectangle_keeps_rectangle_semantics` return the join instead of the
two walls, and, once the bridge is rebuilt, makes the native-backed L13 widget
test report an empty selection: the band's own min corner is empty canvas, so
a point reading selects nothing there. The branch was restored and the library
rebuilt before the results below.

R7.4 after a marquee and the R7.5 rules that v0.1 can express now have
native-backed coverage in `editor_lifecycle_test.dart`. Deleting a
marquee-designated primary leaves the other member selected with primary
UNSET; undo re-creates the wall without reselecting it; idle Escape clears
(fixture L5, reached from a marquee instead of from clicks). A rectangle
spanning every level still yields the active level's walls when the inactive
levels are rendered at full strength and again when they are hidden outright,
which is what R7.5.1 means by hiddenness being the level-state model.
Activating another level clears the result (L7). A primary-targeted body drag
after a marquee keeps the whole selection and its primary and moves only that
wall, which is R7.4's first row and the closest this issue comes to the "no
group commands" rule. In the core,
`a_marquee_never_reaches_another_level` asserts the same exclusion directly
and answers the same rectangle against the upper level to show the filter is a
level filter.

Placeholders (R7.5.3) are NOT implemented, and the reason is missing plumbing
rather than a missing definition. An earlier draft of this section said v0.1
defines no asset-instance entity; that was too broad and is withdrawn.
FORMAT-SPEC section 2.3 reserves a typed `assetInstance` envelope in Tier B
with required `levelId`, `digest` and `transform`, section 9.6 fixes the
degraded box geometry and says the placeholder participates in hit testing as
its entity normally would, and fixture PH1 states the marquee, click and
deletion results with concrete coordinates. Only the SEMANTICS of placement
are draft.

What is absent is implementation, in three places: `domowik-format` carries no
`AssetInstance` (issue 0010), no asset-store query answers "is this instance
degraded, and what extent does its descriptor declare" (issue 0026), and
R7.1.3's `placeholder-box` hit class is unimplemented, which issue 0012
records as its own gap. The issue file now carries the exact prerequisite
interface for all three, so this can be finished rather than left excluded.
The transport is already entity-agnostic, and that is now checked rather than
asserted: a PH1 group in `selection_state_test.dart` runs a placeholder
instance ID through the controller's marquee, primary designation and
survivor pruning, and it behaves exactly as a wall ID does, which is why the
controller needs no change for R7.5.3. What the controller cannot cover is
everything deciding WHICH ids arrive. The issue file carries the core PH1 test
ready to apply, against PH1's own arithmetic: the 1000 mm box at origin
(2000,0) spans x ∈ [1500,2500] and y ∈ [-500,500], which PH1's snap probe
independently confirms by placing the right face at 2500. Two of PH1's steps
are not claimed there: the snap probe belongs to issue 0058, and the deletion
step needs a `deleteAssetInstance` gesture and its bridge call, an ABI change
sequenced by root.

No ABI change. `domowik_plan_marquee` was introduced in ABI 11; the bridge is
at ABI 12 and nothing here bumps it.

Independent runs on 2026-09-12, all exit 0: `cargo test --workspace` (686
tests), `cargo test -p domowik-geometry`, `cargo fmt --check -p
domowik-geometry`, `cargo clippy -p domowik-geometry --all-targets -- -D
warnings`, `flutter analyze` (no issues) and `flutter test` (198 tests).
`./scripts/verify.sh` at commit `f62b651` on 2026-09-12 ended with
`=== VERIFICATION PASSED === all stages` and exit 0. No stage announced
SKIPPED: cargo, flutter, dart and go were all on PATH, so the run included the
Flutter macOS release build and its packaging probes. The tree was clean
before and after. Log: `verify-full.log` in the session scratchpad.

That run is EVIDENCE, not closure, and one limit is recorded rather than
buried. An operator scripting error granted the verification slot to five
worktrees at once, so five verifiers ran concurrently on this host and the
load average was about 11.6 during the run. A pass or fail banner does not
depend on load, so the banner stands. Every timing the run printed does
depend on it, so NO measurement from this run is recorded here, and none is
needed: no acceptance criterion of this issue is a measurement, `verify.sh`
compiles benchmarks without timing them, and the query measurements above
come from the separate 2026-09-07 window.

## Adversarial review, 2026-09-12

Codex `gpt-5.6-sol` at high reasoning effort, banner checked in the output
(`model: gpt-5.6-sol`, `reasoning effort: high`), reading only the branch diff
and an extract of section 7.3. It was asked four narrow questions: whether
R7.3.2a contradicts anything in 7.3, whether L13's arithmetic holds, whether
the new tests discriminate, and whether closing as Done was honest.

It confirmed R7.3.2a against R7.3.1 and L2, noting that L2's zero-area
quantity is the INTERSECTION rather than the input rectangle, so R7.3.2a does
not reach it. It recomputed L13's boxes, order and primary and agreed. Three
findings changed the work:

- L13's Result B said the segment "lies on `W1`'s right face", which reads as
  the whole 500 mm. Only y ∈ [0,100] does. The fixture now says so.
- The widget test was named for L13 but exercised only Result B. It is
  renamed, and it now records why Result A is unreachable through a gesture:
  the preview needs a classified drag, classification needs 5 logical pixels,
  and an affine transform at positive zoom cannot map two such points to one
  plan point. Result A is asserted in the core instead, by
  `a_line_rectangle_keeps_rectangle_semantics`, which covers both halves.
- It judged the Done closure dishonest, because the placeholder criterion was
  checked off while the same file admitted the rule was unimplemented. That
  finding is accepted: the box is unchecked again, the criterion text is
  restored verbatim, and the issue returns to partial scope with issue 0012
  named as its deferred-closure gate.

One finding is rejected on evidence. The reviewer read a duplicated
`reason:` label at `0059.diff:760` and concluded the patch could not compile,
and therefore that the reported Flutter results could not describe it. The
duplication is in the reviewer's own transcript, not the artifact: the diff
and the source each carry one label (`editor_lifecycle_test.dart:2856`), and
`flutter analyze` reports no issues against that file while `flutter test`
runs it.

## Local evidence and provenance

- `/private/tmp/domowik-0059-fix-red.log`: missing-clip regressions, exit 101.
- `/private/tmp/domowik-0059-final-tests.log`: geometry suite, exit 0.
- `/private/tmp/domowik-0059-marquee-bench.log`: exploratory run followed by
  the final count-labelled run reported above, both exit 0.
- `/private/tmp/domowik-0059-final-*-20260907-063137.log`: bridge, harness and
  Flutter integration results, including the parallel close-test failure.

These are local diagnostic files, not portable CI artifacts. Implementation,
fixtures and the clipping argument derive from Domowik's specification and
independent reasoning. No comparable-application source or assets were used.
