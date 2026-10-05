# Issue 0006 Findings — Robust Predicates Spike

Status: evidence for the Candidate numerical model
([NUMERICAL-MODEL.md](../../NUMERICAL-MODEL.md)). The model remains
**unratified**; issue 0030 ratifies, revises, or rejects it. This document
records findings only and grants no ratification authority. Issue status:
evidence producer per NUMERICAL-MODEL.md §11 delegation.

## 1. Incircle verdict (§11 obligation 4)

**incircle is NOT needed for v0.1 planar work.** Rationale:

- Every operation in NUMERICAL-MODEL.md §2 reduces to orientation signs,
  segment-intersection parameters (class C1), collinearity/containment sign
  tests over rationals or quadratic surds, and dyadic comparisons. None of
  the §2 rows involves in-circle tests.
- No Delaunay triangulation, Voronoi construction, circle event, or
  natural-neighbour interpolation appears in walls/joins/openings/rooms
  behavior (BEHAVIOR-SPEC sections 3–5).
- If a future algorithm requires incircle (e.g., meshing for 3D), the same
  filter/exact architecture extends with Shewchuk's published incircle
  bound; that would be a new scoped decision, not silent scope growth.

This verdict matches the candidate's own prediction (§11 item 4).

## 2. FMA-independence: mechanism and disassembly record

**Claim:** returned signs do not depend on FMA fusion behavior.

**Structural basis (primary):**
the exact escalation path forms every decision-relevant product with INTEGER
multiplication (`bits.Mul64` two-product fast path, `math/big` general path;
`intexact.go`). Integer operations are never eligible for floating-point FMA
fusion. The filter's conservative bound `(3+16u)·u·(|detleft|+|detright|)`
remains valid under either fused or unfused evaluation of the float
determinant because fusion only removes intermediate roundings, which can
only shrink the covered error; the input subtractions are separately rounded
operations whose errors the derivation already includes (Shewchuk DCG 1997,
§3 filter A; provenance P-1b).

**Bounded compiler observation (secondary evidence):**

- Environment: go1.27.0, linux/amd64 host, package
  `domowik.dev/domowik/internal/geometry/predicates`.
- Commands:
  - `GOARCH=arm64 go test -c -o /tmp/pred-arm64.test ./internal/geometry/predicates/`
  - `go tool objdump -s 'predicates\.(orientFast|filterSign|unitDirectionGated)$' /tmp/pred-arm64.test | grep -iE 'fmadd|fmsub|fnmadd|fnmsub'`
  - `GOARCH=amd64 go test -c -o /tmp/pred-amd64.test ./internal/geometry/predicates/`
  - `go tool objdump -s 'predicates\.(orientFast|filterSign|unitDirectionGated)$' /tmp/pred-amd64.test | grep -icE 'vfmadd|vfmsub|vfnm'`
- Result (2026-08-23): arm64 emits `FMADDD` at direction.go:130 (the
  `sum := sx + sy` step fused with the preceding square) and `FMSUBD` at
  orient.go:106 (the determinant subtraction fused with a product);
  amd64 emits zero fused instructions in these functions.
- Interpretation: fusion DOES occur despite named temporaries — the earlier
  structural claim "named temporaries prevent fusion" was FALSE and is
  retracted from the API contract. Correctness survives via the integer
  fallback plus fusion-tolerant bounds above. Per-op gates check the
  actually computed values (fused or not), consistent with §4.1.

## 3. Section 4.1 gating interpretation (recorded for 0030)

This implementation applies the operand gate LITERALLY AND CONSERVATIVELY:
any original coordinate outside the normal range — including ±zero and
subnormals — escalates to the exact integer path, rather than claiming a
subtraction-exactness exception (e.g., Sterbenz). Consequence: common
origin-anchored triangles take the fallback. Measured cost appears in
BENCHMARKS.md (classed-bits corpus: ~90% filter-inconclusive rate, dominated
by zeros/subnormals/small exponents interacting with the operand gate).
If 0030 prefers the exception-based reading, the gate can be relaxed with
per-operation exactness proofs as follow-up work.

