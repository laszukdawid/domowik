# Issue 0007 Evidence: Clipping and Planar Room Topology Spike

Status: **evidence for tracker issue 0007 only.** Nothing here ratifies
NUMERICAL-MODEL.md, changes normative behavior, or sets any pass/fail
threshold; tracker 0030 defines and judges all Phase 0 criteria.
All technology choices remain Proposed.

Revision 2 (2026-08-24, ED-0011 disposition): all candidate
face-extraction (CandFaces) claims below are WITHDRAWN as conformance,
determinism, or performance evidence and retained only as measured
characterization of an underdetermined input contract (see §12). The
boolean (B-1x/B-1q) and oracle sections are restated from fresh runs at
the current commit after generic pinch-contact normalization. Issue 0007
remains In Progress (partial scope) with a deferred-closure gate at
tracker 0030.

## 1. Scope and exclusions

- Evaluated separately (per the issue contract): polygon boolean
  strategies (B-1x exact-event, B-1q quantization baseline) against an
  independent exact-decision oracle (NUMERICAL-MODEL §6), with planar
  face extraction (CandFaces) now CHARACTERIZATION-ONLY per §12 and
  ED-0011: its flat segment input cannot express occupancy provenance,
  so no face conformance/determinism/performance claim is made.
- Excluded: CLI/document-model integration, production API surface,
  Clipper2/B-2 adoption (remains CITE-VERIFIED-NOT-CONSULTED), any
  performance threshold, tracker/spec edits, reliance on issue-0006
  outputs. No CandFaces code change exists in this revision; its
  baseline behavior is documented unchanged from the recorded
  comparison base 126fa59 (bf17a3b is the historical prior integration
  of this issue's first evidence pass and is not this branch's start).
  Current integration validates against main d900326 (ED-0011 landed at
  4021981).
- Candidate-face domain narrowing (deliberate, documented in source)
  remains recorded for history: axis-aligned integral coordinates
  |v| <= 2^23 mm. This narrowing does NOT restore conformance claims;
  see §12 for why the flat input contract is underdetermined regardless
  of domain.

## 2. Strategies evaluated

| ID | Description | Status in this issue |
|---|---|---|
| B-1x | Column-sweep boolean over exact rational event arithmetic; signed winding accumulation (membership = winding != 0); single terminal binary64 commitment per output vertex. Revision 2: pinch-contact cycle pairing repaired generically (fixed-successor orbit decomposition + repeated-node walk splitting + exact simple-ring postcondition before terminal commitment); metrics restated from fresh runs in §5 | Candidate ARCHITECTURE/PATH evaluated; revision-2 rerun reports 0/12 topology mismatch on the named corpus (§5); selected-strategy decision remains with 0030 |
| B-1q | Same skeleton with RATIONALLY-ROUNDED pinned-grid quantization: scale 64 units/mm (pitch 2^-6 mm), ties-to-even, unit range gate <= 6.4e10 units with deterministic `grid-range-overflow` reject, no silent rescaling. SCOPE CORRECTION (F18): the originally described int64 storage + math/bits.Mul64 128-bit orientation backend WAS NOT PERFORMED; all B-1q decisions run in big.Rat over grid-aligned dyadic values. Revision 2: shares the repaired pinch-contact traversal; rerun metrics in §5 | Deliberately NONCONFORMING evidence-only baseline for AC1 risk measurement; does NOT satisfy NUMERICAL-MODEL §7 runtime certify-or-escalate — that behavior is UNIMPLEMENTED and its absence is SELECTION-BLOCKING for any quantized strategy; int64/128-bit study not performed; NOT eligible as selected strategy |
| CandFaces | Float-domain arrangement walker + angular chaining over a flat segment soup. WITHDRAWN under ED-0011: flat input cannot express occupancy provenance (nested filled island vs hole; seam vs retraced boundary), and the implementation is direction-sensitive under non-uniform input permutation (§12). NO conformance, determinism, or performance evidence is claimed; retained as measured characterization only. No code change in this revision | Characterization only; NOT candidate-conformance evidence; AC4 face-extraction claims are INVALIDATED and deferred to tracker 0030's structured-input decision or rejection |
| Oracle | Brute-force subdivision + sided-probe booleans; vertical-slab/trapezoid face decomposition with explicit far-field seed; exact fields Q and Q(sqrt(d)). Revision 2: BooleanUnion output is normalized POST-DECISION into simple canonical rings (repeated-node walk splitting + exact simple-ring validation) so closed point-contacts decompose per R3.2.4/R5.1.2; raw pre-normalization repeated-node cycles are recorded in §5. Reference machinery only — never candidate-conformance evidence | Independent ground truth for boolean topology; shares no decision code with candidates |

