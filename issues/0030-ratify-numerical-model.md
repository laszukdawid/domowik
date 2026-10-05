# 0030: Ratify numerical geometry model after spikes

- **Phase:** 0
- **Priority:** High
- **Status:** In Progress (partial scope)
- **Depends on:** 0006, 0007, 0022

Activated under ED-0011 after the reviewed partial-scope 0007 handoff. Issue
0030 owns the deferred structured face-input-or-rejection decision and must not
treat 0007's withdrawn `CandFaces` claims as conformance evidence.

## Problem

Turn the predicate, clipping, and planar-topology evidence into one final
coordinate/numerical decision before entity numeric bounds or Phase 1 geometry
are implemented.

## Acceptance criteria

- [x] Correctness, determinism, coordinate range, quantization/tolerance,
      overflow, and performance evidence is summarized.
      (`../docs/NUMERICAL-MODEL.md` §13, for the Rust core.)
- [x] Selected algorithms/libraries and exact licenses/provenance are recorded.
      (`../docs/NUMERICAL-MODEL.md` §14.)
- [x] Define-before-judge: 0030 defines the Phase 0 performance criterion
      (the oracle) BEFORE judging acceptance, then judges only against it.
      Evidence producers are 0006 predicate benchmarks, 0007 pipeline
      wall-clock/allocation measurements, and H-6 fallback/escalation counts;
      there is no other producer and no other judge.
- [x] ADR 0002 is updated with whether its geometry gate passed and any custom
      maintenance burden.
- [ ] Format numeric bounds and geometry API guarantees are approved or the
      candidate design is revised and spikes rerun.
      (Bounds ratified by ED-0100 and recorded at FORMAT-SPEC §3.1/L14;
      geometry API guarantees ratified by ED-0103 as
      `../docs/NUMERICAL-MODEL.md` §12 and IMPLEMENTED — see the closure
      below.)

## Partial handoff (2026-08-24)

Reviewed issue branch `issue/0030-ratify-numerical-model` at `92b8cf4` was
merged to the main repository by `ed10f87`; integration SBOMs were refreshed
by `8820283`. The full merged-main `./scripts/verify.sh` run exited 0 and the
tree was clean.

The committed criterion was defined before judgment. Existing evidence was
then judged **NOT-JUDGEABLE** because it does not contain the required
independent-process medians, per-case boolean p95/RSS measurements, or H-6
fallback/escalation counts. This is a valid judgment against the registered
criterion, not a performance pass.

ADR 0002 now records gate 1 as open and the numerical model as Candidate.
ED-0012 rejects the provenance-erasing flat `CandFaces` input; no structured
replacement is ratified. B-1x is a leading but unselected candidate, while
B-1q remains unimplemented, nonconforming, and selection-blocking without a
runtime certify-or-escalate path. Direction validation now rejects loose
certificates at or above 1, but certificate authenticity and the exported API
remain unratified.

Open closure work includes the registered performance reruns, H-0/H-1/H-2/
H-5/H-6 (and partial H-3) evidence, post-normalization independence,
Direction provenance/API guarantees, numeric bounds, FORMAT L14/R3.0.8, and
either a provenance-preserving structured face contract or explicit removal
of face extraction from the candidate surface.

## Integrated v3 performance disposition (2026-08-24)

The corrected-source v3 rerun is integrated on main (`a1bf59d`, inventory
refresh `e8da5e7`, merge `db7c5a9`, and main SBOM refresh `d8e25c6`). It is
judgeable for its registered scope, unlike the historical 0006/0007 evidence.
See `docs/evidence/0030-numerical-model/PHASE0-PERFORMANCE-RERUN.md` for the
raw manifest, provisional report, provenance, and analyzer hashes.

The registered Phase 0 criterion is **FAILED** for this scope on A3 robust
medians: `BenchmarkIntersectSegmentsRobust` (12779.5 ns/op) and
`BenchmarkOffsetCertificateAndVertex` (20949.0 ns/op), versus the 10000 ns/op
bound. A1, A2, filtered A3, A4, and applicable A6 observations were within
bounds. H-6 boolean-pipeline, CandFaces, OracleFaces, and B-1q certification
domains remain **MISSING**, never zero-filled.

The numerical model is not ratified or closed: `PARTIAL / NOT COMPLETE` remains
in force, ADR 0002 remains Proposed, and the ratification/promotion gate is
open for the Numerical Model §11 trigger-3 revision decision. Main
`./scripts/verify.sh` passes at clean HEAD `d8e25c6`.

