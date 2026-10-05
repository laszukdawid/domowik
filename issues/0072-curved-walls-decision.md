# 0072: Decide and specify curved walls

- **Phase:** 0
- **Priority:** Low
- **Status:** Open
- **Depends on:** 0001, 0004, 0019, 0022, 0030

## Problem

BEHAVIOR-SPEC R3.0.1 defines a wall axis as a single straight segment and
`unsupported-wall-shape` rejection for anything else; §3.6 lists curved walls
among the v0.1 exclusions; opening rules assume straight hosts; and
FORMAT-SPEC's `wall` entity has no field that could carry curvature. A person
modeling a real building reasonably expects curved walls, so the exclusion
should be re-examined as an explicit decision rather than discovered as a
missing feature. Requested during the 2026-10-01 editor review.

## Scope

Produce a decision record and, only if curvature is accepted, the normative
specification and format changes it requires. The decision must cover at
minimum: authoritative representation (arc by bulge, sagitta, or three-point
form) and its precision; contact, split, and join rules for arc axes;
opening hosting and the `u` parameterization (arc length versus chord);
hit-testing and rendering; the numerical predicates arcs need, owned by
0022/0030; and compatibility, including the prohibition on silently
reinterpreting existing files. Implementation is explicitly NOT in scope
before the specification is ratified. Any adopted algorithm must record
license-reviewed, clean-room provenance.

## Acceptance criteria

- [ ] A decision record states whether curved walls enter a future version,
      with rationale and the consequences for R3.0.1 and the §3.6 exclusions.
- [ ] If accepted, BEHAVIOR-SPEC and FORMAT-SPEC clauses are drafted with
      fixtures and a compatibility assessment; if rejected, the rationale is
      recorded where the next reader will find it.
- [ ] Any adopted algorithm has recorded, license-reviewed provenance.

## Closure

- **Implementation/spec commits:**
- **Follow-up issues:**