Also recorded: the §4.1 gate additionally rejects when ANY computed
difference/product/determinant is zero-or-subnormal, beyond the paper's
original filter (which assumes normal operands throughout).

## 4. Section 4.2/4.3 fallback limitation (partial-scope status)

`UnitDirection` implements §4.2 steps 1–3 with these documented deviations:

1. **Gate thresholds applied conservatively through exponent differences**
   (`emax−emin ≤ 97` suffices for κ ≤ 2^998 and minor-ratio ≥ 2^-1000).
   Borderline directions escalate instead of gambling on an exact-gate
   reimplementation. Sound (no unsafe fast paths); possibly more fallbacks
   than an exact-gate implementation.
2. **The bypassed/gate-failing route is NOT the candidate's §4.3 canonical
   multiquadratic evaluation.** It computes the direction in 256-bit
   big.Float arithmetic with an OUTWARD rational certificate covering its
   own rounding plus binary64 commitment. This is evidence-grade for the
   gate/certificate obligations but must NOT be represented as full §4.2/§4.4
   candidate compliance. Production §4.3 machinery remains open work,
   tracked by the candidate model's hypothesis register (H-0/H-1 territory)
   and ratification at 0030.
3. Direction inputs enforce the provisional COMPONENTWISE difference bound
   |dx|,|dy| <= 2B via ErrOutOfBounds (the vector norm may reach
   2*sqrt(2)*B and is deliberately not bounded); offset certificates
   enforce B on stored points P and finiteness-only on half-thickness s.
   The derived-coordinate bound D = B + 10^6 mm is NOT enforced here
   (unratified; ratified entity validation owns it).

## 5. Certificate validation methodology

Direction and offset certificates are validated against DIRECTED-ROUNDING
interval enclosures (not round-to-nearest with heuristic margins):

- n* = (dx,dy)/√S is enclosed by interval arithmetic with per-operation
  directed rounding at 512 bits: squares and sums get lower/upper directed
  endpoints; √S_lo rounded down, √S_hi rounded up; each signed division uses
  the numerator over the OPPOSITE denominator endpoint (sign-aware
  monotonicity, review finding C13). Numerators are exact conversions.
- Committed-error bound = max over enclosure-box corners of an upward-only
  corner-distance computation (both rounding directions on each difference,
  upward square/add/sqrt, upward conversion of the exact rational bound).
- Offset ideal vertices use the ORIGINAL (dx,dy) sources, not committed
  components, so certificate terms 3 (direction error × s) and 5 (cross term
  with rounded s) are genuinely exercised (review finding C6).
- Corpus covers extreme-κ directions, mixed-scale pairs such as
  (2^-500, 2^-1074), minimal thickness t = 2η (s = η), zero/near-zero
  points, near-bound points, both σ sides, and randomized power-of-two-scale
  directions (fixed seeds in direction_test.go).

Result: **zero certificate violations** across all corpora (tests assert
bound ≤ ε rigorously; any violation fails the run).

## 6. Adversarial vector provenance

- Fixed naive-failure vectors in predicates_test.go are ORIGINAL fixtures
  from a deterministic lattice construction searched offline (Python,
  seed 20260823, bounded <300k attempts, first hits kept; procedure:
  integer direction components near 2^k scaled by powers of two so stored
  coordinates stay dyadic-exact while naive products exceed 53 significant
  bits and round to equal values, collapsing the naive determinant to 0
  while the exact determinant is ±(2j+1)·scale ≠ 0). No external test data
  copied. A bounded wrong-sign search (500k attempts, seed 7) found no
  strictly-wrong-sign lattice cases; zero-collapse cases suffice to
  demonstrate naive failure and are what the fixed vectors encode.