B-1x and B-1q share one candidate codebase with different arithmetic
policies (disclosed; not independent implementations). Oracle vs
candidate independence is by separate traversal and decision machinery;
residual shared-family caveat: same literature domain and shared
primitive big.Int/big.Rat arithmetic.

## 3. Policies defined (AC2)

- Fill/winding rule: NONZERO winding with orientation-aware deltas for
  all boolean membership; even-odd exists only as a labeled comparison
  probe in tests (contrast asserted).
- Self-intersection policy: ValidateSimpleRing rejects degenerate
  rings (<3 vertices), zero-length edges/duplicate adjacent vertices,
  zero-area rings (exact dyadic shoelace), proper self-crossings, and
  non-adjacent touching incidences, deterministically. These are
  spike-local sentinel codes, not ratified diagnostics.
- Invalid input: non-finite (`nonfinite-coordinate`), out-of-range
  beyond proposed B=10^9 mm (`coordinate-out-of-range`), malformed
  polySet shape (holes length != outer length, empty rings:
  `malformed-polyset-shape`, `degenerate-ring`), zero-length edges and
  duplicate adjacent vertices (`zero-length-edge`), pinned-grid
  overflow (`grid-range-overflow`) all reject before any geometry
  runs. No valid-input path panics. SCOPE LIMITATION: within
  ValidateSimpleRing only the area gate is exact dyadic; the
  self-crossing/touching incidence checks are FLOAT PREFILTERS that
  may misclassify near-degenerate configurations — they are an input
  screen, not an exactness claim.
- Holes/multipolygons: outer CCW, holes CW, holes attached to smallest
  containing outer via certified interior witness; components ordered
  canonically (R3.2.4 tuple order).
- Normalization: duplicate/collinear vertex elimination under exact
  tests, lexicographic least-vertex rotation, canonical component and
  hole ordering, reversal invariance asserted corpus-wide.
- Field policy: every oracle value carries its field tag (Q or
  Q(sqrt(d)), d>1 squarefree enforced by `Field`); mixed-field or
  mixed-coordinate operations panic deterministically and are COUNTED
  when routed through GuardMixed / ptKey (counter
  MixedFieldEvents; asserted zero across corpus).
- Coincident fragment invariant (CandFaces): WITHDRAWN as an exact
  invariant claim (§12): the at-most-two/opposed-pair-cancels rule was
  direction-sensitive and its cancellation is part of the recorded
  defect. Retained as characterization of the flat-input behavior.
- Probe epsilon policy: WITHDRAWN for CandFaces together with the rest
  of the face pipeline (§12). The halving-retry/certified-witness
  machinery description remains accurate for OracleFaces probes and
  certified interior/free-side witnesses only.

## 4. Named corpus (AC3)

single-room; disjoint-rooms; thick-L-join; unequal-thickness-T;
X-crossing; shared-edge-touching-bands; touching-vertices-pinch-corner;
sliver-overlap (1000:1 aspect overlap); courtyard-island (nested,
holed); point-pinch (F35-class, two lobes at one occupied point);
large-translation (4e6 mm); near-B-union (B=1e9, boolean/oracle only —
outside candidate-face domain); opening-difference suite (full cut
disconnect, partial notch, order invariance, join-patch-never-
subtracted); normative F35 exact-coordinate fixture; Q(sqrt(2)) surd
45-degree L-join and point-pinch through OracleFaces end-to-end;
non-grid quantization probe (0.03/2.03 class).

