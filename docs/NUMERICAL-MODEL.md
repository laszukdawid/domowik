# Numerical Geometry Model

Status: **Partly ratified.** This document began as the issue-0022 proposal
required by R3.0.8/R4.0.7 of [BEHAVIOR-SPEC.md](BEHAVIOR-SPEC.md). Three things
have happened to it since. §3's coordinate domain is RATIFIED (ED-0100).
§§4.2–4.5, 6, 10 and 11 are SUPERSEDED — they describe candidate machinery for
a Go core that was never built, and nothing in them is normative. §12 is
RATIFIED numeric policy (ED-0103), and it names two obligations the shipped
implementation still owes against BEHAVIOR-SPEC rather than ratifying what the
code does today. Read §§3, 12, 13 and 14 for the model in force; read the rest
as the record of how it was chosen.

The behavior specification is written in exact real arithmetic; its fixtures
are semantic oracles stating exact expected results without mandating
bit-identical implementation output (R3.0.8). This document defines the
candidate model that implementations must satisfy to conform, the guarantees
it can already derive, and the obligations it hands to the spike issues.

## 1. Model comparison and selection

| Family | Strengths | Weaknesses for Domowik |
|---|---|---|
| **A. Float64 inputs + adaptive exact predicates** | Matches exact-real fixture semantics; no quantization perturbation of rational fixtures such as F23a's `(900/41, 4000/41)`; pure Go feasible | Predicates are sign tests only; constructions and booleans need separate treatment |
| **B. Integer quantization / snap rounding throughout** | Combinatorially robust booleans on a grid | Perturbs authoritative geometry; rational fixtures unreachable except within an error budget; conflicts with authoritative endpoints (R3.3.4) unless documents themselves are quantized — a product-semantics change |
| **C. Hybrid (candidate)** | Float64 authoritative coordinates; exact predicates for every sign decision; integer-grid evaluation admissible only inside clipping as internal derived-data steps with separately measured coordinate-error and topology-mismatch rates | Two representations to reconcile — specified by the conformance protocol in section 4 |

Candidate selected for spiking: **C**. Family B cannot honor fixture oracles
without redefining product semantics; family A alone leaves boolean/topology
robustness unevidenced, which is precisely what issue 0007 must prove.

### 1.1 Construction classes

