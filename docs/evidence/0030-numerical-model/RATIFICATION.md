# Issue 0030 Ratification Record — Numerical Geometry Model

Status: **Disposition record of issue 0030 — issue outcome PARTIAL / NOT
COMPLETE** (see §8 for the explicit reasons). This document records what the
evidence supports, what remains open, and which decisions are made. It does
NOT ratify NUMERICAL-MODEL.md as a whole, does NOT promote any technology
choice to Accepted, and does NOT alter normative BEHAVIOR-SPEC semantics.
ADR 0002 remains Proposed.

Branch base: main `4356ef0fd85a0762dc921041ca5143ab3aeeed74` (post-0007
integration).

### Exact commit table

Full 40-character SHAs for key commits cited in this record (pinned at
authoring time; commits after this file's own final revision are listed by
subject in the branch history and cannot carry their own hash):

```
4356ef0fd85a0762dc921041ca5143ab3aeeed74  branch base (main, post-0007 integration)
126fa59e5ac81c267dbdac857a6a1b848dfd54fd  0007 recorded comparison base
f0dad766a6b37138ee4bef1a627f0ad0f05654a0  0007 tested source commit (pinch normalization)
6add04be3ce7451f05489790dfdd7131ce007dce  criterion pre-registration
2fa0ebb03c5ba9665b184ad7e55af950cb6a2276  criterion amendment (NOT-JUDGEABLE verdict)
2fadb5cea974d9ac99a938e6f9cd2de4b289a037  criterion A4 RSS pinning
a7edadbe662d267520cee4cca7501e2fcfb0c442  Direction validation tightening (fix)
```

Subsequent commits touching this record, the criterion addendum freezing the
W-B/A1 judged value, ADR 0002's gate-progress record, and any SBOM
regeneration follow in branch history with Conventional Commit subjects.

## 0. Consumed handoffs and their exact scope

- **0022** (NUMERICAL-MODEL.md): the Candidate proposal itself, consumed as
  unratified input with all technology choices Proposed.
- **0006**: evidence producer for robust predicates
  ([FINDINGS.md](../0006-predicates/FINDINGS.md),
  [BENCHMARKS.md](../0006-predicates/BENCHMARKS.md),
  [PROVENANCE.md](../0006-predicates/PROVENANCE.md)). Its §4.3 canonical
  multiquadratic machinery is ABSENT; H-0 cost is UNMEASURED; its §10.2
  explicitly leaves gate-policy and operand-gate interpretations open.
- **0007**: partial-scope handoff at tested source commit `f0dad76` under
  ED-0011. Only its boolean/oracle work is consumable:
  - The 0/12 topology-mismatch rerun (§5) is **POST-NORMALIZATION
    AGREEMENT**: candidate and oracle apply analogous deterministic
    split/validate normalization, so it is not a wholly independent
    end-to-end pinch proof.
  - Semantic anchors remain `TestBooleanPinchSemantics` (literal
    oracle-derived pinch fixtures) and `TestOracleBooleanRawPinchCycles`
    (raw pre-normalization repeated-node cycles).
  - B-1q runtime certify-or-escalate (NUMERICAL-MODEL §7) is
    **UNIMPLEMENTED / NONCONFORMING / SELECTION-BLOCKING** for any
    quantized strategy; measured rates cannot waive this.

## 1. Define-before-judge: Phase 0 performance criterion

Pre-registered at `6add04b`, amended after adversarial review at `2fa0ebb`
and `2fadb5c`:
[PHASE0-PERFORMANCE-CRITERION.md](PHASE0-PERFORMANCE-CRITERION.md)
(countersigned conditionally by rev0030 against the criterion blob at
commit `2fadb5cea974d9ac99a938e6f9cd2de4b289a037`, i.e.
`6693cc938fbf7a58499fceda890a74234b375a42`; that is the PRE-addendum-2
blob, not the current file hash. The CURRENT criterion blob after the
W-B/A1 judged-value freeze is
`7f4ac272174abf6ef04147dda63794b40be527ae`, reviewed and accepted
conditionally by rev0030 in pair communication).

**Verdict recorded there and binding here: existing recorded performance
evidence (0006 single-run means; 0007 aggregate boolean timing with operand
construction inside the timed loop) is NOT JUDGEABLE under the registered
protocol. The Phase 0 performance gate is OPEN.** No pass or fail is claimed
from old numbers. Any compliant rerun must show per-case retained-process
medians, the pre-declared p95/max tail definition, GNU-time per-case RSS,
preassigned corpus classes, and qualifying runtime fallback counts.

## 2. Correctness, determinism, range, quantization, overflow: summarized