Independent expectations (F11 scope): analytically-derived literal
canonical rings are asserted for single-room, courtyard-island, and
large-translation against the boolean engines and the oracle. WITHDRAWN
(ED-0011): prior revisions claimed the same per-case expectations for
CandFaces face output; that claim is invalidated by §12 and no
candidate-face expectation is relied on. Reversal invariance
(R3.3.6/R4.2.3) is asserted for boolean canonical output under whole-
ring reversal, ring-start rotation, and operand order permutation
(TestBooleanCanonicalInvariances,
TestCandBooleanInputPermutationInvariance — union, intersection, and
difference, both policies, seeds 1..32). Identity-path normalization
(holes CW/rotation, hole order, component order) is implemented and
tested (TestIdentityNormalizesHolesAndComponents, both policies).
The former uniform segment-reversal face test remains in the tree as
legacy coverage only; it does NOT establish input-order determinism
for CandFaces (see §12).

## 5. Discrete results (AC5 topology mismatch)

Harness: TestCorpusBooleanVsOracle (booleans: per-policy component/hole
structure vs exact BooleanUnion over the same stored coordinates, with
operands split even/odd on both sides), TestBooleanPinchSemantics
(literal oracle-derived pinch fixtures), and
TestCandBooleanInputPermutationInvariance. All green at the current
commit; failures ARE the mismatch record (zero tolerance for discrete
outcomes).

WITHDRAWN (ED-0011): the prior revision's claim that CandFaces face
extraction "matched with zero mismatches on all in-domain cases" is
invalidated — see §12 for the measured permutation defect and input
underdetermination. No candidate-face topology result is claimed in
this revision.

Oracle BooleanUnion normalization characterization (post-decision).
Raw chain outputs are now pinned by the committed test
TestOracleBooleanRawPinchCycles (via rawBoundaryCycles, the real
decision machinery without normalization), measured at base 126fa59 /
d900326 and unchanged by this revision's refactor:

- touching-vertices-pinch-corner: RAW oracle emits ONE repeated-node
  8-vertex walk visiting (6,6) twice (self-touching figure-eight
  outer). The old candidate emitted two simple 4-vertex outers.
- point-pinch (committed harness operand split, even/odd rects): the
  RAW oracle chain does NOT merge — it emits one 8-vertex CCW outer
  walk (frame perimeter including band-junction collinear vertices)
  plus two separate 6-vertex CW lobe walks. The OLD CANDIDATE emitted
  ONE self-touching 12-vertex hole visiting (18,18) twice; that
  candidate-side observation is pinned to the historical session-start
  probe at base 126fa59 (identical operand construction:
  rectsToOraclePolySet even/odd). No 12-vertex self-touching ORACLE
  hole exists under this configuration; any finding claiming one must
  cite its exact operand split/revision so it can be recorded as an
  additional characterization.

Post-decision canonicalization (splitRepeatedWalk + oRingSimple) makes
both engines emit R3.2.4 simple rings: touching-corner becomes exactly
two 4-vertex outers [[0,0],[6,0],[6,6],[0,6]] / [[6,6],[12,6],[12,12],
[6,12]]; point-pinch remains exactly two 6-vertex lobes [(2,2),(2,28),
(18,28),(18,18),(10,18),(10,2)] and [(18,2),(18,18),(26,18),(26,28),
(28,28),(28,2)], asserted exactly against candidate output in
TestBooleanPinchSemantics.