## Continuation wave (2026-08-24)

A fresh executor-reviewer pair continues this issue on branch
`issue/0030-trigger3-disposition`, scoped to the NUMERICAL-MODEL §11 trigger-3
assessment: whether a caching/locality strategy (R5.1.9) can plausibly close
the A3 robust medians that failed the registered v3 criterion
(`BenchmarkIntersectSegmentsRobust` 12779.5 ns/op,
`BenchmarkOffsetCertificateAndVertex` 20949.0 ns/op vs the 10000 ns/op bound),
and the resulting disposition (required follow-up evidence versus an ADR-0002
revision-trigger record). It may not silently alter registered thresholds or
protocol rules, promote the model, ratify §6 items, or edit normative
BEHAVIOR-SPEC semantics.

**Trigger-3 disposition recorded (2026-08-24).** Branch
`issue/0030-trigger3-disposition` merged to main by `b51e2be` with SBOM
refresh `2b96d8e`; full main `./scripts/verify.sh` exits 0.
Outcome (b): NOT PLAUSIBLY CLOSABLE via R5.1.9 caching/locality — the judged
statistic is a per-op median over cold rotating-corpus inputs, and OCV stays
above the bound even if all structural re-validations were cached. Revision
trigger 3 FIRES; the ADR-0002 reassessment is recorded as RECOMMENDATION ONLY
(`docs/evidence/0030-numerical-model/TRIGGER3-DISPOSITION.md`,
RATIFICATION addendum, ADR-0002 dated note); any stack decision belongs to the
owner. Gate 1 remains OPEN; model remains Candidate/unratified; PARTIAL /
NOT COMPLETE stays in force. Adversarial review verdict: ACCEPTED; one
process deviation (edits began during a reviewer-provider quota outage before
final conditional agreement) was disclosed by the pair and covered by the
reviewer's subsequent full-diff review.

## Status corrected by ED-0096 — 2026-09-01

**In Progress (partial scope).** The define-before-judge criterion and the ADR 0002 geometry-gate update are discharged. NOT delivered, and this is why the status is not Blocked: three criteria name work nobody is waiting on an owner for — the evidence summary across correctness, determinism, range, quantization, overflow and performance; the algorithm/licence/provenance record; and the approval or revision of the format numeric bounds. Calling this Blocked hid unstarted work behind a decision that has not been asked for yet.

## Bounds ratified — 2026-09-04 (ED-0100)

The numeric half of this issue is done: coordinate bound, dimension cap,
derived-coordinate bound and topology quantum are ratified and enforced, and
`docs/NUMERICAL-MODEL.md` §3 records them with the reasoning. The candidate
machinery in §§4.2–4.5, 6, 10 and 11 is marked SUPERSEDED rather than pending,
because it describes a Go-era design that was never built; retaining it as an
open decision would have meant ratifying a fiction.

**What this issue still owns**, now stated concretely rather than as a
Go-era evidence backlog — see NUMERICAL-MODEL §12:

1. Quantization of stored coordinates onto the topology grid, or an explicit
   statement that sub-quantum geometry is topologically absent.
2. ONE "same point" rule. `VertexKey`, join validation and cut deduplication
   currently implement three that disagree at the margins.
3. Split-point persistence: nearest-float versus exact rational storage.
4. The exact area and half-even display contract of BEHAVIOR-SPEC §5.3, which
   the CLI does not currently meet — translation made area accurate, not exact.
5. The positive-area threshold in `geometry::locality`, which decides R5.1.9
   component linking and is therefore format-observable semantics.

## Ratification complete; implementation conformance outstanding — 2026-09-12

**ED-0103 decides the five questions ED-0100 left open.** They are recorded as
N-1 to N-5 of `../docs/NUMERICAL-MODEL.md` §12, which is now ratified numeric
policy rather than a list of open points. The model document's status header
changed from Candidate to partly ratified and names the sections in force.

Two acceptance criteria are discharged by this work: the evidence summary
(§13) and the algorithm/provenance/licence record (§14). Both cover the RUST
core and say so; neither restates the Go-era evidence ED-0100 superseded.

**Three of the five decisions ratify the implementation. Two rule against it,**
and those two are why the last acceptance criterion stays unchecked:

- **N-4.** R5.3.1/R5.3.2 state an exact area with half-even display. The CLI
  formats `area_mm2()/1e6` with `{:.2}`, which rounds the stored double, and
  **both of R5.3.2's normative tie examples fail** — 30,245,000 mm² displays
  `30.25 m²` where the rule requires `30.24`, and 10,655,000 mm² displays
  `10.65` where it requires `10.66`. Neither case is asserted by a test today.
  The contract was NOT weakened to match the code.
- **N-5.** R5.1.9 links components whose regions intersect with POSITIVE AREA.
  `geometry::locality` applies `OVERLAP_RELATIVE = 1e-12` scaled by the operand
  areas, whose stated justification — area error near the coordinate bound —
  was made stale by ED-0100's own translation fix. What it does now is
  under-include: above 1 m² it overtakes the absolute floor, and at 10¹² mm²
  it suppresses a genuine 0.5 mm² overlap.
- **N-2** additionally leaves a join-admission repair: identity and admission
  contradict each other in both directions, and the admission test's verdict
  depends on which member is listed first, though FORMAT-SPEC §2.3 (C3) makes
  membership an unordered set.

**Why this issue is not Done.** The ratification it owns is complete. What
remains is implementation conformance to rules the specification already
carries, and closing on the strength of a finished decision would leave that
work with no owner — the failure ED-0102's guardrail names.

**Proposed bounded ownership split, for root to assign.** No file is claimed by
two issues:

| Work | Files | Note |
|---|---|---|
| N-4 exact area | `domowik-geometry::polygon`, `::dyadic` | Machinery exists; `Dyadic` is arbitrary-width and a shoelace is degree 2 |
| N-4 display | `domowik-cli`, and the room display path | Overlaps issue 0060, which is building `display_area_m2` now |
| N-5 exact linking | `domowik-geometry::locality` | After N-4 |
| N-2 admission repair | `domowik-format` join validation and the two constants | Changes validation outcomes at the margin; format owner's call |

Issue 0058 holds exclusive ownership of `boolean.rs`, `walls.rs` and
`SnapIndex` for ED-0103 Part 2; none of the above touches those files.

**Also delivered under this issue.** BEHAVIOR-SPEC §5.4 fixtures F27 and F39
were corrected: both stated courtyard geometry with square corners that
R3.2.3's convex-hull patches do not produce, contradicting F26, which derives
the chamfer explicitly. Re-derived in exact integer arithmetic from R3.2.3 and
the fixtures' own wall axes, not from implementation output, with provenance
recorded in the specification.

## Closure — 2026-09-12

**Done.** The ratification this issue owns is complete and the implementation
conformance it named is delivered.

- **Implementation/spec commits:** `6e5dbfb` (ED-0103, NUMERICAL-MODEL §§12–14,
  F27/F39 correction, issues 0006/0007/0022 closed), `e331dbd` (FORMAT-SPEC
  numeric status), `0bacea7` (exact area, exact R5.3.2 rounding, exact R5.1.9
  linking), `c76fbc5` (join agreement on the topology grid).
- **Verified:** `cargo test --workspace` all green (30 suites, no failures),
  `cargo clippy --workspace --all-targets -- -D warnings` clean,
  `cargo fmt --all --check` clean, `cargo bench --no-run` exit 0,
  `domowik-tracker-check` and `domowik-jsoncheck docs` exit 0. No full
  `verify.sh` or timing run: the shared slot was held by issue 0024.

**What changed against the specification, rather than to accommodate the code.**
Three of ED-0100's five open questions ratified the implementation; two ruled
against it, and both are now fixed rather than written down as tolerated:

- R5.3.2's own tie examples failed — 30,245,000 mm² displayed `30.25` where the
  rule requires `30.24`. Area is now exact and its display rounding is an
  integer comparison, so a tie is detected rather than approached.
- R5.1.9's positive-area test carried a relative floor that suppressed a real
  0.5 mm² overlap between kilometre-scale regions. It is removed, not retuned.
- R3.3.4's join agreement depended on which member was listed first, though
  FORMAT-SPEC §2.3 makes membership an unordered set. It is now topology-cell
  equality: one "same point" rule, shared with the arrangement.

**Residuals, named with their owners rather than closed over.**

1. **The boolean pipeline can displace a constructed vertex by one ulp** when a
   cut parameter lands on an existing vertex (witness in ED-0103 amendment
   (a)). It leaves `positive_area` reporting `Uncertain` for about 3% of
   touching component pairs, which `locality::link` resolves as a link — over
   inclusion against R5.1.9's "EXACTLY", recorded as a deviation. Fixing the
   vertex removes both. `arrangement.rs`/`boolean.rs` belong to **issue 0058**.