- **Predicates (0006).** Adaptive orientation with exact integer fallback;
  zero certificate violations across adversarial corpora against directed-
  rounding enclosures; FMA-independence established structurally (integer
  escalation path) after retracting a false "named temporaries prevent
  fusion" claim; segment classification with exact C1 parameters;
  published-vector use limited to two attributed P-3 triples. incircle not
  needed for v0.1 planar work. Limitations: §4.3 machinery absent;
  conservative literal §4.1 operand gate (~90% classed-bits escalation) is
  observed evidence, not settled policy.
- **Booleans (0007).** B-1x revision 2: 0/12 post-normalization topology
  mismatch vs independent oracle on the named 12-case corpus, exact-zero
  measured squared vertex error on integer-grid corpus and on the non-grid
  probe; reversal/permutation invariances tested at boolean-ring level.
  Independence caveat of §0 applies. B-1q: deliberately nonconforming
  baseline; non-grid deviation ≈ 5.08e-6 mm² (exact fraction recorded);
  ineligible for selection regardless of numbers.
- **Faces (0007).** NO candidate-face correctness, determinism, or
  performance claim exists (ED-0011); see §3.
- **Determinism policy** (NUMERICAL-MODEL §8) stands unratified but
  unrefuted: no cross-platform divergence was demonstrated; amd64/arm64
  compile clean; android/arm64 runtime portability remains an undemonstrated
  hypothesis.
- **Overflow/range derivations** (NUMERICAL-MODEL §3) were reviewed and are
  arithmetically sound as PROPOSALS (int64 product bounds, big.Int
  carriers, D = B + 10⁶ derived bound); they are NOT ratified here.

## 3. Candidate face extraction: EXPLICITLY REJECTED (flat form)

Under the authority delegated by ED-0011's decision branch ("explicitly
reject face extraction as a selected candidate"), issue 0030 rejects the
flat `CandFaces([][2][2]float64)` segment-soup extractor as a selected
approach:

1. The flat input transformation T provably discards ring direction,
   multiplicity, and operand/ring provenance; ambiguities P1 (nested filled
   island vs hole) and P2 (seam vs retraced boundary) are information-loss,
   not implementation defects, so no classifier over T's output can be
   conforming (0007 evidence §12).
2. The independently reproduced permutation defect (12/12/2 vs 16/10/1
   fragments/pieces/cycles under seed-1 shuffle+reversal at base `126fa59`)
   is retained as defect provenance.
3. All prior face conformance/determinism/performance claims remain
   WITHDRAWN. Existing characterization tests/benchmarks
   (`TestCorpusTopologyMatchesOracle`, `TestCandFaces*`,
   `BenchmarkCorpusCandFaces`) stay in-tree as regression coverage of the
   unchanged baseline on canonical-direction inputs ONLY; none of their
   results may appear in conformance, determinism, performance, H-6, or
   strategy-selection tables.
4. This rejection is scoped to the flat candidate form. It does not foreclose
   a future planar-face capability; ANY future work requires first a
   normative, testable, provenance-preserving STRUCTURED face-input contract
   that retains ring/operand roles and multiplicity and demonstrably
   distinguishes holes, nested filled components, duplicates, seams,
   contacts, and invalid rings under input permutation (ED-0011 validation
   gate), followed by its own spike evidence, tests, provenance record, and
   selection judgment. There is no silent deferral: the flat candidate is
   rejected now, and structured work starts only through that gated path.

## 4. Boolean strategy status

- **B-1x**: leading candidate architecture (exact-event sweep, nonzero
  winding, single terminal binary64 commitment). NOT SELECTED at Phase 0.
  Selection stays blocked pending: (a) resolution of the post-normalization
  agreement caveat via an independent end-to-end pinch/normalization proof
  or protocol; (b) compliant performance reruns under the registered
  criterion; (c) H-3 multi-stage accumulation evidence. Any phrase reading
  "B-1 pipeline" in evidence refers to B-1x-candidate-only measurement and
  must not be read as accepting quantized B-1q or settling technology.
- **B-1q**: NONCONFORMING (no runtime certify-or-escalate), therefore
  SELECTION-BLOCKING and permanently ineligible unless a future revision
  implements per-invocation certification with `geometry-indeterminate`
  budget behavior and re-evidences under new gates. The int64/Mul64 study
  was never performed.
- **B-2 (Clipper2)**: unchanged; CITE-VERIFIED-NOT-CONSULTED with an
  incomplete third-party record; conflicts with pure-Go core preference via
  native binaries. Not a candidate.

## 5. Hypothesis register disposition