Independence qualification for the 0/12 metric below: decision and
traversal machinery remain independent between candidate and oracle,
but both apply analogous deterministic split/validate normalization,
so the 0/12 agreement is POST-NORMALIZATION AGREEMENT, not a wholly
independent end-to-end pinch proof; the literal normative pinch rings
anchored in TestBooleanPinchSemantics are the semantic anchor, and raw
repeated-node characterization is disclosed above. Normalization is
post-decision canonicalization to simple-ring form, NOT an oracle
decision substitute. OracleFaces remains independent reference
machinery only.

Measured boolean-vs-oracle status from fresh runs at tested source
commit f0dad76 (selected per-case log lines plus aggregates, reproduced
exactly
as emitted; metric = EXACT squared Euclidean distance via big.Rat
between the committed dyadic vertex and the exact normalized oracle
vertex; denominator = all 12 named boolean cases including near-B;
no case excluded; see independence qualification above):

```
    corpus_test.go:506: B-1x/single-room: MATCH comps=1 maxSquaredDist=0
    corpus_test.go:506: B-1q/single-room: MATCH comps=1 maxSquaredDist=0
    corpus_test.go:506: B-1x/disjoint-rooms: MATCH comps=2 maxSquaredDist=0
    corpus_test.go:506: B-1q/disjoint-rooms: MATCH comps=2 maxSquaredDist=0
    corpus_test.go:506: B-1x/thick-L-join: MATCH comps=1 maxSquaredDist=0
    corpus_test.go:506: B-1q/thick-L-join: MATCH comps=1 maxSquaredDist=0
    corpus_test.go:506: B-1x/unequal-thickness-T: MATCH comps=1 maxSquaredDist=0
    corpus_test.go:506: B-1q/unequal-thickness-T: MATCH comps=1 maxSquaredDist=0
    corpus_test.go:506: B-1x/X-crossing: MATCH comps=1 maxSquaredDist=0
    corpus_test.go:506: B-1q/X-crossing: MATCH comps=1 maxSquaredDist=0
    corpus_test.go:506: B-1x/shared-edge-touching-bands: MATCH comps=1 maxSquaredDist=0
    corpus_test.go:506: B-1q/shared-edge-touching-bands: MATCH comps=1 maxSquaredDist=0
    corpus_test.go:506: B-1x/touching-vertices-pinch-corner: MATCH comps=2 maxSquaredDist=0
    corpus_test.go:506: B-1q/touching-vertices-pinch-corner: MATCH comps=2 maxSquaredDist=0
    corpus_test.go:506: B-1x/sliver-overlap: MATCH comps=1 maxSquaredDist=0
    corpus_test.go:506: B-1q/sliver-overlap: MATCH comps=1 maxSquaredDist=0
    corpus_test.go:506: B-1x/courtyard-island: MATCH comps=2 maxSquaredDist=0
    corpus_test.go:506: B-1q/courtyard-island: MATCH comps=2 maxSquaredDist=0
    corpus_test.go:506: B-1x/point-pinch: MATCH comps=1 maxSquaredDist=0
    corpus_test.go:506: B-1q/point-pinch: MATCH comps=1 maxSquaredDist=0
    corpus_test.go:506: B-1x/large-translation: MATCH comps=1 maxSquaredDist=0
    corpus_test.go:506: B-1q/large-translation: MATCH comps=1 maxSquaredDist=0
    corpus_test.go:506: B-1x/near-B-translation: MATCH comps=1 maxSquaredDist=0
    corpus_test.go:506: B-1q/near-B-translation: MATCH comps=1 maxSquaredDist=0
    corpus_test.go:516: B-1x: cases=12 topologyMismatches=0 mismatchRate=0.000 matchedVertices=122 maxSquaredEuclideanDist=0
    corpus_test.go:516: B-1q: cases=12 topologyMismatches=0 mismatchRate=0.000 matchedVertices=122 maxSquaredEuclideanDist=0
```