2. **The level total is rounded twice**: the bridge exports `floorAreaMm2` as a
   raw `f64` sum and the Flutter side formats it with `toStringAsFixed(2)`,
   which is not half-even. The exact sum is now expressible; exporting the
   rounded value instead is an ABI change in **issue 0024**'s files.
3. **Five display call sites still format the float area.** Issue **0060**
   owns the single string formatter and will consume
   `exact_area::display_hundredths_m2` once this branch is merged; it is
   holding deliberately rather than building against an unsequenced branch.

None of the three is a numerical-model decision. Each is a consumer adopting a
decision this issue has now made and implemented.

## Reopened — 2026-09-12: R5.1.9 is not closed exactly

**Status returns to In Progress (partial scope)** on root's direction. The
closure below stands as a record of what landed; what it claimed about R5.1.9
does not.

`locality::link` resolves `PositiveArea::Uncertain` as a link. That is
over-inclusion against a rule that says EXACTLY which faces recompute, so
R5.1.9 conformance is unmet, and an issue whose remaining work is a known
deviation from a normative rule is not Done. The numeric criterion is
unchecked again with it.

### Why the current approach cannot close it

The overlap is computed as `area(A) + area(B) − area(A ∪ B)`, which makes the
verdict depend on the union's coordinates. Two separate failures follow:

1. A constructed cut is `lerp(a, b, t)` with `t = num/denom` and
   `denom = |r||s|·sin θ`, so its displacement is bounded by `|r|·6u/|sin θ|`
   and is unbounded as segments approach parallel (issue 0058's derivation and
   its 60,000-pair measurement: 1.18×10⁻⁵ mm at `sin θ = 10⁻⁵`, reaching the
   topology quantum near `10⁻⁷` and 1.14×10⁻¹ mm at `10⁻⁹`). No constant band
   covers it, which is why ED-0103 amendment (b) withdrew the one this issue
   had.
2. Even with no constructed coordinate, "every union vertex came from the
   input" proves only that no coordinate was invented. It does NOT prove the
   union got the TOPOLOGY right — a routine can assemble rings from input
   vertices alone and still drop a fragment or mis-classify one. `Yes`/`No`
   therefore rest on the boolean routine being correct, which this issue does
   not establish.

Both vanish if the decision never consults a union at all.

### SUPERSEDED by ED-0103 clarification (c) — the raw-piece route was wrong

Kept as the record of a route that was proposed, validated, and then correctly
rejected. It decided overlap on raw wall rectangles and join patches, which is
a SECOND GEOMETRY rather than the one the system uses, and it answered the
stored-versus-welded question the wrong way: a 0.4 µm overlap is topologically
absent under N-1 and must not link, and this route links it. Separating-axis
over convex pieces remains admissible only where equivalence to the welded
topology is certified.

The route now in force is in ED-0103 clarification (c): decide the WELDED
occupied regions exactly, by proper crossings, collinear same-side overlap,
and partitioned edge-interior witnesses over existing vertices, with
containment as ring enclosure rather than occupied-set membership.

### Proposed closure (SUPERSEDED): an exact interior-intersection predicate

**Reduction.** A component's region is the union of its pieces: one rectangle
per wall (`WallSolid::rectangle`) and one patch per join (`walls::join_patch`).
For finitely many closed sets,

> `int(⋃Aᵢ) ∩ int(⋃Bⱼ) ≠ ∅  ⟺  ∃ i,j : int(Aᵢ) ∩ int(Bⱼ) ≠ ∅`

(⇐ is immediate; ⇒ takes a ball inside both unions, notes finitely many closed
pieces cover it, so some `Aᵢ` and some `Bⱼ` each have positive measure in it,
and their interiors meet within it.) So the component question reduces to
piece-against-piece, and **no union is computed for this relation at all**.

**Every piece is convex.** A wall rectangle is convex; a join patch is a convex
hull (R3.2.3) and therefore convex. Both are built from stored wall endpoints
and thicknesses.

**The predicate.** Two convex compact sets have disjoint interiors iff a
separating line exists, and for convex polygons such a line may be taken
parallel to an edge of one of them. So for each candidate axis — the normal
`(−dy, dx)` of each edge of each piece — project both pieces and test whether
the intervals overlap in POSITIVE length. Interiors intersect iff no axis
separates. Each projection is `−dy·vx + dx·vy`: a degree-2 polynomial in
stored coordinates, with no division anywhere, so the sign is decidable
exactly by `dyadic` and by a filtered float with a proven bound — the same
filter-then-exact discipline `orientation` already uses. Degenerate pieces
(zero-area rectangle, collinear patch) have empty interiors and are skipped by
an exact zero-area test.

