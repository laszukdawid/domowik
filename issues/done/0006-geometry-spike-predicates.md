# 0006: Spike required robust predicates in Go

- **Phase:** 0
- **Priority:** High
- **Status:** Done
- **Depends on:** 0022
- **Deferred-closure gates:** 0030 (canonical section 4.3/H-0 fallback and
  numerical-model policy ratification)

## Problem

Implement only predicates required by the selected numerical design, citing
exact publications/source artifacts and licenses. Candidate needs include
orientation and robust segment intersection; `incircle` is included only if a
chosen algorithm needs it.

## Acceptance criteria

- [x] API states finite-input policy, exact sign semantics, coordinate bounds,
      and orientation convention.
- [x] Exact oracle uses rational representation of input float values (not an
      unspecified `big.Float` precision).
- [x] Tests include published adversarial vectors plus generated scale,
      translation, near-collinear, coincident, and non-finite cases.
- [x] Benchmarks record Go version, hardware, corpus, allocations, and
      comparison with the simple float implementation.
- [x] Provenance identifies paper and any source consulted, adaptation type,
      license, notices, and confirmation that no GPL implementation was
      consulted or used.

## Integrated evidence handoff — 2026-08-24

Commits `5ab3c31` and `020056c` were integrated into the main repository by
merge commit `c9936d7`. The executor and reviewer approved the implementation
as an evidence-only partial handoff; `./scripts/verify.sh` passes on the
combined integration tree.

The issue remains In Progress. The canonical multiquadratic fallback required
by `NUMERICAL-MODEL.md` section 4.3 is not implemented, and the candidate
model remains unratified. The conservative gate threshold, literal section
4.1 operand-gate interpretation, exact-path performance envelope, and
Android/arm64 hypothesis are recorded in
`../docs/evidence/0006-predicates/FINDINGS.md` for continuation and issue 0030.

## Continuation evidence handoff — 2026-08-24

Commit `43fbd87` added two eligible, provenanced primary-publication
adversarial vectors and corrected the opening-width evidence terminology. It
was independently approved at branch head `8b2e137`, integrated by the main
repository's issue-0006 merge, and followed by a clean full verification run.

All evidence-producer acceptance criteria above are now satisfied. Closure is
still deferred to issue 0030 for the explicitly out-of-scope section 4.3/H-0
fallback and numerical-model policy decisions; this handoff does not ratify
thresholds, performance criteria, representation policy, or Android runtime
behavior.

## Rust delivery — 2026-08-31

`crates/domowik-geometry`'s `orientation` is the adaptive filtered predicate
this spike selected: a floating-point determinant with a proven error bound
(Shewchuk 1997 §2.2, cited at the constant), escalating to an exact evaluation
by error-free transformations — `two_product` via `f64::mul_add`, and
`two_sum` — with no allocation and no arbitrary-precision arithmetic.

Measured 1.15 ns filtered, 1.79 ns on the exact fallback. The fallback figure
is the load-bearing one: floor plans are full of axis-aligned and collinear
input that lands on it, and the Go implementation's unconditional exact path
cost 12,134 ns for the same answer.

Segment intersection is delivered as `arrangement::intersection_param`, which
decides EXISTENCE with the exact predicate before computing a position.
`incircle` is not needed by any algorithm here and is not implemented.

The ED-0011 fallback policy and numerical-model ratification this issue gates
on remain open under issue 0030.

## Closure — 2026-09-12

**Done.** Every acceptance criterion above was satisfied by recorded evidence,
and the single deferred-closure gate no longer names a live obligation.

- **The gate is discharged, not deleted.** The `Deferred-closure gates:` line
  points at issue 0030 for "canonical section 4.3/H-0 fallback and
  numerical-model policy ratification". ED-0100 SUPERSEDED NUMERICAL-MODEL
  §§4.2–4.5 — the canonical multiquadratic fallback was candidate machinery for
  a Go core that was never built, and nothing in those sections is normative.
  The policy half of the gate is discharged by ED-0103, which ratifies
  NUMERICAL-MODEL §12 in full. Neither half is an unmet requirement that was
  written off.
- **The spike's subject shipped.** `domowik-geometry::orientation` is the
  adaptive filtered predicate this spike selected: a floating-point determinant
  with a Shewchuk 1997 §2.2 error bound, escalating to an exact evaluation over
  stored coordinates. `incircle` is not needed by any algorithm here and is
  not implemented, as the Problem statement allowed.
- **Provenance and licences** for the shipped predicate are recorded in
  NUMERICAL-MODEL §14. No implementation of any cited algorithm was consulted
  and no GPL floor-plan source was used.
- **Residual, and where it now lives.** None against this issue. The exactness
  of CONSTRUCTIONS — as distinct from the exactness of the predicate's
  DECISIONS — is settled by ED-0103 N-3 and is not a predicate obligation.