Revision history of this metric: the integrated prior revision measured
2/12 mismatches (both policies) on touching-vertices-pinch-corner and
point-pinch. That measurement paired the UNNORMALIZED candidate against
the UNNORMALIZED oracle and is superseded by the rerun above after
generic normalization on both sides; the historical block is retained
in revision-control history only and is not valid acceptance evidence
for either engine.

## 6. Continuous results (AC1/AC5 coordinate error)

Separate from discrete outcomes; measured, no threshold.

Boolean metric (TestCorpusBooleanVsOracle): maxSquaredEuclideanDist=0
(exact big.Rat) for both policies over integer-corpus matched vertices
(122 vertices, 12 cases incl. near-B, post-normalization rerun) —
expected: all corpus coordinates are integers on the 1/64 grid, so
commitment and rounding are exact. The prior revision's 98-vertex
count reflects the superseded pre-normalization pairing and is
withdrawn with the 2/12 block.

Quantization probe (TestQuantizationProbeNonGrid): non-grid stored
coordinates of the 0.03/2.03 class, matched-ring correspondence against
exact BooleanUnion of the SAME stored floats; metric is EXACT squared
Euclidean distance in big.Rat (renegotiated from per-axis per review
F10). Verbatim log:

```
    corpus_test.go:627: B-1x: matched ring, measured max squared Euclidean deviation 0 (mm^2)
    corpus_test.go:627: B-1q: matched ring, measured max squared Euclidean deviation 25749152817139737777585049/5070602400912917605986812821504 (mm^2)
    --- PASS: TestQuantizationProbeNonGrid (0.00s)
```

Approximation note: the B-1q fraction is approximately 5.08e-6 mm^2;
its square root is approximately 0.00225 mm — clearly-labeled
approximation only; the recorded evidence value is the exact fraction
above.

Interpretation for 0030: B-1x coordinate error is bounded by the single
terminal 1/2-ulp commitment; B-1q adds deterministic grid perturbation
up to the pinned pitch. Neither number is a pass threshold.

## 7. Wall-clock and allocation measurements (AC6)

Reference environment (identical for correctness and benchmark runs;
verified read-only): AMD Ryzen 9 7950X 16-Core Processor (16 cores /
32 threads), 61 GiB RAM, Linux x86_64 kernel 7.1.8-arch1-3 (Arch
Linux), go1.27.0 linux/amd64 with GOTOOLCHAIN=local,
CGO_ENABLED=1 (go test default; no other build flags), module
domowik.dev/domowik.

Exact command (boolean pipelines only):

```
go test ./internal/geometry/ -run '^$' -bench '^BenchmarkCorpusBoolean' -benchmem -count=1
```

Verbatim output (fresh run at tested source commit f0dad76 after
pinch-contact
normalization; benchmarks count deterministic rejections via rejects/op
instead of aborting):

```
goos: linux
goarch: amd64
pkg: domowik.dev/domowik/internal/geometry
cpu: AMD Ryzen 9 7950X 16-Core Processor
BenchmarkCorpusBooleanB1X-32    	     116	   9832985 ns/op	         0 rejects/op	14503050 B/op	  492402 allocs/op
BenchmarkCorpusBooleanB1Q-32    	     100	  11988893 ns/op	         0 rejects/op	15840996 B/op	  545187 allocs/op
PASS
ok  	domowik.dev/domowik/internal/geometry	3.222s
```

WITHDRAWN (ED-0011): the prior revision's BenchmarkCorpusCandFaces /
BenchmarkCorpusOracleFaces wall-clock and allocation figures are NOT
relied on as candidate-face performance evidence; they are not
recaptured because CandFaces claims are characterization-only. The
prior boolean benchmark block is superseded by the capture above (the
repaired traversal changes the measured pipeline). COMPARABILITY
CAVEAT retained from the prior revision: the boolean benchmarks build
operands inside the timed loop; their numbers must not be read as
directly comparable to any face-pipeline number.

## 8. H-register disposition (H-0..H-6)