**Bounded.** For components of `a` and `b` pieces the work is `a·b` piece
pairs; a rectangle has four edges and a patch at most `2·members`, so each
pair is a small constant number of exact sign tests. Bounding boxes prefilter
per component and per piece, as now. There is no escalation whose depth
depends on conditioning, because there is no division to condition.

**No epsilon.** Positive-length interval overlap is a strict inequality on
exactly evaluated quantities.

### What this does NOT close, stated so it is not assumed

- **R5.1.9's CONTAINMENT relation.** "One region lies within another's rings"
  is a property of the union's topology — a courtyard's hole — and does not
  reduce piecewise: a convex piece can be covered jointly by several pieces
  without being covered by any one of them. It still consults union rings via
  `ring_contains_ring`. Those are exact predicates over inexact inputs, which
  is a weaker guarantee than the overlap half would then have. Whether
  containment must also be closed exactly is an open scope question and needs
  boolean boundary semantics agreed with issue 0058.
- **A conflict with N-1 that the predicate makes visible.** An exact predicate
  over STORED coordinates reports a 0.4 µm overlap as positive area and links.
  N-1 says sub-quantum geometry is topologically absent, and the arrangement
  welds such an overlap into contact. So the exact predicate is strictly MORE
  sensitive than the topology it feeds. Either R5.1.9 is evaluated on stored
  geometry and links pairs the arrangement treats as touching, or it is
  evaluated on the welded geometry and the predicate must quantize first.
  This is a specification choice, not a numerics one, and it is unresolved.

### Prototype validation of the proposed predicate — 2026-09-12

Built outside the repository and not committed, so nothing below is a claim
about shipped code. It tests the ALGORITHM's logic; the projections are
evaluated in `f64`, so it does not test the exactness the real implementation
would add.

22 cases, all correct, and agreeing with the union route everywhere that route
can decide:

- The quantum family — overlap at 10× and 0.4× the quantum, exact touch, gap at
  0.4× and 10× — all five correct.
- Structural: disjoint, point touch, shared full edge, shared partial edge,
  containment, T-junction touch, proper crossing, degenerate zero-width piece,
  multi-piece components, rotated squares overlapping, rotated squares touching
  at a corner.
- 1,000 pseudo-random touching pairs: zero spurious overlaps.

**It is strictly more decisive than the union route.** On a proper crossing and
on a two-piece component with one overlapping pair, `positive_area` returns
`Uncertain` because the union constructs its crossing points, while the
piecewise predicate returns a definite `Yes` — it never looks at a constructed
coordinate, so there is nothing to be uncertain about.

**The reduction survived being attacked.** An L of two pieces with a rectangle
filling the notch touches both arms and overlaps neither; a filler nudged to
bite one arm overlaps; a strip straddling the seam between two abutting pieces
overlaps; a zero-width strip lying exactly on that seam does not. All four
correct.

**A harness error worth recording, because the implementation must avoid it.**
The first run reported the L-notch case as a positive overlap. That was the
harness, not `locality`: it summed the components' raw rings, which
double-counts a component's OWN internal self-overlap — the two arms of an L
share their corner square. `locality` is right to union each component first
and compare `areas[a] + areas[b]` against the union of both. Any replacement
predicate must preserve that: the question is overlap BETWEEN components, and a
component overlapping itself is not evidence of it. The piecewise predicate
gets this for free, since it only ever tests a piece of A against a piece of B.

### Measured: exact area and display rounding — 2026-09-12

First timings for the N-4 work. Apple M4 Max (16 cores, 48 GiB), macOS 26.6.2,
rustc 1.98.0, criterion, `--sample-size 20 --warm-up-time 0.5
--measurement-time 1`.

**Host contention is stated, not asserted away.** No other agent workload was
running — the slot was held exclusively — but this machine carries a permanent
MDM baseline (`kandji-daemon` around 100% of one core, `XprotectService`
around 40%) that cannot be waited out. One-minute load average was 5.40 before
the run and 6.33 after. These are contended observations on a machine whose
floor is not idle, not measurements on a quiet host, and the ratios below are
the load-bearing part rather than the absolute figures.