| Hypothesis | Disposition at 0030 |
|---|---|
| H-0 algebraic sign-evaluation cost | OPEN — §4.3 machinery absent; cost unmeasured |
| H-1 intersection chain beyond C1 | OPEN — no derivation-backed bound beyond commitment |
| H-2 hull evaluation | OPEN — true convex-hull stage unimplemented (rect proxies); flagged toward tracker 0013 |
| H-3 boolean vertex generation | PARTIALLY EVIDENCED — single-terminal-commitment supported on corpus; multi-stage accumulation unexercised |
| H-4 face assembly | OPEN/WITHDRAWN — no candidate-face assembly claim exists |
| H-5 area-chain propagation | OPEN — untouched; display certification path unevidenced |
| H-6 fallback/escalation counts | OPEN — CandFaces counters excluded (ED-0011); OracleFaces counters reference-only; boolean rejects/op=0 captured runs only; runtime certification unimplemented |

None of these may be summarized as passing. Each blocks promotion of the
affected guarantees.

## 6. Coordinate bounds, diagnostics, budgets, API guarantees: NOT APPROVED

The following remain PROPOSED/OPEN exactly as written in NUMERICAL-MODEL.md
§3/§9 and are neither approved nor ratified by this record:

- coordinate bound B = 10⁹ mm (vs 2³¹−1 alternative), derived-coordinate
  D = B + 10⁶ mm, dimension caps 10⁶ mm, level/room field domains;
- proposed diagnostics `coordinate-out-of-range`, `dimension-out-of-range`,
  `geometry-indeterminate`, `area-tie-indeterminate`;
- commitment-error budgets, ε̄_A enclosure guard precision, expectation-
  encoding tiers, escalation/guard-precision policy;
- split-point persistence (nearest-float vs exact rational storage) and the
  final boolean-strategy choice (§12 open points);
- every exported geometry API guarantee. Evidence-only packages establish no
  production contract. Specific unresolved API limitations recorded from
  review of source at `a7edadb`: `predicates.Direction` exports NX/NY/Path/
  Cert; `validateDirection` performs structural checks only and CANNOT
  authenticate provenance — a forged band-consistent vector (e.g. norm 0.5
  with Cert 0.5, or the forged zero vector pre-fix) can pass; the Cert<1
  tightening (`a7edadb`) closes the degenerate-band hole only and is NOT a
  certificate-authentication fix. FORMAT-SPEC L14 numeric-bound delegation
  and BEHAVIOR-SPEC R3.0.8's precision/tolerance deferral remain unresolved
  pending ratification-grade evidence.

Approving any of these requires the open gates to close and a new explicit
ratification step; nothing is promoted by implication.

## 7. Provenance and licensing conclusion (scoped)

- Clean-room posture: credible within recorded scope. Actually consulted
  artifacts: the P-1b paper (concepts only; independent implementation;
  predicates.c P-1c deliberately unconsulted), stdlib L-1, and exactly two
  CC BY 4.0 coordinate triples from P-3 with attribution. Ledger citation
  rows B-1a/B-1b/B-1c are retained as CITE-VERIFIED-NOT-CONSULTED records —
  those publications were NOT consulted. No GPL artifact consulted;
  quarantined entries untouched; 0007 normalization machinery is an
  independent first-principles implementation.
- NOT closed: P-3's distribution classification is a PROPOSED scoped
  classification with no settled release-inventory registry row or SBOM
  package entry; a future release-policy review may reclassify it with
  registry/NOTICE/SBOM consequences. Ledger statuses elsewhere remain as
  recorded (QUARANTINED/CITE-VERIFIED entries uncleared). This record makes
  no claim that license/provenance gates are fully closed.
- Generated-only rule: SPDX/SBOM regeneration after substantive changes is
  produced by `domowik-sbom generate` exclusively and committed separately.

## 8. Disposition summary

**Issue 0030 outcome: PARTIAL / NOT COMPLETE.** The issue's acceptance
criteria are not fully met, and this is the honest final state of the
evidence rather than an omission:

- Performance: **FAILED FOR RECORDED SCOPE / gate open for §11 trigger-3
  disposition** — historical 0006/0007 records are NOT JUDGEABLE; a compliant
  v3 rerun now exists ([PHASE0-PERFORMANCE-RERUN.md](PHASE0-PERFORMANCE-RERUN.md))
  and supersedes any earlier "no compliant rerun exists on this branch"
  statement. Under the registered rule the criterion FAILED for that scope on
  the A3 robust breaches (`BenchmarkIntersectSegmentsRobust` median 12779.5 ns/op,
  `BenchmarkOffsetCertificateAndVertex` median 20949.0 ns/op, both above the
  10000 ns/op bound); other applicable thresholds were met as recorded.
- B-1q: **selection-blocking** (runtime certify-or-escalate unimplemented,
  nonconforming); int64/Mul64 study never performed.
- Face extraction: flat candidate **rejected** with NO structured
  replacement specified or evidenced on this branch — planar room topology
  therefore has no conforming candidate path.
- Open gates carried forward: H-0, H-1, H-2, H-5, H-6 (H-3 partial), the
  post-normalization independence caveat, unratified coordinate bounds/
  diagnostics/budgets, unresolved API guarantees (Direction provenance),
  FORMAT-SPEC L14 and BEHAVIOR-SPEC R3.0.8 deferrals.