- The decimal-vs-binary discriminator (0.01, 0.03, 0.05, 0.15) is an
  ORIGINAL fixture proving the oracle evaluates stored values, not decimal
  literals.
- Published-vector note (revised 2026-08-24, supersedes the earlier
  blanket statement): NO vectors were copied from predicates.c or any
  third-party IMPLEMENTATION (P-1c unconsulted). As of the continuation,
  exactly two concrete coordinate triples were adopted from primary
  publication P-3 (Bartels/Fisikopoulos/Weiser, BIT Numer. Math. 63:31,
  2023, CC BY 4.0 — see PROVENANCE.md) into
  TestPublishedAdversarialVectors; every other value remains an
  original as described above.

## 7. Oracle independence statement

The test oracle decodes float64 through shortest-round-trip decimal text
re-parsed with `big.ParseFloat(s, 10, 53, big.ToNearestEven)` (correctly
rounded decimal→binary64, matching strtod semantics), converts exactly to
big.Rat, and SELF-CHECKS the reconstructed bit pattern against the input on
every call, falling back to an independently written bit decoder otherwise.
The oracle classifier is structured independently (Cramer solve + interval
overlap on rationals) and never calls production helpers. Sign comparison
runs through an explicit local table so production enum bugs cannot hide.
TestOracleRejectsDecimalSubstitution proves decimal-rational substitution
would be caught.

## 8. Segment classification scope notes

- Classes and their R3.1.2 mapping are documented on SegmentClass;
  T/S parameters distinguish endpoint-endpoint vs endpoint-on-interior
  contact within ClassTouchEndpoint (a parameter of 0/1 marks endpoint
  involvement), verified by matrix and differential tests.
- Positive-length overlaps carry exact intervals (OverlapT/OverlapS), not a
  single point; T/S are nil there. Reversed coincidence legitimately reports
  OverlapS = [1,0].
- Split-chain orchestration (operation #2 ordering) is out of scope; the
  package exposes exact rational parameters and RatStrictlyBetween
  sufficient for downstream deterministic ordering (R4.3.3 containment
  form covered by evidence helpers).

## 9. Known limitations / follow-ups for 0030

1. Exact fallback performance: scaled-big.Int path allocates (~0.66 us/op
   on the hard-filter-fail corpus and ~0.80 us/op on classed-bits in the
   final recorded run; see BENCHMARKS.md). Expansion arithmetic
   (double-double/bits.Mul64 chains per Shewchuk §5–8) is the natural
   optimization if 0030 demands lower worst-case latency; correctness is
   unaffected.
2. §4.3 multiquadratic machinery absent (see section 4 above).
3. Gate threshold policy (conservative exponent-difference vs exact κ
   computation) is a tunable recorded for 0030.
4. Operand-gate interpretation (literal vs exception-based, section 3 here).
5. android/arm64 portability of this package is a declared hypothesis
   pending demonstration (ADR 0002 Proposed); amd64/arm64 compile clean.

## 10. Continuation record (2026-08-24, evidence-only)

This continuation is EVIDENCE-ONLY (scope S1+S3). Issue 0006 remains In
Progress; issue 0022 remains unapproved/unconsolidated; the candidate model
is NOT ratified. No closure or ratification is claimed.

### 10.1 Contract split

- All five tracker acceptance criteria are otherwise evidenced by the
  integrated commits; AC3's "published adversarial vectors" element was the
  literal gap at start point 126fa59 (the consulted Shewchuk paper P-1b
  publishes bounds/failure modes but no concrete vector table, and
  predicates.c P-1c remains deliberately unconsulted). CONDITIONAL ON the
  two P-3 coordinate triples remaining in tests with their provenance
  record, that narrow AC3 element is now SATISFIED: they are exercised by
  TestPublishedAdversarialVectors with exact stored-binary64 sign
  assertions plus deterministic naive failure-mode documentation (spurious
  zero unfused in both common groupings; strictly wrong sign under FMA
  contraction via math.FMA). Full provenance: PROVENANCE.md entry P-3 and
  revised Test-vector provenance / FINDINGS §6. The paper's overflow
  example (2^800) is outside candidate bound B and is deliberately not
  adopted as an in-domain test. No issue closure or model ratification
  follows from AC3 satisfaction.
- NUMERICAL-MODEL §4.3 canonical multiquadratic evaluation remains ABSENT
  and is an EXPLICIT issue-0030 decision gate (H-0 unmeasured): the current
  integrated fallback path provides scaled big.Int exact dyadic predicate
  handling and a big.Float(256) direction fallback with outward rational
  certificate only. Per the 0022 pair's classification this is not an
  owned 0006 code deliverable; no silent substitution occurred or is
  claimed.

### 10.2 Observed evidence, not settled policy (0030 owns define-before-judge)

- The ≤97 exponent-difference fast-path gate and the literal §4.1 operand
  gate (~90% classed-bits escalation) are OBSERVED evidence of the current
  implementation, not settled policy; both interpretations remain open for
  0030.
- Exact-path metrics remain threshold-free (BENCHMARKS.md); no pass/fail
  judgment is made or implied.
- Release-inventory/SBOM status: start point 126fa59 predates the current
  main's valid refresh (its exact-tree SPDX files are stale by construction
  because ED-0010 followed commit 71e8544). Integration therefore merges
  main 466afca into this branch after the issue edits are committed,
  regenerates all SPDX documents through `domowik-sbom generate` only,
  commits generated outputs separately, and requires ./scripts/verify.sh
  exit 0 from the final clean committed tree. No green verification is
  claimed from 126fa59.