| Benchmark | Median |
|---|---|
| `display/filtered` | 10.26 ns |
| `display/on-a-tie` | 1.029 µs |
| `ring/4-vertices` exact | 598.7 ns |
| `ring/4-vertices` float | 2.39 ns |
| `ring/64-vertices` exact | 10.34 µs |
| `ring/64-vertices` float | 31.45 ns |
| `ring/512-vertices` exact | 79.61 µs |
| `ring/512-vertices` float | 297.8 ns |
| `positive-area/touching` | 2.12 µs |
| `positive-area/overlapping` | 346.3 ns |

**The filter earns its place, which is the point of measuring it.** A display
value that the bound certifies costs 10.26 ns and allocates nothing; one that
lands on a tie and escalates costs 1.029 µs. That is about a hundredfold
difference paid only by inputs that genuinely sit on a rounding tie — which is
the case R5.3.2 puts in its fixtures and which ordinary room geometry does not
reach.

Exact ring area runs 250–330× the float shoelace across three ring sizes, so
the escalation is expensive in the way arbitrary-precision arithmetic always
is. That is the cost the filter exists to avoid, and it is why
[`ExactArea`] is documented as eager and heap-allocating rather than offered
as a general replacement for `Polygon::area_mm2`.

**`positive-area/overlapping` is FASTER than `touching`** — 346 ns against
2.12 µs — and the asymmetry is not an anomaly. An overlapping pair hits a
constructed vertex during the scan and returns `Uncertain` before any exact
arithmetic happens; a touching pair finds no constructed vertex and therefore
has to evaluate both sides exactly. Those two figures measure the CURRENT
implementation and would be superseded if the piecewise predicate above
replaces it.

### Re-homed from issue 0060, whose owner was dropped — 2026-09-12

`herdr agent send` silently truncates past roughly 1,700–2,000 characters,
returning success and delivering a fragment. I verified against the issue-0060
owner's transcript that only the TAIL of my F27/F39 message reached them: the
closing paragraph arrived and everything before it did not, so they never
received the correction or the one thing it asked them to change. By the time
this was discovered their agent had been dropped, so the finding is recorded
here rather than sent.

**The item.** `crates/domowik-geometry/src/rooms.rs` carries
`f27_courtyard_hole_is_chamfered_as_the_f26_patch_rule_requires` — renamed from
`f27_courtyard_area_follows_f26_patch_geometry_not_f27_stated_rectangle`
during the 2026-09-13 integration, when F27's correction reached the
specification and the test stopped pinning a divergence. It was written while
F27 still stated a four-vertex hole, to pin the divergence between the
specification and `walls::plan`.

**It is not a build break, and that is established rather than assumed.** The
test already asserts `38_220_000.0` and `"38.22"`, which is exactly what the
corrected F27 states, so it passes now and keeps passing. What becomes false
when this branch lands is its NAME and its assertion message, which reads
"F27 states 38,200,000; the 20,000 difference is four 5,000 mm² …". After the
correction F27 states 38,220,000 and there is no divergence to pin.

**What it needs:** a rename dropping the "not F27 stated rectangle" framing, an
assertion message that describes conformance rather than divergence, and
ideally the three F39 values (38,220,000 / 37,260,000 / 42,120,000) alongside.

**Why it is not done here.** `rooms.rs` was issue 0060's file; its owner is
gone and the file is on main, which is frozen. It needs an owner from root
rather than a unilateral edit by the issue that invalidated the comment.

### Route corrected twice more before implementation — 2026-09-12

ED-0103 clarification (c) records the decisions; both corrections were
confirmed by construction before being accepted, and both were defects rather
than clarifications.

**A vertex-only witness is incomplete.** Square `[0,2]²` against the inscribed
diamond `(1,0),(2,1),(1,2),(0,1)`: no proper crossing, no vertex of either
strictly inside the other — all four diamond vertices lie ON the square's
boundary — and no collinear overlap, yet the diamond has area 2 inside the
square. Exact symbolic edge-interior witnesses repair it; every diamond edge
midpoint is strictly inside.

**A first off-boundary probe is not enough either.** Vertex-on-edge contacts
partition an edge into inside and outside spans with no proper crossing:
probe edge `(0,0)–(10,0)` against `(2,0),(5,3),(8,0),(8,-3),(2,-3)` classifies
outside, INSIDE, outside, so a probe at `x=1` or `x=9` certifies the wrong
answer. The route partitions at existing opposing vertices and tests the
midpoint of every open interval.

**Containment and overlap are different contracts**, pinned on F39: ring
enclosure links the island, occupied-set membership does not, and R5.1.9
mandates the link.
