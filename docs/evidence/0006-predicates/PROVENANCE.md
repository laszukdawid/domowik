# Issue 0006 Provenance — Robust Predicates

Status: evidence for the Candidate numerical model (unratified; issue 0030
owns ratification). This document completes the P-1 ledger rows flagged
PINNED-PENDING-STUDY in NUMERICAL-MODEL.md §10 to the extent issue 0006's
scope consumed them. It does not modify NUMERICAL-MODEL.md.

## Consultation log — P-1b (paper)

- **Publication:** Jonathan Richard Shewchuk, *Adaptive Precision
  Floating-Point Arithmetic and Fast Robust Geometric Predicates*,
  Discrete & Computational Geometry 18:305–363 (1997); also CMU-CS-96-140(R).
- **Artifact consulted:** author-distributed PDF at
  https://people.eecs.berkeley.edu/~jrs/papers/robustr.pdf (HTTP 200,
  checked 2026-08-23). Front matter states the paper may be duplicated and
  distributed for noncommercial/educational purposes with this notice
  preserved; this log constitutes that notice-carrying record.
- **Sections used:** §3 (orient2d filter A error bound derivation), and the
  general adaptive filter/exact-fallback architecture concept (§1–2, §5).
- **What was taken:** the mathematical filter-bound formula
  `(3+16ε)·ε·(|detleft|+|detright|)` and the architecture pattern
  (cheap float filter, escalate undecided signs to exact arithmetic).
- **What was NOT taken:** no source code, no pseudocode transcription, no
  expansion-arithmetic implementation (this spike uses scaled-big.Int
  escalation instead of Shewchuk expansions), no test vectors.
- **Adaptation type:** independent implementation from published
  mathematical description. Constants re-derived and documented in code;
  formula attributed in comments.

## Consultation log — P-1a (provenance hub page)

- https://www.cs.cmu.edu/~quake/robust.html (HTTP 200, checked 2026-08-23).
- Used only as the hub locating the paper and the public-domain statement
  about predicates.c. No code or text from the page was copied.

## Consultation decision — P-1c (predicates.c): DELIBERATELY UNCONSULTED

- http://www.cs.cmu.edu/afs/cs/project/quake/public/code/predicates.c
  (revision 1996-05-18; URL live-checked 2026-08-23, HTTP 200).
- Although the code is stated on P-1a/P-1c pages to be public domain, we
  did NOT open, read, download its contents for study, translate, adapt, or
  derive from it. Rationale: minimize similarity risk under
  CLEAN-ROOM-POLICY.md ("Review similarity") even where licensing would
  permit use; the paper alone suffices for the obligations delegated to
  this issue.
- Status update for the §10 ledger: P-1c remains UNCONSULTED and
  NON-RELIED-UPON by issue 0006 (no completed code study occurred; none is
  claimed).

## Consultation log — P-3 (Bartels/Fisikopoulos/Weiser 2023): adopted vectors

- **Publication:** Tinko Bartels, Vissarion Fisikopoulos, Martin Weiser,
  *Fast floating-point filters for robust predicates*, BIT Numerical
  Mathematics 63(2), article 31 (2023), published 2023-05-17, DOI
  10.1007/s10543-023-00975-x,
  https://link.springer.com/article/10.1007/s10543-023-00975-x.
- **License basis:** open access under Creative Commons Attribution 4.0
  International (CC BY 4.0), https://creativecommons.org/licenses/by/4.0/,
  stated on the Springer version-of-record page.