- Fresh checks on this tree (2026-08-24, rerun after the final test/doc
  edit of this continuation): `go test -race -count=1
  ./internal/geometry/predicates` PASS (`ok ... 3.995s`). Shuffle/`-count`
  probes exercise TEST SCHEDULING AND ORDER INDEPENDENCE only; they are NOT
  a demonstration of arbitrary API input-permutation determinism, and no
  source ordering contract was changed by this continuation. Predicate
  outputs themselves are deterministic pure functions of the stored input
  bits (doc.go); scheduling probes say nothing additional about them.
  Benchmark denominators: orientation `%inconclusive` uses the b.N
  iteration denominator while cycling its fixed 256-case corpora;
  intersection corpora are 256 segment pairs;
  BenchmarkOffsetCertificateAndVertex iterates over benchDirs = 256
  directions and each iteration covers UnitDirection construction PLUS
  OffsetVertexCertificate PLUS OffsetVertex (the BENCHMARKS.md row omits
  the corpus-size field; the denominator is 256 directions). No
  performance threshold is claimed or rerun beyond this record.

### 10.3 Platform status

- `CGO_ENABLED=0 GOOS=android GOARCH=arm64 go test -c -o
  /tmp/opencode/pred-android-arm64.test ./internal/geometry/predicates`
  exits 0 on this tree (ELF 64-bit LSB pie executable, ARM aarch64,
  interpreter /system/bin/linker64), extending the amd64/arm64 compile
  record. This is COMPILE evidence only; android/arm64 runtime portability
  remains an undemonstrated hypothesis (ADR 0002 Proposed).

### 10.4 Cross-pair flags relayed to 0030 (RELAYED-NOT-VERIFIED by 0006)

Attributed to the 0022/0007 pairs; not validated by this issue and not part
of 0006 evidence:

- 0007 oracle simple-ring/pinch normalization gap
  (oraclebool.go chainCycles/pickNext, cyclesToPolySet) makes the reported
  2/12 B-1x/B-1q mismatch an oracle-integrity rerun gate rather than a
  one-sided candidate verdict.
- A direct permutation probe found CandFaces input-order nondeterminism
  (single-room shuffled segments losing Exterior.Holes) despite race-clean
  and shuffle-passing suites.