| H | Disposition this spike |
|---|---|
| H-0 sign-evaluation cost | Q-path: exact rational signs throughout oracle; Q(sqrt(d)) path exercised end-to-end (surd L-join/pinch) with mixed-field events counted (zero on corpus). Cost not budgeted — open for 0030. |
| H-1 intersection chain | B-1x events exact rational; B-1q event ordinates rounded to pinned grid once then integer-stable. No derivation-backed error bound beyond commitment — open. |
| H-2 hull evaluation | Join patches represented as rect proxies in corpus; true convex-hull stage NOT implemented. Open; flagged for 0013. |
| H-3 boolean vertex generation | B-1x: exact until single terminal commitment (measured 0 error on non-grid probe). B-1q: grid-rounded (measured above). Partially evidenced; multi-stage accumulation unexercised. |
| H-4 face assembly | WITHDRAWN for CandFaces (§12): no candidate-face assembly claim is made. Face-level evidence is deferred to tracker 0030's structured-input decision or rejection. |
| H-5 area-chain propagation | Out of scope (measurement/display owned elsewhere); untouched. Open. |
| H-6 fallback/escalation counts | ExactFallbackCount is a CandFaces counter and is NOT relied on as evidence (§12); ProbeEscalations/ProbeBudgetRejects are OracleFaces reference-machinery counters only; benchmark rejects/op (boolean) = 0 on captured runs. Counts that feed 0030's judgment are the boolean rejects/op figures above; runtime certification counts remain unimplemented pending any quantized-strategy decision. |

No uncertified discrete outcome feeds an observable without the
escalation/rejection routing described. This sentence describes the
routing requirement only: B-1q runtime certify-or-escalate is
UNIMPLEMENTED in this revision — B-1q is evidence-only, NONCONFORMING,
and SELECTION-BLOCKING for any quantized strategy until a real
per-invocation certification path with `geometry-indeterminate` budget
behavior exists (NUMERICAL-MODEL §7). Measured rates cannot waive
this.

## 9. Provenance ledger additions

Governing bibliographic rows live in NUMERICAL-MODEL.md §10 and are
cross-referenced here, not restated in shortened form. This spike adds
only the per-artifact consultation/adaptation records required by the
ledger-before-reliance rule:

| Governing row (NUMERICAL-MODEL §10) | Consulted here? | Adaptation type recorded |
|---|---|---|
| B-1c: de Berg, Cheong, van Kreveld, Overmars, *Computational Geometry: Algorithms and Applications*, 3rd ed., Springer (2008) — edition/publisher verified there | Citation-level use only (chapter-2 sweep/DCEL descriptions); no artifact pages consulted | Independent implementation from described behavior; no identifiers or structure copied |
| B-1a: Martínez, Rueda, Feito, *A new algorithm for computing Boolean operations on polygons*, Computers & Geosciences 35(6):1177–1185 (2009), DOI 10.1016/j.cageo.2008.08.009 — citation verified at publisher DOI there | NO — title/citation only; the paper text and authors' C++ artifact remain unconsulted | None; strategy framing citation only |
| B-1b: Guibas, Marimont, *Rounding arrangements dynamically*, SCG '95 pp. 190–199, DOI 10.1145/220279.220300; extended IJCGA (1998), DOI 10.1142/S0218195998000096 — DOIs verified there | NO — background framing for the B-1q baseline only | None |
| P-1b: Shewchuk, *Adaptive Precision Floating-Point Arithmetic and Fast Robust Geometric Predicates*, DCG 18:305–363 (1997); CMU-CS-96-140(R); https://people.eecs.berkeley.edu/~jrs/papers/robustr.pdf — URL/front-matter duplication notice recorded there. The PUBLICATION is freely duplicable WITH NOTICE; it is not itself "licensed" in the code sense, and the public-domain statement attaches to P-1c's predicates.c ONLY | Pattern consulted at the level of the published technique description; predicates.c (P-1c) NOT downloaded or viewed | Filtered-determinant-with-exact-fallback pattern independently re-implemented for CandFaces; WITHDRAWN as conformance-relevant under ED-0011 — the CandFaces code remains in the tree unchanged and carries no relied-upon evidence claim (§12). No source translated |
| L-1: Go stdlib math/big, math/bits, sync; go1.27.0 reference toolchain — BSD-3-Clause, Copyright (c) 2009 The Go Authors, go.dev/LICENSE | Used unmodified | NONE (stdlib packages as-is) |