- **Artifacts consulted (2026-08-24):** Springer version-of-record page
  (metadata, abstract, license statement, and its own excerpt of the
  underflow example); full text verified through the authors' preprint
  arXiv:2208.00497v1 (submitted 2022-07-31,
  https://arxiv.org/abs/2208.00497, arXiv perpetual non-exclusive license),
  whose section 2 carries the identical examples.
- **What was used:** exactly two concrete coordinate triples, transcribed as
  numeric test fixtures only:
  1. near-collinear decimal example `a = (-0.01,-0.59), b = (0.01,0.57),
     c = (0,-0.01)` (paper §2);
  2. underflow example `(2^-801,2^-801), (2^-800,2^-800),
     (2^-801,2^-800)` (paper §2).
- **What was NOT used:** no algorithms, filter constructions, code,
  pseudocode, figures, or prose. The paper's real-arithmetic collinearity
  framing of vector 1 is deliberately NOT asserted as Domowik semantics;
  the tests decide the exact sign over stored binary64 values instead
  (NUMERICAL-MODEL.md §7 conformance protocol) and separately document the
  naive float failure modes (spurious zero unfused; wrong sign under FMA
  contraction via math.FMA).
- **Adaptation type:** factual test-data transcription with attribution;
  independent expected-value derivation performed with Domowik's own exact
  rational oracle, not taken from the paper.
- **Distribution status / third-party policy review (scoped classification
  PROPOSAL, not a settled release-inventory decision):** under
  THIRD-PARTY-POLICY.md review, the two-triple numeric fixture is PROPOSED
  as NOT a linked or bundled runtime component: it is test data embedded in
  an Apache-2.0 source file, and on that PROPOSED basis no registry row was
  added and no SBOM package entry arises from P-3 itself in the current
  tree. Issue 0006 holds no authority to settle distribution classification
  under THIRD-PARTY-POLICY.md or RELEASE-INVENTORY.md. Attribution and
  license identification are retained in source comments and this log;
  CC BY 4.0's attribution and change-indication expectations are honored by
  naming the source and the transcription/adaptation in both places. No
  legal conclusion is asserted beyond this scoped classification; if a later
  release-policy review reclassifies the fixture as distributed content,
  registry/NOTICE/SBOM consequences return to that review. The paper's
  overflow example (2^800 coordinates) lies outside the candidate coordinate
  bound B and was not adopted as an in-domain test.

## Other consulted material

- Go standard library `math/big`, `math/bits`, `math` (go1.27.0):
  BSD-3-Clause, Copyright (c) 2009 The Go Authors. Used unmodified;
  adaptation type NONE. No third-party Go modules added (module graph
  remains single-module per scripts/verify.sh guard).
- P-3 (Bartels/Fisikopoulos/Weiser 2023, see consultation log above) is the
  only other consulted artifact; its use is limited to the two transcribed
  coordinate triples. Clipper2 (B-2), Martínez et al. (B-1a),
  Guibas–Marimont (B-1b), de Berg et al. (B-1c), and Yap (P-2) remain
  unconsulted by THIS issue; their statuses are unchanged and their study
  belongs to issue 0007 where relevant.

## Test-vector provenance

- Two concrete test inputs are ADOPTED PUBLISHED vectors from primary
  publication P-3 with full attribution (see the consultation log above and
  the attribution comments in predicates_test.go's
  TestPublishedAdversarialVectors): the near-collinear decimal triple and
  the underflow powers-of-two triple. Their expected signs are derived by
  Domowik's own exact rational oracle over stored binary64 values.
- All other concrete test inputs are ORIGINAL self-generated fixtures OR
  FIRST-PARTY normative-behavior fixtures derived from Domowik's own
  specifications and reviewed documents (for example NUMERICAL-MODEL §3
  fixture shapes); there are NO third-party vectors beyond the two P-3
  triples above:
  - fixed-seed deterministic generators (seeds recorded inline:
    101–104, 555, 777 superseded, 20260823 segment differential seed,
    4242 gate consistency, 90210 direction corpus, 31337 offset corpus,
    60606 comparator oracle cross-check);
  - hand-built analytic fixtures with documented derivations (class-matrix
    segments; half-ulp contract table; touching-fit forms);
  - offline-searched adversarial orientation vectors (procedure, seeds,
    and bounds documented in FINDINGS.md §6; search performed in original
    scratch code, not from any external suite).
- P-1b contributed CONCEPTS (failure modes, bound formula), not vectors.
  P-3 contributed exactly the two attributed coordinate triples above; no
  other vector set from any publication or implementation was copied, and
  predicates.c (P-1c) remains unconsulted.

## Clean-room confirmation

To the best of this agent's knowledge and recorded process:

- **No GPL-licensed source code, tests, data formats, or fixture files were
  consulted, downloaded-for-study, translated, adapted, or derived from** —
  including CGAL, Sweet Home 3D, DobryDom3D, GPC, Boost.Polygon, GEOS,
  JTS, or any floor-plan application.
- Quarantined entries X-1/X-2/X-3 (NUMERICAL-MODEL.md §10) were not
  accessed.
- All implemented algorithms trace to the single consulted publication
  above plus standard library facilities, or are original derivations
  documented inline (interval enclosures, certificate builders, integer
  scaling path, oracle construction).
- AI-training-context caveat per CLEAN-ROOM-POLICY.md: an LLM cannot prove
  its training contains no similar material; this record documents the
  consultation process relied upon, not a guarantee of independence.