- ADR 0002: **remains Proposed**; gate 1 recorded NOT PASSED/OPEN.

Per-issue dispositions recorded here are inputs to tracker synchronization;
the tracker itself is edited only by the orchestrator.

1. Numerical model: **NOT RATIFIED; revised-and-continued as Candidate**
   with the recorded partial support above. Spikes need not be rerun in
   full solely because of this record; the mandatory reruns are those named
   in §1/§5 gates when their owning work resumes.
2. Phase 0 performance gate (ADR 0002 gate 1 component): **FAILED for the
    recorded v3 scope** (A3 robust breaches; see
    [PHASE0-PERFORMANCE-RERUN.md](PHASE0-PERFORMANCE-RERUN.md)); the
    ratification/promotion gate remains OPEN for the NUMERICAL-MODEL §11
    trigger-3 revision decision. Historical 0006/0007 evidence remains
    NOT JUDGEABLE.
3. Flat candidate face extraction: **REJECTED** (§3); face capability
   requires the ED-0011 structured-contract gate before any implementation.
4. ADR 0002: gate-status and maintenance-burden updates applied in
   `docs/decisions/0002-go-core-cli-first.md`; ADR remains Proposed.
5. Numeric bounds/API guarantees: remain proposed/open (§6).
6. Tracker: no tracker edits in-repository; see the outcome statement above.

Review trail: rev0030 (GPT-5.6 Luna) reviewed the criterion registration
(countersignature finding), the Direction-validation findings, and the
checkpoint constraints incorporated above; final approval is recorded in the
pair communication log once granted.

## Addendum 2026-08-24 — Phase 0 performance v3 rerun (scope distinction)

This addendum supersedes any earlier reading that no compliant rerun exists.
Two evidence generations are distinguished:

1. **Historical 0006/0007 recorded evidence remains NOT JUDGEABLE** under the
   registered protocol (single-run means, aggregate boolean timing with operand
   construction inside the timed loop, no RSS measurement, no qualifying H-6
   counts).
2. **The v3 rerun is judgeable for its recorded scope.** It is recorded in
   [PHASE0-PERFORMANCE-RERUN.md](PHASE0-PERFORMANCE-RERUN.md) from raw
   artifacts under `build/0030-performance-raw-v3/`. Under the registered rule
   the Phase 0 performance criterion **FAILED for that recorded scope** on the
   A3 robust threshold (≤ 10000 ns/op median): `BenchmarkIntersectSegmentsRobust`
   median 12779.5 ns/op and `BenchmarkOffsetCertificateAndVertex` median
   20949.0 ns/op both exceed the bound. All other applicable thresholds were
   met as recorded, and H-6 MISSING domains persist (boolean pipeline,
   CandFaces, OracleFaces, B-1q certification: absent from the evidence, never
   zero).

Countersigned v3 artifact identity: analysis REPORT.txt sha256
`92854f17a35c19c6344efb386611af3c952fad427c284eca1d722fe2eae7c15d`;
PROVENANCE.txt sha256
`5b61917a441bccd5328abdb387e105b3c8b6d820727aade2eb39a6f735090f03`;
ANALYZER-HASH.txt sha256
`4bb835e17c85f7bf89fc2ef167535bc00a4fd284930141bee88b9b7a5ce776d5`;
canonical snapshot sha256
`7f8aca35d7dbcdaf7d5ac575cf47f576285e8df75b3e9c59e815c3bc668cdd02`;
measured test binary sha256
`13c91b1e13fc071327cb9b21795d9d4793d0020ff05a4e7233d5e98eddc0fb8a`.

This addendum changes no registered threshold or protocol rule, ratifies
nothing, and leaves the numerical model NOT RATIFIED, ADR 0002 Proposed,
PARTIAL / NOT COMPLETE in force, and the ratification/promotion gate OPEN for
the NUMERICAL-MODEL §11 trigger-3 revision disposition.

## Addendum 2026-08-24 — §11 trigger-3 disposition recorded

The NUMERICAL-MODEL §11 trigger-3 assessment left open by section 8 and the
v3 rerun is now recorded in
[TRIGGER3-DISPOSITION.md](TRIGGER3-DISPOSITION.md): no caching/locality
strategy per R5.1.9 can plausibly close the recorded A3 robust gap, so
trigger 3 FIRES. Per the registered failure-handling rule, the ADR-0002
revision-trigger assessment is recorded as a RECOMMENDATION ONLY (see
`docs/decisions/0002-go-core-cli-first.md`); any stack reassessment is an
owner decision, and a revised candidate model under a new pre-registered
criterion remains the alternative path. This addendum changes no threshold
or protocol rule, ratifies nothing, closes no H-register item or open gate,
and leaves every scope guard above in force.