Revision-2 boolean/oracle code provenance: the fixed-successor orbit
decomposition, repeated-node walk splitting, and exact simple-ring
validators in sweep.go/oraclebool.go are INDEPENDENT IMPLEMENTATIONS
by this project from first principles, tied to normative R3.2.4/R5.1.2
semantics and the citation-only sweep/planar-subdivision background
already recorded above (B-1a/B-1b/B-1c remain unconsulted at artifact
level). No new artifact was consulted, downloaded, or adapted; no
source, test, or data file was translated; no GPL artifact was
inspected or copied.

No other artifacts were consulted. No GPL source (CGAL, Sweet Home
3D, DobryDom3D, or any floor-plan application) was inspected,
consulted, or copied. Abstract algorithms carry no license claims.
Test vectors are constructed for this spike from BEHAVIOR-SPEC
fixtures (F35 coordinates taken from the reviewed normative text); no
third-party vectors were copied. Tracker issue-0006 outputs (code,
design, results) were NOT consulted and no reliance is claimed.

Statements:

- No GPL source (CGAL, Sweet Home 3D, DobryDom3D, or any floor-plan
  application) was inspected, consulted, or copied for any part of this
  work.
- Abstract algorithms carry no license claims; only concrete artifacts
  have license rows above.
- Test vectors are constructed for this spike from BEHAVIOR-SPEC
  fixtures (F35 coordinates taken from the reviewed normative text);
  no third-party vectors were copied.
- Tracker issue-0006 outputs (code/design/results) were NOT consulted;
  the filtered-predicate pattern in CandFaces is independently
  implemented from the P-1b publication.

## 10. Acceptance criteria mapping (0007)

| AC | Evidence |
|---|---|
| Quantization scale/range/overflow/max-error measured IF integer clipping used | B-1q pinned constants (§2), overflow sentinel + ties-to-even unit tests, non-grid probe measurements as exact squared distances (§6). SCOPE CORRECTION: rationally-rounded big.Rat baseline; int64/Mul64 backend not performed (§2) |
| Fill/winding, self-intersection policy, holes, normalization, invalid input defined | §3 policies + validation/normalization tests |
| Corpus coverage (L/T/X joins, unequal thickness, shared edges, touching vertices, slivers, opening difference, nested faces, large translations) | §4 named corpus + dedicated tests; plus pinch/near-B/surd extensions |
| Separate planar face extraction & exterior/hole classification evidence | INVALIDATED under ED-0011: CandFaces provides NO conformance evidence (§12); face-extraction evidence is deferred to 0030's structured-input decision or rejection. OracleFaces remains reference machinery only |
| Topology-mismatch rate vs independent oracle measured and reported separately from coordinate error | §5 discrete tables/logs vs §6 continuous metrics, never combined |
| Wall-clock/allocation recorded on same named corpus/environment, NO threshold | §7 verbatim benchmark output + environment; correctness harness identical inputs |
| Provenance records exact publications/licenses; no algorithm-license conflation | §9 ledger |

## 11. Disposition and deferred obligations (ED-0011)

1. Boolean strategies: B-1x revision-2 reports 0/12 topology mismatch
   on the named corpus with zero measured coordinate error (§5/§6);
   selected-strategy decision remains with tracker 0030. B-1q shares
   the repaired traversal for measurement purposes but remains
   deliberately NONCONFORMING: NUMERICAL-MODEL §7 runtime certify-or-
   escalate is UNIMPLEMENTED, and its absence is SELECTION-BLOCKING
   for any quantized strategy. Measured rates cannot waive this.