- **Class C1 — rational of dyadic inputs.** Segment-intersection parameters
  and axis split points are rational functions of stored binary64 values.
  They are computed exactly (scaled big.Int) and then committed under the
  DEFAULT candidate representation: nearest binary64, commitment error ≤ ½
  ulp of magnitude per coordinate. The exact-rational-persistence alternative
  carried open in §12 is strictly stronger — every guarantee that relies on
  commitment error (§4.4 term 1, §5's ε_A) collapses to zero and holds a
  fortiori. Statements below are made UNDER THE DEFAULT REPRESENTATION
  unless stated otherwise.
- **Class C2 — irrational-bearing.** Perpendicular offsets (R3.0.2, R3.2.2)
  divide by √(dx²+dy²) and are generally irrational; patch hulls (R3.2.3),
  boolean outputs, and face rings inherit such vertices. F23a's outline
  vertex `(900/41, 4000/41)` is exactly rational only because direction
  `(40/41, −9/41)` forms a 9-40-41 Pythagorean triple whose unit normal is
  exactly rational; general directions do not.

Exactness guarantees attach to SIGN DECISIONS OVER REPRESENTED VALUES:
adaptive predicates return the exact sign of expressions over the coordinates
actually fed to them. Fidelity of represented values to ideal real geometry
is a bounded-error property governed by sections 3–5, measured on spike
corpora, and ratified at 0030.

## 2. Required operations

Enumerated from the normative rules (issue-0022 acceptance criterion 1):

| # | Operation | Rules | Numerical demand |
|---|---|---|---|
| 1 | Segment intersection: existence, class, parameter extraction | R3.1.2, R3.1.3 | Exact orientation/collinearity signs; parameters as rationals of stored coordinates |
| 2 | Splitting walls at ordered parameter sets | R3.1.3, R3.1.4 | Monotone reversal-invariant ordering; deterministic chain order |
| 3 | Offsets: centered butt-capped rectangles | R3.0.2, R3.2.2 | Unit-normal construction (class C2); closed-set semantics |
| 4 | Convex hulls of cross-sections (join patches) | R3.2.3 | Exact hull decisions over represented vertices; collinear degeneracy as closed set |
| 5 | Polygon union → occupied region O_L | R3.2.1 | Multipolygon output; shared-edge/touching-vertex safety |
| 6 | Polygon intersection (positive-area overlap decisions) | R3.1.2 `wall-area-overlap`; R5.1.9 linking | Exact positive-area vs zero-area discrimination; ring containment |
| 7 | Polygon difference (opening cuts) | R4.2.2, R4.0.6, R4.2.5 | Per-wall CLOSED rectangle MINUS union of OPEN aperture-interior footprints BEFORE union; join patches never subtracted; order-independent; rooms unaffected |
| 8 | Rings/holes canonical normalization | R3.2.4, R4.2.3, R5.1.3 | Duplicate/collinear elimination under exact tests; CCW exteriors/CW holes; lexicographic rotation; total component order |
| 9 | Planar faces of F_L = ℝ²∖O_L | R5.1.1–R5.1.4 | Arrangement/traversal beyond booleans; point-pinch splitting (F35); exterior exclusion; unbounded nesting |
| 10 | Point containment | R5.2.3, R5.2.4 | Strict interior; boundary/wall/exterior seed statuses |
| 11 | Measurement and display rounding | R5.3.1, R5.3.2 | Exact committed-ring area; hole subtraction; certified tie guard (section 5) |

Supporting non-topological operations: axis-aligned bounding boxes for the
spatial index (tracker 0012); view transforms (BEHAVIOR-SPEC §1.2, already
normative).

### 2.1 Full rule-to-operation-to-fixture/owner matrix

Fixture numbering RESTARTS per behavior-spec section; entries below are
disambiguated as `§n:Fnn`. "Owner" names who ratifies or benchmarks the row's
numerical behavior; every normative rule itself remains owned by its
behavior-specification section.

| Rule(s) | Behavior | Operation #(§2) | Fixtures | Numerical owner |
|---|---|---|---|---|
| R3.0.2–R3.0.8 | Conventions, exact-real boundary | 3, 4, 11 | §3:F1–F3, F6–F8 | 0030 |
| R3.1.1–R3.1.8 | Creation, contact classes, splits, joins, atomicity | 1, 2, 5, 6 | §3:F4–F15, F14b | 0006/0007 evidence → 0030 |
| R3.2.1–R3.2.5 | Occupied region, patches, canonical normalization | 3, 4, 5, 8 | §3:F2–F8, F14b | 0007 evidence → 0030 |
| R3.3.1–R3.3.7 | Join entity, agreement, merge, reversal, detach | 1, 10 | §3:F2, F7, F8, F14–F18 | 0006 evidence → 0030 |
| R3.4.1–R3.4.5 | Editing, drags, incidental resolution, deletion | 1, 2, 5 | §3:F16–F19, F22, F23 (variants a–b) | 0006/0007 evidence → 0030 |
| R3.4.6–R3.4.7 | Host gates, undo equivalence | 7 | §3:F20–F22 | 0030 |
| **R3.4.8** | Performance locality over join-connected components | supports 5–9 (invalidation scope) | exercised via §3:F17, §5:F39; corpora owned by tracker 0032 | tracker 0032 (benchmarks); 0030 (ratification) |
| R4.0.1–R4.0.7 | Opening kind, fields, data classes, numerical boundary | 7 + §3 domain coverage | §4:F25–F27, F37 | 0030 |
| R4.1.1–R4.1.5 | Batch boundaries, strict fit, top check, overlap matrix | 7 (exact u·L ± w/2 evaluation) | §4:F25, F27–F31 | 0006 evidence (exact rational comparisons) → 0030 |
| R4.2.1–R4.2.5 | Aperture footprints, subtraction scope/order, normalization, rooms unaffected | 7, 8 | §4:F25–F26, F35–F36; §5:F36 (enclosure exclusion) | 0007 evidence → 0030 |
| R4.3.1–R4.3.2 | Reversal transform, edits keep u, complete revalidation | 7 + reversal-invariance of 8 | §4:F29–F31 | 0030 |
| R4.3.3 | Split remap-or-reject | 1, 2 (parameter comparison) | §4:F32, F32b, F33 | 0006 evidence → 0030 |
| R4.3.4–R4.3.5 | Deletion cascade (via R3.4.6), host identity immutability | 7 | §3:F20 (cascade), §4:F34, F36, F38 | 0030 |
| R4.3.6–R4.3.7 | Undo equivalence, aperture locality | mirrors 5–8 | §4:F37, F38; corpora tracker 0032 | tracker 0032; 0030 |
| R5.1.1–R5.1.6 | Free space, faces, pinch splitting, holes, degeneracy | 5, 8, 9 | §5:F25–F27, F35(§5) | 0007 evidence → 0030 |
| R5.1.7–R5.1.8 | No manual geometry, no face identities | documentation constraints | — | 0030 |
| **R5.1.9** | FULL locality closure: transitive containment AND positive-area linking over pre/post states, per-level scope | 6 (positive-area discrimination), plus invalidation driver for 5–9 | §5:F28–F31, F33, F37a–j, F39; corpora tracker 0032 | correctness: 0007 evidence → 0030; benchmark corpora: tracker 0032 |
| R5.2.1–R5.2.8 | Room records, candidacy (strict interior), statuses, tie-breaks | 10 | §5:F28–F34, F37 vectors (a)–(j) | 0006 evidence → 0030 |
| R5.3.1–R5.3.5 | Exact area, half-even display, labels, surfaces, non-authority | 11 | §5:F25–F27 (30,245,000 mm² tie in F26), F39 | this document §5 → 0030 |

Rows marked "evidence → 0030" preserve the spike issues' evidence-only roles;
tracker-0032 rows remain benchmark-owned. No row grants any issue authority
over normative semantics.

## 3. Coordinate domain and validation

- Coordinates and dimensions are finite IEEE-754 binary64 values.
  **RATIFIED (ED-0100, 2026-09-04):** |x|, |y| ≤ B = 10⁷ mm (±10 km);
  thicknessMm, heightMm, widthMm, depthMm, sillMm ∈ (0, 10⁶] (sillMm ≥ 0);
  derived vertices ≤ B + 10⁶ mm. The 10⁹ proposal and the 2³¹−1 alternative
  are both rejected.

  **Why the bound is a product decision, not a numerical one.** The argument
  that previously favoured a smaller domain was that a small polygon far from
  the origin has its area computed as a difference of huge nearly-equal
  products. That was a defect in the area routine rather than a property of the
  domain: `signed_double_area` now translates to a local origin, and the
  measured worst-case area error for a room-sized rectangle placed anywhere in
  the domain is 3.7×10⁻⁹ mm² — identical at 10⁶, 10⁷ and 10⁹. With the
  numerical argument gone, the bound must simply cover the documents people
  draw. ±1 km excludes a campus, an airport apron or a long site plan; ±10 km
  does not, and still leaves the 10⁻³ mm topology quantum about 537,000 ulps
  wide at the bound.

  **Coordinates are PLAN-LOCAL.** No coordinate reference system and no
  georeferencing transform is defined for v0.1. National-grid coordinates in
  millimetres run to 10⁸–10⁹ and are deliberately outside this domain; a
  survey importer rebases into local coordinates and carries its datum
  separately.
- **Opening fields (R4.0.4/R4.0.5/R4.1.1–R4.1.5/R4.2.1–R4.2.5).** The candidate domain covers
  them as follows; constraint SEMANTICS are normative in BEHAVIOR-SPEC
  section 4 — this model only proposes magnitude bounds and evaluation
  rules, ratified at 0030:
  - `widthMm`, `heightMm`, `depthMm` > 0, finite, proposed cap 10⁶ mm;
    existing diagnostics apply (`nonpositive-width`, `nonpositive-height`,
    `nonpositive-depth`) plus proposed `dimension-out-of-range` above the
    cap; `depthMm ≤ host.thicknessMm` is checked against stored host values
    (`depth-exceeds-host-thickness`);
  - `sillMm` ≥ 0, finite, proposed cap 10⁶ mm (`negative-sill`;
    `nonfinite-value`);
  - the host parameter u ∈ [0, 1] (R4.0.5) and host length L. L =
    √(dx²+dy²) over stored endpoint differences is GENERALLY IRRATIONAL and
    bounded by L ≤ 2√2·B ≈ 2.83×10⁹ mm (not 2B — a (−B,−B)→(B,B) diagonal
    has L = 2√2·B), so u·L is generally irrational too. Sound evaluation:
    - **Along-axis decisions (R4.1.2 strict fit; R4.1.4 horizontal overlap;
      R4.3.3 remap containment).** With m = dx²+dy² formed EXACTLY as a
      dyadic integer product sum (scaled big.Int), every along-axis
      inequality reduces to a sign test on rationals by squaring
      non-negative sides, in STRICT >0 forms (zero clearance rejects):
      u·L − w/2 > 0 ⟺ 4u²m − w² > 0 and
      L − u·L − w/2 > 0 ⟺ 4(1−u)²m − w² > 0; interval-overlap positive-length
      tests compare |u₁L − u₂L| against (w₁+w₂)/2 ⟺ 4(u₁−u₂)²m vs
      (w₁+w₂)². Zero and sign of these dyadic-exact rational expressions
      decide exactly (§4.3-grade arithmetic); zero clearance and touching
      cases (R4.1.2's u = 1/2 with L = 900; §4:F27 touching openings)
      therefore classify without tolerance.
    - **R4.3.3 remap containment** compares the aperture center parameter u
      against child split parameters tᵢ — segment-intersection parameters
      are class-C1 RATIONALS of stored coordinates, so containment
      tᵢ₋₁ < u < tᵢ is an exact rational comparison.
    - **Vertical checks are exactly dyadic.** R4.1.3's top check
      sillMm + heightMm ≤ host.heightMm and R4.1.4's vertical-interval
      positive-length test involve only stored dyadic values and their sums
      — exact scaled-big.Int comparisons, no surds.
    Overflow safety: all reduced comparisons are over integers whose
    u-bearing numerators reach 4·(2√2·B)² ≈ 3.2×10¹⁹ — EXCEEDING int64 —
    and are therefore carried in big.Int — no int64
    reliance.
  - `swingDeg` ∈ [0°, 180°], finite (`invalid-swing-angle`) — display-only
    symbolism per section 4, never fed to planar topology;
  - ownership/precedence: field-constraint semantics are owned by the
    behavior specification (normative sections 3–5); physical magnitude caps
    are proposed here and bind entity schemas only after 0030 ratification;
    serialization limits remain owned by the format specification.
- **Level fields (§2.1; bounds delegated here by line "exact magnitude
  bounds follow the ratified numerical model").** Proposed domain:
  `elevationMm` finite with |elevationMm| ≤ B (plan-datum relative height);
  `floorThicknessMm` ≥ 0, finite, proposed cap 10⁶ mm; `defaultWallHeightMm`,
  `defaultCeilingHeightMm` > 0, finite, same cap. Existing diagnostics
  (`nonfinite-value`, `nonpositive-thickness`, `nonpositive-height`) apply;
  cap violations take the proposed `dimension-out-of-range`. The §2.1
  warning comparisons (ceiling-below-wall; slab overlap
  elevation_n < elevation_(n−1) + floorThickness_(n−1)) are exact dyadic
  sums/comparisons of stored values.
- **Room record fields (R5.2.1/R5.2.2).** `seedMm` is a coordinate pair
  under the SAME regime as wall coordinates: finite binary64, |seed| ≤ B,
  `nonfinite-coordinate` on violation plus proposed `coordinate-out-of-
  range`; its strict-interior candidacy test (R5.2.3) and boundary/wall/
  exterior classification (R5.2.4) run through §4's exact predicate and
  escalation machinery over stored values.
- Non-finite input rejects under existing diagnostics: `nonfinite-value`,
  `nonfinite-coordinate` (R3.0.4), `nonpositive-thickness`,
  `nonpositive-height` (R3.0.4/R3.0.6).
- Magnitude-bound violations currently have NO diagnostic for coordinates,
  wall dimensions, level fields, or room seeds beyond the non-finite/
  non-positive tokens above. Proposed new tokens, naming owned by later
  ratification: `coordinate-out-of-range` (coordinates and seeds beyond B),
  `dimension-out-of-range` (thickness/height, opening dimensional fields,
  and level defaults above 10⁶).
- Signed zero: −0 is canonicalized to +0 at validation; thereafter equality
  comparisons are simultaneously numerical and bit-safe. Consequently
  `startMm == endMm` (R3.0.5) and join coordinate agreement (R3.3.4) are
  equalities on stored values. No epsilon enters validation; near-degenerate
  input classifies exactly, resolving the R3.0.8 deferral by refusing a
  tolerance.
- Derived-coordinate bound D = B + 10⁶ mm covers worst-case ideal derived
  vertices (offset ≤ (√2/2)·10⁶ beyond B). Binary64 magnitude overflow is
  unreachable at these scales; int64-range products of in-domain differences
  fit int64 (worst case (2·10⁹)² = 4×10¹⁸ < int64 max = 2⁶³−1 ≈
  9.22×10¹⁸)); wider
  intermediates use big.Int/expansion arithmetic, so no silent overflow
  exists anywhere.

Overflow derivations: with |x|,|y| ≤ B, any difference satisfies |Δ| ≤ 2B =
2×10⁹; products of two such differences ≤ 4×10¹⁸ < int64 max ≈ 9.22×10¹⁸;
shoelace accumulation over arbitrarily many vertices uses big.Int scaling,
never int64 wraparound.

## 4. Error model

Definitions: u = 2⁻⁵³. One correctly rounded binary64 operation has relative
error ≤ u (for z in [2^e, 2^(e+1)), ulp(z)/2 = 2^(e−53) ≤ u·|z|).
Sequential-model constant γₙ = n·u/(1 − n·u), used only inside gated fast
paths. ulp(0) = ulp(subnormal) = η := 2⁻¹⁰⁷⁴; for normal x, ulp(x) =
2^(e−52). Uniform subnormal bound: when an exact result z is zero-or-
subnormal, |fl(z) − z| ≤ η/2. Direction conditioning κ_dir(dx,dy) :=
max(|dx|,|dy|)/min(|dx|,|dy|), unbounded.

### 4.1 Per-operation normality gating

A chain of float operations earns analytic bounds ONLY if every elementary
operation passes a gate: operands normal-range AND rounded result
normal-range, checked on the actual computed value. A result that is zero or
subnormal fails its gate unless the operation is provably exact (power-of-two
scaling whose RESULT is verified representable is exact; see §4.2 step 2).
ANY failed gate ⇒ that quantity is recomputed by exact/outward-rounded
arithmetic (dyadic rationals; intervals with directed rounding) and its
certificate terms carried exactly. No analytic bound is claimed past a gate.

### 4.2 Direction construction

Counterexamples that shape this design: direction `(10⁹, 2⁻¹⁰⁷⁴)` has true
minor component ≈ 10⁻³³², below the minimum subnormal — unrepresentable by
any float scheme; direction `(2⁻⁵⁰⁰, 2⁻¹⁰⁷⁴)` passes ratio-based tests yet
its minor square underflows after max-only prescaling. The procedure below
therefore gates on ORIGINAL inputs first and preserves them unchanged for
fallback:

1. **Original-input gate (exact, before any float operation).** From the
   binary exponents of the stored dyadic values, compute κ_dir exactly. If
   κ_dir > 2⁹⁹⁸, OR the exact minor component |dᵢ|/√(dx²+dy²) < ρ := 2⁻¹⁰⁰⁰,
   route the WHOLE direction to the exact algebraic pair
   (dx, dy)/√(dx²+dy²) via section 6 — the stored (dx, dy) are never rounded
   by any intermediate step, so fallback loses nothing.
2. **Joint lossless scaling.** Otherwise choose the exact integer k placing
   max(|dx|,|dy|)·2ᵏ ∈ [2⁴⁹⁹, 2⁵⁰⁰). Then min(|dx|,|dy|)·2ᵏ ≥ 2⁴⁹⁹/κ_dir ≥
   2⁴⁹⁹/2⁹⁹⁸ = 2⁻⁴⁹⁹. Both scaled components lie in [2⁻⁴⁹⁹, 2⁵⁰⁰) — inside
   the normal range — so multiplying dyadic values by 2ᵏ shifts exponents
   without touching significands: the scaling is EXACT because its results
   are verified representable. (Scaling is not unconditionally exact; it is
   exact here by this range proof.) Consequently the scaled squares lie in
   [2⁻⁹⁹⁸, 2¹⁰⁰⁰], their sum in [2⁻⁹⁹⁸, 2¹⁰⁰¹], and the root in
   [2⁻⁴⁹⁹, ~2⁵⁰¹] — all normal. No component can be discarded before
   fallback because fallback happens BEFORE scaling (step 1) and scaling is
   exact.
3. **Gated fast-path computation** s′ = fl(fl(dx′²)+fl(dy′²)),
   r′ = fl(√s′), n̂ᵢ′ = fl(dᵢ′/r′), each operation individually gated per
   §4.1 (defense in depth; the ranges above predict the gates pass).

**Gated derivation.** Let S = dx′² + dy′² (exact real), n̂* = (dx′, dy′)/√S.
Counting rounded operations per output component: two squarings (θ₁, θ₂),
one addition (θ₃), one square root (θ₄), one division (θ₅) — five roundings.
Because both summands are POSITIVE there is no cancellation: relative errors
compose multiplicatively without amplification.

- s̃ = ((dx′²)(1+θ₁) + (dy′²)(1+θ₂))(1+θ₃) = S(1+Θ₃), |Θ₃| ≤ γ₃;
- r̃ = fl(√s̃) = √S(1+Θ₄), |Θ₄| ≤ γ₃ + u (|√(1+x) − 1| ≤ |x| on the gated
  range, then the sqrt's own rounding);
- n̂ᵢ′ = fl(dᵢ′/r̃) = n̂ᵢ*·(1+Θᵢ), |Θᵢ| ≤ γ₃ + 2u ≤ γ₅.

  Second-order note: expanding the composition exactly produces cross
  terms of order u² between Θ₃, Θ₄, θ₅; the γ-form envelopes them by
  construction — |Θ| ≤ nu/(1−nu) is derived for composed roundings with
  cross terms included, and n·u ≪ 1 on the gated range keeps the
  denominator bounded away from zero — so no second-order term escapes
  the stated envelope.

This relative-per-component bound holds at EVERY representable magnitude —
including small minor components — because every intermediate is built from
products and sums of positives followed by one division; magnitude safety is
supplied by the step-1/step-2 range proofs and step-3 gates, not by the
error algebra. Corollaries: |n̂ᵢ′ − n̂ᵢ*|∞ ≤ γ₅ (absolute, ‖n̂*‖∞ ≤ 1);
‖n̂′ − n̂*‖₂ ≤ √2·γ₅ (L∞→L2 conversion). Committing n̂ᵢ′ to storage adds at
most ½ ulp per coordinate.

4. Any gate failure during step 3 ⇒ recompute that quantity exactly per
   §4.1. Bypassed directions consume downstream stages under H-register
   obligations.

### 4.3 Canonical multiquadratic evaluation (zero/sign)

Interval refinement alone cannot decide exact zeros; the algorithm:

1. **Canonical form.** Every radicand is a positive rational m. Decompose
   m = r²·d with r ∈ ℚ₊ and d a squarefree positive INTEGER (√(5/4):
   r = 1/2, d = 5); absorb r into coefficients FIRST. Prime-factor d:
   √d = ∏√p over primes p with odd exponent. Expressions become
   Σ q_S·B_S over ℚ where B_S = ∏_{p∈S} √p for subsets S of the finite
   prime set P present. {B_S : S ⊆ P} is a basis of the multiquadratic
   extension ℚ(√p : p ∈ P) of degree 2^|P|. Multiplication reduces
   exponent-parity vectors modulo 2; coefficients merge in big.Rat
   arithmetic; rational square factors never leak into the basis.
2. **Zero test (exact, terminating):** zero iff every q_S = 0 — sound
   because prime-level power-products are linearly independent over ℚ.
   Composite-squarefree bases are NOT sound as a ℚ-basis: distinct
   composite squarefree radicands are multiplicatively dependent —
   e.g. √6 = √2·√3 gives the nontrivial ℚ-relation √2·√3 − √6 = 0 —
   so a coefficient-zero test over such a basis could miss real zeros.
3. **Sign test (terminating for nonzero):** some q_S ≠ 0 gives positive
   distance from zero; isolate by rational-endpoint interval refinement
   until exclusion. Cost spike-measured (H-0).
4. Budget breach ⇒ `geometry-indeterminate`.

Collinearity, overlap, containment, and hull decisions route here whenever
float filter stages are inconclusive.

### 4.4 Vertex certificates

Committed C2-derived vertices v = fl(P ± s·n̂) with s = t/2 carry certified
error radii εᵢ — rational OUTWARD UPPER BOUNDS on ‖δᵢ‖₂ (L2 radii), built
from derivation-backed terms only:

1. C1 commitment of P: per-coordinate cⱼ ≤ ½ ulp(Pⱼ); L2 contribution ≤
   outward(c₁ + c₂) (using √(c₁²+c₂²) ≤ c₁ + c₂; tighter outward rationals
   permitted);
2. half-thickness s: exact when halving is exact; else bounded by ½ ulp(s)
   (subnormal loss included: ½ ulp(s) = η/2 for subnormal s; ulp(0) = η);
3. normalized-direction error CONVERTED from the §4.2 gated derivation:
   ‖δn̂‖₂ ≤ √2·γ₅ on the gated fast path; contribution ≤
   s · outward(√2·γ₅);
4. offset assembly rounding P ± s·n̂: on gated normal-range ops,
   per-coordinate ≤ γ₃·max(|Px|,|Py|,s), converted ×√2;
5. cross term combining rounded s and rounded direction:
   ‖δᵥ‖₂ ≤ ‖δP‖₂ + |δs| + s·‖δn̂‖₂ + |δs|·‖δn̂‖₂ — the final product is
   retained, not dropped;
6. subnormal floors: any elementary operation whose result may be zero-or-
   subnormal contributes the uniform absolute bound η/2 IN ADDITION to the
   converted relative terms;
7. further boolean/face stages contribute certified terms ONLY if
   derivation-backed under §4.1-style gates (hypothesis register below);
   otherwise the vertex is uncertifiable and section 5 escalation applies
   wherever its data feeds an observable.

All certificate sums/comparisons run in exact rational or outward-rounded
arithmetic; εᵢ ∈ ℚ and εᵢ ≥ ‖δᵢ‖₂ holds constructively.

### 4.5 Hypothesis register

Stages whose bounds cannot be derived before algorithm structure is fixed
are hypotheses with mandatory spike obligations — each strategy either
derives explicit stage bounds (upgrading it to derivation-backed status) or
remains uncertified with uncertainty routed to escalation:

- **H-0** algebraic sign-evaluation cost (§4.3 step 3);
- **H-1** intersection-parameter chain beyond C1;
- **H-2** hull evaluation;
- **H-3** boolean vertex generation;
- **H-4** face assembly;
- **H-5** area-chain propagation beyond §4.4's enumerated terms;
- **H-6** gate-fallback frequency on realistic corpora (performance evidence
  for 0030).

0030 accepts strategies only with derivations or validated measured budgets
recorded. Uncertified data never feeds an observable without escalation.

### 4.6 Failure behavior

A non-finite intermediate surviving all guards, a derived vertex outside D,
or an escalation budget breach aborts the derivation: the batch rejects with
proposed diagnostic `geometry-indeterminate`; no partial output, no silent
substitution. These paths are internal-invariant failures, expected
unreachable for valid inputs; reaching one is treated as an implementation
defect and gated by corpus runs. Derived geometry exceeding D signals an
implementation defect, not user-input rejection.

## 5. Area and display (R5.3.1/R5.3.2 preserved exactly)

R5.3.2 makes displayed area a pure function of the EXACT area A. No display
decision may rest on uncertified data or underestimated bounds. This section
preserves that semantics; nothing here (and no ratification at 0030) may
alter observable display behavior — changes to normative display semantics
belong solely to the behavior-specification process.

**Units and exactly rational quantities.** The guard runs entirely in
SQUARE MILLIMETRES; R5.3.2's conversion to m² is the exact factor 1,000,000,
applied only after the boundary decision, never inside it. A_c, the exact
area of the committed rings (dyadic big.Int shoelace, hole rings subtracted —
under the DEFAULT representation of §1.1 ring vertices are binary64, hence
dyadic, so the shoelace sum is exactly computable; under the exact-rational
alternative A_c is likewise exact and every enclosure below collapses to
the exact value), is exact in mm². R5.3.2 displays
A/1,000,000 m² rounded to two decimals with half-even tie resolution: the
display grid points are multiples of 10⁴ mm² (= 0.01 m²), and the exact
tie/half boundaries between consecutive displayable values are therefore

  𝒯 = { (2j+1)·5000 mm² : j = 0, 1, 2, … }   (odd multiples of 5000 mm²),

e.g. F26's tie A = 30,245,000 mm² = 6049·5000 mm² ∈ 𝒯 (displays `30.24 m²`,
ties-to-even). All interval/boundary comparisons below are exact rational
comparisons in mm²; no float conversion precedes them.

**Outward enclosure of the radius:** given certified radii εᵢ ∈ ℚ (§4.4),
the rigorous deviation bound is

  |A(v + δ) − A(v)| ≤ ε_A := Σᵢ [ εᵢ·‖vᵢ₊₁‖₂ + εᵢ₊₁·‖vᵢ‖₂ + εᵢ·εᵢ₊₁ ]

(+ hole rings analogously), retaining the second-order εᵢεᵢ₊₁ perturbation.
Norms ‖·‖₂ are generally irrational, so the guard computes a SCALAR OUTWARD
ENCLOSURE ε̄_A ∈ ℚ with ε̄_A ≥ ε_A: each ‖v‖₂ is enclosed by a rational
interval [ℓ, ū] (integer square-root floor/ceil comparison of the rational
‖v‖₂²), and all products/sums round OUTWARD (upward for upper bounds) at a
guard precision declared in the implementation and recorded in benchmark
evidence. The enclosure must never underestimate ε_A; guard arithmetic is
never accumulated in floats.

**Decision procedure.** Construct [A_c − ε̄_A, A_c + ε̄_A] — rational
endpoints forming a conservative superset of the true deviation interval
about A_c. If it intersects no element of 𝒯 (exact rational endpoint
comparison), the displayed value follows from A_c alone: conforming and
deterministic, because the superset property pins A to A_c's side of every
boundary.

**Escalation (mandatory).** If the interval straddles 𝒯, OR any contributing
vertex lacks certification, the face's displayed value is recomputed through
certified/exact paths: refined-precision reconstruction of contributing
vertices (shrinking certified radii, recomputing ε̄_A), terminating in
§4.3-grade algebraic evaluation of A over the confirmed ring topology —
decidable and terminating. Discrete/topology conformance gates run FIRST; a
topology failure resolves there. Exact ties engage ties-to-even legitimately
under this procedure.

**Residual failure.** Only a budget breach exits abnormally, emitting
proposed diagnostic `area-tie-indeterminate`: a NONCONFORMANCE signal
attached to the projection (area being derived, non-authoritative data per
R5.3.5), never a licensed alternative display rule. `-0.00` cannot occur
(areas of open-set components are strictly positive, R5.1.6).

## 6. Escalation ladder and oracle

When a deciding predicate's magnitude lies within propagated uncertainty:

1. extended precision (double-double, or exact rational where the
   construction field permits);
2. canonical multiquadratic evaluation (§4.3) — terminates;
3. documented budget breach ⇒ `geometry-indeterminate`.

**Independent exact-decision oracle.** Ground truth for topology-mismatch
rates and ambiguity resolution: the same stored-float inputs evaluated via
§4.3 machinery, yielding ground-truth discrete decisions and exact continuous
values. Where normative fixtures state expectations, they take precedence.
Topology-mismatch rate = fraction of corpus cases differing from this oracle.
Quantized clipping candidates AND the primary pipeline answer to the SAME
oracle; neither judges itself.

## 7. Conformance protocol

**Inputs.** Fixture decimal literals decode to binary64 with
round-to-nearest-even (`strconv.ParseFloat` semantics; shortest-form
round-trip guaranteed). Stored float64 values are the sole execution inputs,
matching R3.0.8's framing.

**Expectations.** Fixtures state expectations as exact-real facts,
independently of any implementation. The harness compares implementation
outputs DIRECTLY to stated expectations; it never re-derives expectations
from the implementation's own committed geometry.

**Expectation encoding (proposal).** Adoption amends the behavior
specification, so this is a proposal for the format-corpus owners, not an
edit made here:

- Tier 1 — rational: integer numerator/denominator pairs. Covers ALL current
  normative section 3–5 fixtures (integer and denominator-41 families).
- Tier 2 — quadratic surds: (a + b√m)/c, m decomposed r²·d as needed —
  single-radicand offset constructions.
- Beyond Tier 2 SHOULD be avoided by fixture design; if ever needed, the
  encoding question returns to behavior-spec maintainers. This document
  claims coverage for Tiers 1–2 only.

**Comparison classes.**

- Discrete/topological outcomes — contact classes, split structure, join
  membership, face count and topology, nesting, statuses, orderings,
  warning/rejection tokens — MUST match stated expectations exactly. No
  tolerance of any kind.
- Continuous outputs (vertex positions, areas) compare against stated exact
  values within budgets composed from section 4 terms; concrete numeric
  budgets are measured on spike corpora and ratified at 0030. None are
  asserted here.

Topology conformance (discrete, exact) and geometric error (continuous,
budgeted) are tracked SEPARATELY and never conflated. A quantized internal
clipping pipeline fails acceptance if EITHER its topology-mismatch rate is
nonzero on the corpus OR its measured geometric budget exceeds ratified
limits.

**Runtime guarantee beyond the corpus (mandatory, not corpus-dependent).**
Measured mismatch rates justify 0030 ACCEPTANCE; they are never a runtime
safety argument for unseen inputs. Any quantized candidate must therefore:

1. **Pin its grid.** Declare scale factor, integer grid range/width, and
   overflow behavior as fixed constants in its evidence record; silent
   rescaling is prohibited.
2. **Certify or escalate at runtime.** For EVERY invocation, a discrete
   outcome is admissible only if the pipeline certifies it — the deciding
   predicate's magnitude lies outside its declared uncertainty envelope
   (derived bounds where available, otherwise the strategy's recorded
   worst-case uncertainty). Uncertain outcomes MUST escalate to the section
   6 exact-decision oracle path in the same run; if that escalation exceeds
   its budget, the batch rejects via `geometry-indeterminate`. Guessing from
   measured rates alone is prohibited: no input may receive an uncertified
   discrete outcome regardless of how rare the configuration was in the
   corpus.
3. **Report honestly.** Corpus mismatch rates remain acceptance evidence;
   runtime escalation counts and budget breaches join H-6's evidence record.

## 8. Determinism

Same toolchain + same released binary + same input bytes ⇒ identical
results. Cross-platform bit-identity is NOT claimed: Go permits FMA fusion,
libm/compiler variation exists, and R3.0.8/F36 mandate no bit-identical
output. Predicate implementations are written so correctness does not depend
on fusion behavior. Identical corpora run on every supported platform during
issues 0006/0007 and in CI from tracker 0008 onward. Divergence means
semantic disagreement — differing discrete outcome, topology, status,
ordering, token, or certified-budget breach — never mere byte differences.
Observed divergence blocks acceptance until eliminated. linux/amd64 and
linux/arm64 portability is standard pure Go; android/arm64 support is a
declared HYPOTHESIS pending demonstration (ADR 0002 itself remains
Proposed).

## 9. Diagnostics summary

| Bound | Diagnostic (proposed names; owned by later ratification) |
|---|---|
| Non-finite coordinates/values | Existing: `nonfinite-value`, `nonfinite-coordinate` (R3.0.4) |
| Non-positive thickness/height | Existing: `nonpositive-thickness`, `nonpositive-height` (R3.0.4/R3.0.6) |
| Coordinate above B | New: `coordinate-out-of-range` |
| Thickness/height above 10⁶ mm | New: `dimension-out-of-range` |
| Derived vertex outside D; escalation budget breach | New: `geometry-indeterminate` |
| Display certification unresolved after full escalation | New: `area-tie-indeterminate` — nonconformance signal, NOT an alternative display rule |

Bounds apply to authoritative fields at validation. A derived-geometry breach
of D is an implementation-defect signal, not user-input rejection.

## 10. Candidate library and provenance ledger

Ledger-before-reliance rule: no entry enters the relied-upon set until its
row is complete (artifact URL, live-check date, license text source, notice
obligations, consulted/unconsulted status, adaptation type). Statuses:
PINNED-RELIABLE; PINNED-PENDING-STUDY (pinned now; reliance requires
completed study log per CLEAN-ROOM-POLICY); CITE-VERIFIED (citation checked;
artifact unconsulted); QUARANTINED (excluded until cleared).

| ID | Entry | Role | Evidence recorded | License basis | Status |
|---|---|---|---|---|---|
| P-1a | Shewchuk page https://www.cs.cmu.edu/~quake/robust.html (HTTP 200, checked 2026-08-23) | Provenance hub; hosts public-domain statement | Statement location recorded | Author's public-domain statement for the code | PINNED-RELIABLE — hub/provenance-record role ONLY (consultation log: evidence/0006-predicates/PROVENANCE.md) |
| P-1b | Shewchuk, *Adaptive Precision Floating-Point Arithmetic and Fast Robust Geometric Predicates*, DCG 18:305–363 (1997); CMU-CS-96-140(R); https://people.eecs.berkeley.edu/~jrs/papers/robustr.pdf (checked 2026-08-23) | Predicate algorithm source | URL + front-matter duplication notice | Freely duplicable with notice | PINNED-RELIABLE — consulted as MATHEMATICAL DESCRIPTION; independently implemented (no source code, pseudocode, or vectors taken; consultation log: evidence/0006-predicates/PROVENANCE.md) |
| P-1c | `predicates.c` rev. 1996-05-18, http://www.cs.cmu.edu/afs/cs/project/quake/public/code/predicates.c (HTTP 200, checked 2026-08-23) | Public-domain reference code | Header statement location | Public domain (code ONLY, not paper text) | UNCONSULTED / NON-RELIED — deliberately never opened or studied (see PROVENANCE.md consultation decision) |
| L-1 | Go stdlib `math/big`, `math/bits`; reference toolchain go1.27.0 (released 2026-08-19; go.dev checked 2026-08-23) | Oracle/exact dyadic arithmetic | NO operative project toolchain pin exists yet (tracker 0008 is unscaffolded and records none); go1.27.0 is a REFERENCE pin only, re-verified when 0008 lands; adaptation type: NONE — stdlib packages used unmodified | BSD-3-Clause, Copyright (c) 2009 The Go Authors — go.dev/LICENSE | PINNED-RELIABLE (stdlib); operative-pin verification remains open at 0008 |
| B-1a | Martínez, Rueda, Feito, *A new algorithm for computing Boolean operations on polygons*, Computers & Geosciences 35(6):1177–1185 (2009), DOI 10.1016/j.cageo.2008.08.009 | Boolean strategy literature | Citation verified at publisher DOI | Publication; authors' C++ artifact license UNREVIEWED, untouched | CITE-VERIFIED — NOT CONSULTED |
| B-1b | Guibas, Marimont, *Rounding arrangements dynamically*, SCG '95 pp. 190–199 (1995), DOI 10.1145/220279.220300; extended IJCGA (1998), DOI 10.1142/S0218195998000096 | Snap-rounding literature | Both DOIs verified | Publications; no code | CITE-VERIFIED — NOT CONSULTED |
| B-1c | de Berg, Cheong, van Kreveld, Overmars, *Computational Geometry: Algorithms and Applications*, 3rd ed., Springer (2008) | Bentley–Ottmann sweep literature | Edition/publisher verified | Published book; no code | CITE-VERIFIED — NOT CONSULTED |
| P-2 | Yap, *Fundamental Problems of Algorithmic Algebra*, Oxford University Press, New York, 2000, ISBN 0-19-512516-9; author's book page https://cs.nyu.edu/yap/book/ (TOC/errata live, checked 2026-08-23) | Algebraic-number literature | Bibliographic record (ISBN/publisher/year) verified today | Published book; no code | QUARANTINED-until-consulted (edition/section pinning at first consultation; until then §4.3 rests on its stated mathematical argument plus spike validation H-0) |
| B-2 | Clipper2 v2.0.1, source https://github.com/AngusJohnson/Clipper2 (release tag + LICENSE live-checked 2026-08-23) | Alternative boolean strategy | Source URL, tag, license text location recorded; platforms C++17/C#/Delphi (no official Go binding); notice obligation: carry Boost-1.0 license text in distributions; transitive-dependency review NOT YET PERFORMED | Boost Software License 1.0 | CITE-VERIFIED — NOT CONSULTED; third-party record INCOMPLETE per THIRD-PARTY-POLICY: black-box study permitted, but any adoption requires completed transitive scan, NOTICE integration, SBOM entry, and maintenance assessment at 0030 before acceptance |
| X-1 | neilpa/cgo-shewchuk-robust v0.2.0 (cgo, stale) | Prior art | Unlicense displayed | Unlicense | QUARANTINED |
| X-2 | intdxdt/robust | Prior art | Version/license unresolved | Unverified | QUARANTINED |
| X-3 | georust/robust | Prior art | Version/license unverified | Unverified | QUARANTINED |

Issue-0006 continuation records (2026-08-24): consultation logs, adaptation
types, and test-vector provenance for P-1a/P-1b as consumed by issue 0006,
the deliberate P-1c non-consultation decision, and recorded test-vector
provenance for the adopted P-3 published adversarial vectors
(Bartels/Fisikopoulos/Weiser 2023, CC BY 4.0; exactly two coordinate triples,
attribution retained) live in
[evidence/0006-predicates/PROVENANCE.md](evidence/0006-predicates/PROVENANCE.md);
the P-3 record covers test-vector provenance only — it is not model adoption,
a threshold/representation choice, or release-policy ratification. The same
evidence empirically validates §4.1 gating interpretation, §4.2 direction
gates, and §4.4 certificate terms ONLY; the §4.3 canonical multiquadratic
machinery is ABSENT and H-0 cost is UNMEASURED, remaining open and owned by
issue 0030. These status updates complete ledger METADATA for the recorded
roles only; they do not ratify the candidate model or alter any open gate in
§11–§12.

Policy compliance: abstract algorithms are never described as licensed; every
consulted artifact receives publication/artifact/adaptation-type/license/
notice/test-vector-provenance entries; NO GPL source is consulted (CGAL code,
Sweet Home 3D, DobryDom3D, or any floor-plan application); uncertain
provenance quarantines per CLEAN-ROOM-POLICY. Unofficial Clipper2 Go bindings
have unverified provenance and are unconsulted.

Boolean-strategy candidates for issue 0007: **B-1** original pure-Go engine
implemented from B-1a/B-1b/B-1c literature (preferred; GPL implementations
off-limits); **B-2** Clipper2-based approach (requires the full third-party
record and conflicts with the pure-Go core preference via native binaries).
Selection is evidence-driven at 0007; ratification at 0030.

## 11. Delegated proof obligations

To issue 0006 (robust predicates — evidence producer only):

1. Clean-room adaptive `orient2d` + intersection-parameter machinery: exact
   signs over stored binary64, subnormals, filter fallbacks, FMA
   independence, non-finite policy; oracle = exact dyadic-rational
   representation of inputs (never unspecified `big.Float`). Empirical
   validation of §4.2 gates and §4.4 certificate terms.
2. Benchmarks vs naive float (Go version, hardware, corpus, allocations),
   fully recorded. No pass/fail authority.
3. Adversarial corpus within section 3 bounds, including extreme-κ_dir
   directions, minimal-thickness offsets (P = 0, t = 2η), and mixed-scale
   pairs like (2⁻⁵⁰⁰, 2⁻¹⁰⁷⁴).
4. Whether `incircle` is needed (this document predicts NO for v0.1 planar
   work).
5. Provenance verdicts per section 10 statuses.

To issue 0007 (clipping/planar topology — evidence producer only):

1. Strategy evidence among B-1/B-2 on the mandated corpus (thick-wall L/T/X
   joins, unequal thickness, shared edges, touching vertices, slivers,
   opening difference per R4.2.2 ordering, nested faces, large translations),
   reporting measured quantization scale/range/overflow/max-coordinate-error
   AND topology-mismatch rates vs the section 6 oracle — separately.
2. Fill/winding rules, self-intersection policy, hole handling, invalid-input
   behavior, adapter renormalization into R3.2.4/R4.2.3/R5.1.3 canonical
   forms.
3. Separate planar-face-extraction evidence (arrangement + traversal, pinch
   splitting R5.1.2, exterior/hole classification R5.1.4); clipping alone is
   assumed insufficient.
4. Hypotheses H-0…H-6: per-strategy derivations upgrading stages to
   derivation-backed status, or strategies accepted only with
   escalation-on-uncertainty wiring demonstrated end-to-end.
5. Provenance records distinguishing publications from implementations.

Gate semantics (PROPOSED delegation — ADR 0002 itself remains Proposed, and
this allocation takes effect only if 0030 accepts it and the ADR is updated
through its own process): gate 1 (Phase 0) requires robust intersections,
polygon operations, and planar room topology with documented coordinate
bounds AND acceptable performance; correctness is judged against corpora and
ratified budgets, while performance acceptability would be judged by 0030,
which MUST define at least one measurable Phase 0 criterion before judging
the recorded measurements. Issues 0006/0007 remain evidence producers in
every reading; they hold no gate authority. **Phase 0 performance-evidence
chain (resolved by orchestrator ruling, 2026-08-23):** 0006 records
predicate/contact-classification benchmarks (already in its acceptance
criteria); 0007 RECORDS wall-clock and allocation measurements of its
boolean/planar-face pipeline on the same named corpus and reference
environment as its correctness runs — measurements are evidence only, with
no pass/fail threshold in 0007; 0030 MUST define the Phase 0 performance
criterion BEFORE judging, then judges that criterion against the recorded
0006 benchmarks, 0007 pipeline measurements, and H-6 fallback/escalation
counts together. There is no other producer and no other judge. The
criterion 0030 registered under this obligation, and its disposition
record, live in
[evidence/0030-numerical-model/PHASE0-PERFORMANCE-CRITERION.md](evidence/0030-numerical-model/PHASE0-PERFORMANCE-CRITERION.md)
and
[evidence/0030-numerical-model/RATIFICATION.md](evidence/0030-numerical-model/RATIFICATION.md).
Gate 2 (Phase 1 targets, named
hardware, tracker 0032) is assessed at Phase 1 exit. Flutter-bridge (gate 3,
tracker 0024) and renderer (ADR 0003, tracker 0025) failures are outside this
document's lane. Under no reading may any issue or ADR alter normative
R5.3.2 (or other BEHAVIOR-SPEC) display semantics.

ADR-0002 revision triggers (PROPOSED, same delegation qualifier as above),
operationalized: (1) robustness failure — no
candidate satisfies the corpus after revisions attributable to the Go/float
approach; (2) custom-stack burden — passing requires a research-grade
subsystem well beyond adaptive predicates + classified constructions + one
boolean strategy; (3) Phase 0 performance failure — 0030 judges recorded
measurements unacceptable under its pre-defined criterion with no caching/
locality strategy (R5.1.9) plausibly closing the gap; (4) dependency/
packaging failure — only correctness-passing candidates force copyleft
linkage, unverifiable provenance, or ABI-incompatible native packaging while
pure-Go alternatives fail 1–3.

## 12. Ratified numeric policy and open obligations

**Closed by ED-0100 (2026-09-04):** the coordinate bound B, the dimension
cap, the derived-coordinate bound, and the coincidence/weld quantum. See §3.

**Superseded by the shipped implementation, not decided:** boolean strategy
(B-1 vs B-2), C2 representation selection among hypotheses, expectation
encoding, and the escalation-cost/guard-precision policy. These were choices
between candidate designs for a Go core that no longer exists. The Rust core
implements a filtered-with-exact-fallback orientation predicate and builds its
booleans by boundary classification over it; §§4.2–4.5, 6, 10 and 11 describe
machinery that was never built and are retained only as a record of the
alternatives considered. **Nothing in them is normative.**

**Closed by ED-0103 (2026-09-12):** the five questions ED-0100 left open are
decided below as N-1 to N-5. Three of them are decided by stating what the
implementation already does, because that behaviour is defensible and the
alternative would have changed a format-observable contract for no benefit.
Two of them are decided AGAINST the implementation, because the implementation
contradicts a normative rule; those carry an obligation rather than a
ratification, and the obligation is named with its owner.

### N-1 Stored coordinates are not quantized; sub-quantum geometry is topologically absent

**Ratified.** A stored coordinate is any finite binary64 value within §3's
bounds. Nothing rounds it to the 10⁻³ mm topology grid, on the way in or on
the way out.

The topology grid applies to the ARRANGEMENT, not to storage. Vertex identity,
face traversal and room detection are computed on the image of the coordinates
under N-2's identity relation, so a feature whose extent falls below the
quantum contributes no vertex distinct from its neighbours, bounds no face, and
detects no room. It is topologically absent.

**The asymmetry this creates is deliberate and must not be relied on
accidentally.** Measurement reads stored coordinates, not the arrangement. A
wall 0.4 µm long has a positive stored length and reports one; it has no
topological effect whatever. The two answers are consistent because they answer
different questions, and no rule requires a measured feature to be a
topological one.

Rejected: quantizing stored coordinates, which would make the persisted value
differ from the value the user supplied and would move the identity question
onto the grid rather than settling it; and rejecting sub-quantum features at
validation, which would refuse documents the geometry handles correctly.

### N-2 One identity relation: the topology grid. Metric proximity is admission, not identity

**Ratified.** Two points are the SAME POINT when they quantize to the same
topology-grid cell — `VertexKey::of` in `domowik-geometry::arrangement`. That
relation, and no other, decides vertex identity, graph adjacency, cut
coincidence and face traversal.

**Why a grid and not a distance.** An identity relation must be an equivalence
relation, and a metric ball is not transitive: three points can pair up within
tolerance while the outer two are twice the tolerance apart. Grid rounding is
transitive, order-independent and total, and the arrangement already depends on
those properties.

**The three rules that disagree today, with the disagreement measured.**
`WELD_MM` and `JOIN_COINCIDENCE_MM` are both 10⁻³ mm, which reads as one
tolerance and is not. Grid identity admits points up to √2 × 10⁻³ mm apart —
the cell diagonal — and can separate points arbitrarily close together across a
cell boundary. The join-agreement predicate in `Document::validate` measures
Euclidean distance from the FIRST member. Executed against the shipped code:

- `(4×10⁻⁴, 4×10⁻⁴)` and `(−4×10⁻⁴, −4×10⁻⁴)` are ONE vertex to the
  arrangement and a `JoinCoordinateDisagreement` to validation: they share a
  cell, and their distance is 1.131×10⁻³ mm.
- `(4×10⁻⁴, 0)` and `(1.4×10⁻³, 0)` validate as coincident and are TWO
  vertices to the arrangement: their distance is exactly the tolerance, and
  they fall either side of a cell boundary.
- Three members at `0`, `9×10⁻⁴` and `−9×10⁻⁴` on one axis validate or fail
  **according to which member is listed first** — max radius 9×10⁻⁴ from the
  middle member, 1.8×10⁻³ from either outer one. FORMAT-SPEC §2.3 records join
  membership as an unordered set (C3), so this is a determinism defect, not a
  tolerance question.

**Ratified policy.** Identity is the grid. The join-agreement check is a
DIFFERENT obligation — an admission predicate deciding whether a document's
stored endpoints agree well enough to describe one join — and it is restated as
such rather than left standing as a third identity rule:

1. Admission must not contradict identity. Points the arrangement treats as one
   vertex must not be reported as disagreeing, which requires
   `JOIN_COINCIDENCE_MM ≥ √2 · WELD_MM`. The two constants are equal today, so
   the first case above is reachable. The ordering between them is a derived
   requirement and belongs in the source beside the constants, not in a
   reviewer's head.
2. Admission must be a function of the member SET. The star-from-first form is
   replaced by a pairwise or circumscribed-radius test, so member order stops
   changing the verdict.

**DELIVERED (2026-09-12), and stronger than clause 1 above.** `Document::validate`
decides R3.3.4 by topology-cell equality, so admission IS identity rather than a
distance ordered against it. Widening the ball would have removed the false
rejection and made the false acceptance MORE common — no metric ball aligns
with a grid in both directions — so the two rules share a predicate instead of
a constant. The quantum does not move. Both disagreements above are regression
tests, as is the order dependence. `domowik_geometry::locality` carries a guard
that fails if the two crates' quanta drift, since geometry depends on format and
the constant cannot be shared. See ED-0103 amendment (a).

`join_at` and the `domowik-edit` proximity queries keep a metric ball
deliberately: "is there a join near this point" is a different question.

### N-3 Split points persist as the nearest binary64. No exact rational storage

**Ratified.** Intersection parameters and `lerp` are evaluated in ordinary
binary64 and the constructed point is stored as the nearest representable
value. FORMAT-SPEC §2/§3 are unchanged, and no rational coordinate
representation is introduced.

**Why this is safe, quantified.** Topological coherence comes from N-2's
identity relation, not from coordinate exactness: two independently constructed
copies of one intersection meet in the adjacency structure because they
quantize alike, not because they are bit-identical. At the coordinate bound one
ulp is 1.86×10⁻⁹ mm, about 537,000 times finer than the topology quantum, so
construction rounding cannot move a point into a different cell unless the true
point already lies within a few ulp of a cell boundary — a case a sub-quantum
perturbation of the input would also decide differently, and which no
representation choice removes.

**What this does NOT claim.** The exact orientation predicate makes the
DECISIONS exact; it does not make CONSTRUCTIONS exact, and nothing in this
section says otherwise. A constructed vertex does not lie exactly on either
parent segment.

### N-4 Area is exact and its display is half-even — the implementation does not meet this, and the contract does not move

**NOT ratified as built.** BEHAVIOR-SPEC R5.3.1 states an exact-real area and
R5.3.2 a half-even display rounding of that exact value. The shipped CLI
divides a binary64 shoelace result by 10⁶ and formats it with `{:.2}`, which
rounds the STORED DOUBLE rather than the exact area. Both of R5.3.2's normative
tie examples fail:

| Exact area | Required | Produced |
|---|---|---|
| 30,245,000 mm² | `30.24 m²` (4 is even) | `30.25 m²` |
| 10,655,000 mm² | `10.66 m²` (6 is even) | `10.65 m²` |

The nearest double to 30.245 lies above the tie and the nearest double to
10.655 lies below it, so half-even applied to the double disagrees with
half-even applied to the exact value in both directions. Neither case is
currently asserted by a test.

**The contract stands; the implementation owes conformance.** Weakening R5.3.2
to an accuracy statement would ratify a defect that is fixable with machinery
the crate already contains, so it is not weakened.

**The specified fix.** A ring's doubled signed area is a sum of products of two
stored coordinates, so it is exactly a dyadic rational and
`domowik-geometry::dyadic` already represents and compares those exactly at
arbitrary width. Then:

1. Evaluate the shoelace over STORED coordinates, not translated ones. The
   translation `signed_double_area` performs is a floating-point mitigation and
   is itself inexact — `a.x - origin.x` rounds — so it must not appear on the
   exact path. This is the same argument `exact_det` already records for
   preferring stored coordinates to their differences, and in exact arithmetic
   the cancellation the translation was defending against cannot occur.
2. Subtract hole rings exactly rather than halving and summing in binary64.
3. Round for display by exact comparison against the decimal midpoint: with the
   area as `M · 2^e`, rounding `A/10⁶` to two decimals is an exact integer
   comparison of `2 · M · 2^e` against `(2k + 1) · 10⁴`, with the half-even tie
   resolved on equality. No floating division enters the display path.

**DELIVERED (2026-09-12).** `domowik_geometry::exact_area` implements exactly
the above. `display_hundredths_m2` is a filtered entry point: it evaluates the
float area with a proven forward error bound and builds an exact value only
when that value lies within the bound of a tie, so the allocation is paid by
the inputs that need it. Non-finite input and `i64` overflow both return `None`
rather than saturating, and the preconditions a shoelace cannot check — simple
rings, contained holes, degenerate rings — are documented at the module.

The numeric contract is `hundredths_m2_half_even`; rendering the string belongs
to the room module, so R5.3.2's arithmetic exists once and its presentation
once. A further counterexample from the issue-0060 owner is a regression here:
a single `f64` division can round a quotient that merely lies NEAR a tie ONTO
it, after which ties-to-even walks to the wrong neighbour — which is why the
rounding cannot be done by dividing first, however careful the mode afterwards.

### N-5 Positive-area component linking is exact-signed. The relative tolerance goes

**NOT ratified as built.** R5.1.9 links two join-connected components whose
occupied regions "intersect with POSITIVE AREA". `geometry::locality` decides
this as `overlap > MIN_OVERLAP_MM2.max(OVERLAP_RELATIVE × separate)` with
`OVERLAP_RELATIVE = 10⁻¹²`, which is not a positive-area test.

**The stated rationale for the relative term is stale.** It reads: "An area
computed from coordinates near `COORD_BOUND_MM` carries an absolute error far
above `MIN_OVERLAP_MM2`." That was true of the untranslated shoelace. ED-0100
translated `signed_double_area` to a local origin precisely so that accuracy
stops depending on distance from the origin, and §3 records the resulting
worst-case error as 3.7×10⁻⁹ mm², identical across the domain. The term now
guards an error that the routine no longer makes.

**What it does instead is under-include, against a normative rule.** The
relative term overtakes the absolute floor above a separate area of 10⁶ mm²,
which is 1 m². At a separate area of 10¹² mm² — two regions of roughly 700 m on
a side, which is the campus and site-plan case ED-0100 RAISED the coordinate
bound to admit — the tolerance is 1 mm², and a genuine 0.5 mm² overlap is
reported as no link at all. Over-inclusion would merely recompute more faces
than necessary; under-inclusion means faces that R5.1.9 requires to recompute
do not.

**Ratified policy.** The link test is a sign test on an exactly evaluated area
difference. With N-4's exact shoelace, regions that merely touch cancel to
exactly zero and no summation tolerance is needed at all. `OVERLAP_RELATIVE` is
removed rather than retuned.

**The residual, stated so it is not mistaken for exactness.** Exact summation
removes the shoelace error; it does not make the union's CONSTRUCTED vertices
exact (N-3). Where a link decision turns on a span whose endpoints were
constructed, the residual is the construction error of those vertices — bounded
by N-3's ulp figure and the number of constructed vertices on the span, not by
a constant chosen to make a test pass. If a floor is retained for that residual
it is DERIVED from those two quantities and recorded with its derivation.

**DELIVERED (2026-09-12), with a third answer the decision did not
anticipate.** `MIN_OVERLAP_MM2` and `OVERLAP_RELATIVE` are both removed and no
floor replaces them. Measured on 1,000 pseudo-random touching pairs, 971 cancel
to EXACTLY zero and 29 leave a residue near 10⁻⁹ mm² — traceable not to a
distribution of rounding noise but to ONE union vertex sitting an ulp from an
input vertex it should have reproduced.

So `positive_area` reports **Yes / No / Uncertain**: the arithmetic is certain
and the geometry is not. `locality::link` maps Uncertain to a link, which is
over-inclusion against R5.1.9's "EXACTLY" and is recorded as a deviation rather
than called conformance, because recomputing a component that did not need it
produces identical geometry while skipping one leaves the document stale.

The defect that creates the band belongs to the boolean pipeline — a cut
parameter landing on an existing vertex should reproduce it — and fixing it
removes both the band and the deviation. `arrangement.rs` and `boolean.rs` are
issue 0058's. ED-0103 amendment (a) carries the witness.

### N-6 Boundary-edge ownership and snap exclusion

Ownership of a canonical occupied-boundary edge, and the exclusion rule that
consumes it, are decided by **ED-0103**: exclusion is decided per maximal
constant-owner sub-segment rather than per canonical edge; the ownership unit
is the contributing RING, not the wall; and the join-patch exclusion set is
derived in core from the excluded-wall set while the caller-supplied join list
keeps its narrower join-node meaning. The counterexamples that forced each
clause are recorded there. Implementation belongs to issue 0058.

## 13. Evidence summary for the shipped model

This section discharges issue 0030's first acceptance criterion for the RUST
core. It summarizes what is established and names what is not; it is not a
restatement of the Go-era evidence in §§6, 10 and 11, which ED-0100 superseded.

**Correctness.** `orientation` is exact for every finite input, including
mixed subnormal and near-maximal magnitudes: a filtered determinant with a
Shewchuk error bound escalates to an exact integer evaluation of the six-product
determinant over stored coordinates (`exact_det`). Two defects found by
adversarial review during ED-0100 — a subnormal-scale triangle reported
Collinear, and a 2 µm square at the coordinate bound computing exactly zero
area — are fixed and carry regression tests that fail against the previous
implementation. Booleans are built by boundary classification over that
predicate, with holes, multipolygons, nested faces and point-pinch cycles
covered. 241 tests in `domowik-geometry` pass at HEAD (`cargo test --locked -p
domowik-geometry`, 2026-09-12, Apple M4 Max, rustc 1.98.0).

**Determinism.** Output is canonical under R3.2.4 in full — exterior rings
counter-clockwise, holes clockwise, least-vertex rotation, sorted holes, sorted
components — so one document always yields one sequence. Turn direction is
decided by the exact predicate, so no vertex is kept or dropped by rounding.
The one order dependence found in this audit is in join VALIDATION, not in
geometry, and is recorded as N-2's second obligation.

**Coordinate range and overflow.** §3, ratified. Products in the exact path are
exact by construction rather than range-limited; the filtered path's error
bound is the gate that decides whether it may answer.

**Quantization and tolerance.** N-1, N-2, N-3. One identity relation, at
10⁻³ mm, applied to the arrangement and not to storage. The cut-collapse
tolerance is spatial rather than parametric, tied to `WELD_MM` and therefore
correct at any edge length.

**Performance.** Recorded, with each figure carrying its source rather than
being re-measured here. `orientation` measures 1.15 ns filtered and 1.79 ns on
the exact fallback (issue 0006, Rust delivery); the Go implementation's
unconditional exact path cost 12,134 ns for the same answer. A 22-wall,
100-room fixture unions in 392.88–398.56 µs (issue 0058 checkpoint,
2026-09-11). Cached 840-wall snap queries median 9.36–9.89 µs (issue 0058,
Apple M4 Max). All are far inside the 10,000 ns/op predicate bound whose miss
triggered ADR 0002's revision.

**What this summary does NOT establish.** It is not a re-run of the registered
v3 Phase 0 protocol, which measured Go code against independent-process medians
and per-case p95/RSS. That protocol judged the GO stack and its verdict stands
in the record; ADR 0002 is Superseded and ADR 0008 Accepted, so re-running it
against Rust would answer a question no longer being asked. No p95, RSS or
frame-time claim is made here.

## 14. Selected algorithms, provenance and licences

This section discharges issue 0030's second acceptance criterion for the Rust
core. Every entry is an INDEPENDENT IMPLEMENTATION from a published
description, per `docs/CLEAN-ROOM-POLICY.md`. No implementation of any of these
algorithms was consulted, and no GPL-licensed floor-plan application's source
was consulted for any of them.

| Algorithm | Where | Published source | Adaptation |
|---|---|---|---|
| Adaptive filtered orientation | `geometry::orientation` | Shewchuk 1997, "Adaptive Precision Floating-Point Arithmetic and Fast Robust Geometric Predicates", §2.2 forward error analysis | Error bound taken from the paper and cited at the constant; the exact path is this project's own, not the paper's expansion arithmetic |
| Exact determinant sign over dyadic integers | `geometry::exact_det`, `geometry::dyadic` | None — derived from IEEE 754-2019 §3.4, which makes every finite binary64 exactly `±m · 2^e` | Original. Chosen over Shewchuk expansions because those lose exactness under underflow, with a witness recorded in the module |
| Monotone-chain convex hull | `geometry::polygon::convex_hull` | A. M. Andrew, "Another efficient algorithm for convex hulls in two dimensions", 1979 | Turn test replaced by the exact predicate; degenerate input returned in reduced form per R3.2.3 |
| Crossing-number point-in-ring | `geometry::arrangement::classify_point` | Standard published rule; boundary case decided by the exact predicate before the interior test | Half-open vertical-span rule so a ray through a vertex counts once |
| Boundary classification booleans | `geometry::boolean` | None — built from this project's specification (R3.2, R3.2.4) | Original. Rooms are the bounded components of the COMPLEMENT, traced from the same boundary in the opposite direction, because clipping alone does not solve room topology |

**Licences.** All of the above are original code in this repository under
Apache-2.0. The exact and filtered predicate paths introduce no dependency:
`exact_det` uses `u64`/`u128` and a fixed array, `dyadic` a `Vec<u64>`. An
algorithm described in a paper carries no code licence, and none is claimed
here. Distributed dependencies and their licences are inventoried in
`docs/RUST-DEPENDENCIES.md`, regenerated and diffed by a verification stage.
