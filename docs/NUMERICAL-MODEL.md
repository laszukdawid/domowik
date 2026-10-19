# Numerical geometry model

This is the current numerical contract. The
[behavior specification](BEHAVIOR-SPEC.md) defines product semantics; this page
distinguishes exact decisions over represented values from derived topology
and construction rounding. It does not ratify unimplemented alternatives.

## 3. Coordinate domain and validation

- Coordinates and dimensions are finite IEEE-754 binary64 values, in millimetres.
- Plan-local coordinates, room seeds and level elevations have magnitude at
  most **10⁷ mm** (±10 km). No georeferencing coordinate system is defined.
- Positive dimensional fields have maximum **10⁶ mm**. Fields that permit zero,
  such as sill height and floor thickness, retain that allowance. Opening depth
  must also fit its host wall's thickness.
- Derived vertices are bounded by **10⁷ + 10⁶ mm**. This is a derived-geometry
  obligation, not permission to store an out-of-domain authoritative coordinate.
- Signed zero is canonicalized to positive zero. No metric epsilon changes
  stored endpoint equality or makes invalid non-finite input acceptable.
- Opening fit, horizontal overlap and vertical-clearance decisions obey the
  strict/non-strict inequalities in behavior §4. Exact comparisons reduce
  products and sums of stored dyadic values to integer arithmetic; intermediate
  values are not assumed to fit a fixed-width signed integer.

The format specification owns serialized resource limits and diagnostics;
behavior owns field semantics. Bounds are product limits, not a claim that
binary64 constructions exactly represent real geometry.

## 4. Predicates and constructions

`orientation` returns the exact determinant sign for finite represented inputs.
A floating-point filter with a proven Shewchuk error bound handles certified
cases; ambiguous, overflowing or underflow-sensitive cases use the independent
exact dyadic-integer determinant. No epsilon decides a determinant's sign.

Segment intersection coordinates are rational functions of dyadic inputs.
`arrangement::intersection_point` computes exact dyadic numerators/denominators
and rounds each coordinate to nearest binary64, ties to even. Edit split points
use this construction too. There is no exact-rational coordinate storage.
Perpendicular offsets generally involve irrational unit normals and remain
represented constructions; exact predicates do not make those coordinates exact.

## 5. Area and display

Area evaluation sums the shoelace products over represented ring coordinates
and subtracts holes exactly when required. Display rounds square millimetres
to hundredths of a square metre using **half-even** decimal midpoint decisions,
not floating division followed by formatting.

`exact_area::display_hundredths_m2` first uses a proven forward error bound and
constructs an exact value only when needed near a rounding boundary. Non-finite
input and result overflow fail rather than saturating. Ring validity and hole
containment remain caller preconditions documented by the API.

For example, exact areas of 30,245,000 mm² and 10,655,000 mm² display as
`30.24 m²` and `10.66 m²` respectively. This arithmetic contract is separate
from constructing the boundary on which the area is evaluated.

## 7. Conformance and determinism

Tests use exact dyadic/rational or integer-grid oracles over the stored input
values. They distinguish exact predicate signs, correctly rounded constructions,
snap-rounded topology and display rounding. Published example provenance is
recorded in [Provenance](PROVENANCE.md); other fixtures are original synthetic
data or derive from Domowik's normative coordinate examples.

Canonical output uses counter-clockwise exterior rings, clockwise holes,
least-vertex rotation, sorted holes and sorted components. Output must not
depend on input iteration order, hash-map order or arrival order beyond the
ordering explicitly specified by the document contract.

## 12. Numeric policies

### N-1 Stored coordinates are not quantized

Storage accepts in-domain binary64 coordinates without rounding to a topology
grid. Sub-quantum features may have positive stored length yet contribute no
distinct topological vertex or room. Metric measurement and topological
presence answer different questions.

### N-2 Identity is the topology grid

The quantum is **2⁻¹⁰ mm**. Cells are half-open `[k − ½, k + ½)` in grid units.
Cell equality is an equivalence relation and determines derived vertex identity,
adjacency and join-coordinate agreement. Agreement is a function of the member
set, never the first member's position. Proximity queries may use metric radii,
but proximity is not topological identity.

The snap-rounded arrangement routes segments through the centres of all hot
cells they meet, including endpoint and crossing cells. Faces carry winding
numbers; occupied boundaries, rooms and their areas use this derived geometry.
Authoritative wall coordinates remain unchanged.

### N-3 Split points persist as nearest binary64

Exact intersection arithmetic followed by correctly rounded binary64 commitment
does not introduce rational storage. A persisted constructed point need not lie
exactly on its ideal parent lines. Topological coherence is provided by the
arrangement contract, not by pretending construction rounding is absent.

### N-4 Area display remains exact and half-even

The contract is §5 and behavior R5.3.1–R5.3.2. Rendering a formatted string must
consume the shared exact/filtered rounding result rather than implement a
second decimal-rounding algorithm.

### N-5 Component linking uses positive-area topology

Behavior R5.1.9 links join-connected components through containment or
positive-area overlap and takes the required transitive closure. A relative
area threshold must not hide a genuine small overlap of large regions.
Per-component winding coverage in the snap-rounded subdivision supplies the
overlap decision. Any conservative over-inclusion or unresolved construction
case must be documented as such, not called exact conformance.

### N-6 Boundary-edge ownership and snap exclusion

Exclusion is evaluated per maximal constant-owner sub-segment, not a whole
canonical edge. The ownership unit is the contributing ring. Core derives
join-patch exclusion from excluded walls; a caller-supplied join list retains
its narrower join-node meaning.

## 14. Selected algorithms, provenance and licences

The filtered predicate, exact dyadic determinant, monotone-chain hull,
crossing-number containment, snap rounding and winding-number face tracing are
independent implementations. Their publications, consulted artifacts,
adaptation details and attributed test inputs are retained in
[Provenance](PROVENANCE.md). Original project code is Apache-2.0; a mathematical
algorithm is not assigned a source-code licence merely by citation.

Distributed dependency licences are maintained separately in
[RUST-DEPENDENCIES.md](RUST-DEPENDENCIES.md), [NOTICE](../NOTICE) and
`docs/third-party/`. Benchmarks compile during verification, but host-specific
timings are not a correctness gate or a whole-editor latency guarantee.