2. Candidate face extraction (CandFaces): flat-input contract is
   underdetermined (§12); all face conformance/determinism/
   performance claims are WITHDRAWN; no code change was made.
   Recommended reversible disposition: NO flat API change in this
   issue; re-scope candidate face extraction to tracker 0030 now.
   Tracker 0030 may later choose a provenance-preserving structured
   input (new spec/tests/provenance/release review) or reject the
   candidate face extractor before any implementation proceeds.
3. Multi-radicand (mixed irrational direction) constructions remain
   outside oracle coverage; single-radicand Q(sqrt(d)) demonstrated.
4. Convex-hull join patches (H-2) proxied by rectangles.
5. Probe-nudge narrow-feature rejection policy needs 0030 ratification
   as behavior (oracle-side machinery).
6. Issue 0007 closes as In Progress (partial scope) with a
   deferred-closure gate at 0030.

## 12. CandFaces characterization: flat-input underdetermination and permutation defect

No CandFaces code change exists in this revision. The following
characterization withdraws prior candidate-face claims and records the
defects for tracker 0030.

**Information-loss transformation T.** The flat input contract maps an
input record of oriented segment occurrences to a sorted set of
undirected endpoint pairs after deduplicating coincident geometry,
discarding orientation, multiplicity, and operand/ring provenance.
Any classifier that sees only T's output cannot recover the discarded
roles.

**Ambiguity P1 — nested filled island vs hole.** Let B have vertices
(0,0),(4,0),(4,4),(0,4) and I have (1,1),(3,1),(3,3),(1,3). A union
input with B and I as two CCW filled outers and a polygon input with B
CCW and I as a CW hole map under T to the same eight undirected
segments. Required canonical semantics differ: one filled outer with
no hole versus an outer carrying one hole. No direction/multiplicity-
independent classifier can recover the lost role.

**Ambiguity P2 — seam vs retraced boundary.** Two adjacent occupied
squares require their shared edge to be internal (absent from the free
boundary), while a duplicated genuine boundary occurrence requires it
to remain boundary. After undirected dedupe the edge geometry is
identical.

Consequence: signed winding over structurally reconstructed contours,
parity classifiers, multiplicity-weighted schemes, and contour-trace
heuristics each fail at least one of P1/P2 or misclassify contact
nodes; occupancy provenance must come from the INPUT, not be inferred
from the soup. Directed traversal can deterministically canonicalize
only T's post-transformation graph; it cannot recover discarded
ring/operand provenance, so any pairing choice is a representation
decision, not recovered semantics. OracleFaces is independent
reference machinery only and is NOT candidate-conformance evidence.

**Measured permutation defect (seed-1 reproduction).** At base 126fa59,
for single-room, cSubdivideAll produces 12 fragments / 12 directed
pieces / 2 cycles from canonical input ordering, but 16 fragments /
10 directed pieces / 1 cycle under non-uniform segment shuffle plus
per-segment endpoint reversal (rng seed 1). Mechanism: seam
cancellation keyed on directed fragment geometry flips opposed pairs
to same-direction duplicates under endpoint reversal; surviving seam
fragments inject phantom winding deltas into sided probes, flipping
boundary classification. Uniform whole-corpus reversal preserves
coincident-pair opposedness, which is why the legacy uniform-reversal
tests passed while arbitrary permutation failed.

**Invalidation scope.** All CandFaces face/hole/exterior topology
claims, determinism/order-invariance claims, ExactFallbackCount-based
H-6 claims, and BenchmarkCorpusCandFaces performance figures in prior
revisions of this document are WITHDRAWN. TestCorpusTopologyMatchesOracle
and TestCandFaces* remain in the tree as regression coverage of the
unchanged baseline on canonical-direction inputs only; they confer no
conformance claim.
