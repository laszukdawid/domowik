# Implementation and test-data provenance

This records the external material relied on by the current implementation.
It is not a development diary or a claim of legal clearance. Follow the
[clean-room policy](CLEAN-ROOM-POLICY.md) and
[third-party policy](THIRD-PARTY-POLICY.md) before relying on additional material.
Dependency notices remain in [NOTICE](../NOTICE), the
[Rust dependency inventory](RUST-DEPENDENCIES.md) and [third-party records](third-party/).

## Optional server sign-in

The native sign-in flow follows OAuth 2.0 for Native Apps (RFC 8252, loopback
redirect) and PKCE (RFC 7636, S256). SHA-256 for the PKCE challenge is an
independent implementation of FIPS 180-4, so the editor keeps no hashing
dependency. Its tests use the published FIPS 180-4 example messages (`""`,
`"abc"`, the 448-bit message, one million `a`) with their published digests,
and the RFC 7636 Appendix B verifier/challenge pair. Server tests use original
synthetic responses. No application source, dependency, distributed content or
notice obligation was added.

## Solid appearance overrides

Per-material placed-model colors and whole-wall colors are original
owner-requested product behavior. Implementation uses the existing bounded
OBJ/GLB decoders, material identities, wall contours, operations and rendering
paths; no external application code, schema, artwork or test vectors were
consulted or adapted. User-supplied home photographs guide approximate local
color choices only; the photos, personal plan and imported models are not
committed or distributed as test data. Tests use original synthetic model bytes
and plans. No new third-party dependency, distributed content or notice
obligation was introduced; the CLI now also uses the existing first-party
model decoder and its already-reviewed dependencies.

The warm beige default wall finish and linear-light 3D shading are original
owner-requested appearance corrections. They reuse the sRGB transfer functions
already present in the texture renderers; no new external sources, dependencies
or assets were consulted. Regression inputs are synthetic colored surfaces.

## Architectural primitives and straight stairs

The body/opening/connection distinction and straight-stair assembly are original
Domowik product choices, specified in [Architectural primitives](ARCHITECTURAL-PRIMITIVES.md).
Geometry uses elementary rectangle offsets and linear interpolation, together
with the existing reviewed overlay and scene triangulation implementations. No
new external publication, source artifact, algorithm implementation or asset was
consulted or adapted. Tests and benchmarks use original synthetic geometry;
personal plans are local acceptance inputs only, never committed fixtures.
No dependencies, distributed content or notice obligations were added. Future-
schema rejection fixtures use the still-unsupported `0.3` label now that the
explicit architectural `0.2` schema is supported; their refusal guarantees remain.

The direct descending stairs tool and Floor-panel name/count/elevation controls
are original owner-requested interactions. Nearest-lower-level/default selection,
read-only preview and offset conversion are implemented in the shared Rust edit
layer. Tests use synthetic plans and real bridge-driven UI gestures, not personal
plan fixtures. No additional external sources, dependencies or content were used.
Live stair-field updates, typing coalescing and the compact inspector layout are
original owner-requested interactions. Their regression tests use synthetic data
and existing Flutter/core APIs; no new sources, content or dependencies were added.

## Active floor on reopen

The first-listed default, per-device last-active-floor memory for legacy files,
and ordered preference writes are original owner-requested product choices in
BEHAVIOR-SPEC section 2.6. Implementation uses the existing local config store;
no plan-format change, new dependency or distributed content is involved.
Tests use original synthetic levels and temporary plans. No external publication,
application source, artwork or test vectors were consulted or adapted; no new
notice obligations arise.

## Bulk wall measurements

Bulk height/thickness editing is an original product choice from the owner's
request, specified in BEHAVIOR-SPEC R7.1.7. Implementation uses Domowik's existing
field operations and atomic admission. Tests use original synthetic walls and
openings, not personal plans. No external publication, application source,
artwork or test data was consulted or adapted; no dependencies or notices added.

## Direct 2D object rotation

The rotation handle, Shift-held 15° snapping, R shortcut and click-away acceptance
are original owner-requested interactions, specified in BEHAVIOR-SPEC's direct
2D rotation rule. Pointer headings use elementary atan2 angle differences;
footprint projection and edits reuse the existing Rust core.
Tests use original synthetic coordinates and the existing original model
fixture. No external publication, source, artwork or test data was consulted or
adapted. No dependencies, distributed content or notice obligations were added.

## Robust predicates

- **Publication:** Jonathan Richard Shewchuk, *Adaptive Precision Floating-Point
  Arithmetic and Fast Robust Geometric Predicates*, Discrete & Computational
  Geometry 18:305–363 (1997), also CMU-CS-96-140(R).
- **Consulted artifact:** https://people.eecs.berkeley.edu/~jrs/papers/robustr.pdf,
  consulted 2026-08-23. The front matter permits duplication and distribution
  for noncommercial/educational purposes with its notice preserved. The paper
  itself is not bundled here.
- **Use:** the orientation filter's mathematical error bound
  `(3+16ε)·ε·(|detleft|+|detright|)` and the filter/exact-fallback architecture,
  from §3 and §§1–2, 5. This is an independent implementation; no source,
  pseudocode transcription, expansion implementation or test vectors were taken.
- **Reference hub:** https://www.cs.cmu.edu/~quake/robust.html was consulted
  only to locate the paper and its implementation's public-domain statement.
  The `predicates.c` implementation was deliberately not studied or adapted.

The exact determinant path is original dyadic-integer arithmetic derived from
the IEEE 754-2019 binary64 representation `±m · 2^e`, not Shewchuk's expansion
arithmetic. Its implementation and numerical guarantees are documented in
`crates/domowik-geometry/src/exact_det.rs` and `dyadic.rs`.

## Attributed published test inputs

Tinko Bartels, Vissarion Fisikopoulos and Martin Weiser, *Fast floating-point
filters for robust predicates*, BIT Numerical Mathematics 63(2), article 31
(2023), DOI [10.1007/s10543-023-00975-x](https://doi.org/10.1007/s10543-023-00975-x).
The version of record is CC BY 4.0; the authors' preprint is
[arXiv:2208.00497v1](https://arxiv.org/abs/2208.00497v1). Both were consulted
2026-08-24; §2 supplies exactly these two transcribed coordinate triples:

1. `(-0.01,-0.59), (0.01,0.57), (0,-0.01)`.
2. `(2^-801,2^-801), (2^-800,2^-800), (2^-801,2^-800)`.

No code, algorithm, figure or prose was adapted. Expected signs are independently
derived over the stored binary64 values, rather than copied from the paper's
real-arithmetic interpretation. Attribution and the transcription are also
recorded beside the test in `crates/domowik-geometry/src/lib.rs`. All other
geometry fixtures are original synthetic inputs or derive from Domowik's own
specification. Classification of these numeric inputs for release inventory
purposes remains subject to release-policy review; this record does not claim
that a test-data classification settles distribution obligations.

## Geometry algorithms

| Algorithm | Reference and use |
| --- | --- |
| Monotone-chain convex hull | A. M. Andrew, *Another efficient algorithm for convex hulls in two dimensions* (1979); independently implemented with exact turn decisions and reduced degenerate output. |
| Snap rounding | J. D. Hobby, *Practical segment intersection with finite precision output*, Computational Geometry 13 (1999), 199–214, §§2–4; independently implemented with binary-grid, half-open cells and exact segment/cell incidence. |
| Point-in-ring and face tracing | Standard crossing-number and planar-subdivision rules; original implementation with exact boundary predicates, half-open ray crossings and winding propagation. |

These describe mathematical references, not copied implementations or code
licences. Domowik's original implementations are Apache-2.0. Their numerical
contract is in [Numerical model](NUMERICAL-MODEL.md).

## Public model formats

OBJ/MTL parsing was independently implemented from Paul Bourke's public format
documentation, https://paulbourke.net/dataformats/obj/ (consulted 2026-10-03).
No example geometry or reference code was copied. GLB, accessors and static
transforms use Khronos glTF 2.0.1 (2021-10-11),
https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html. The specification's
terms govern that document; no specification text or code is bundled.
Texture-specific provenance is in [Model textures](third-party/model-textures.md).
Library thumbnails reuse the independent decoder and rasterizer with an original
fixed orthographic camera projection. List/grid presentation and its tests use
original synthetic models and pixels, not external catalogs or artwork.

The [hosted furniture catalog](../catalog/README.md) contains 100 original
Apache-2.0 generic models and their Rust-derived PNG thumbnails. Dimensions,
designs and recipes are invented in `catalog/generate.py`; its GLB writer reuses
the first-party Welcome Home box builder. No external furniture data, geometry,
textures, photographs, brand designs or catalog were consulted or adapted.
The already reviewed Khronos glTF publication above governs encoding references,
not the original geometry. No new package or external artwork is introduced.
Generation creates an asset-by-asset index of dimensions, sizes, SHA-256 hashes
and licenses alongside LICENSE/NOTICE, separately hosted rather than bundled.

The decoder dependency `draco-core` 2.2.1 is Apache-2.0, checked against
https://github.com/Filyus/draco-rust at
`c6244f35ec1a3c22a69948b04907244e9b215981`. Its normal dependency is `bytemuck`,
with `bytemuck_derive` 1.12.1 taken under Apache-2.0. No decoder source or C++
library was copied into Domowik. The package omits its licence text; the pinned
upstream licence and the repository's Apache-2.0 licence record the provenance,
and NOTICE explicitly records the missing packaged text.

Original mock models and documentation images are described by the
[Welcome Home example](../examples/welcome-home/README.md). User-imported
content is not bundled and is not relicensed by the application.

The [Four-level house example](../examples/four-level-house/README.md) is an
original synthetic layout with invented dimensions, authored through Domowik's
CLI and native C ABI. Its generators, editable plan, eight new box-built models
and per-level PNG/SVG exports are Apache-2.0 project content. It reuses the nine
original Welcome Home models and its box builder, not external catalog data.
The new objects are rugs, desks, low cabinets and illustrative stairs, hatch
covers, ladder and stairwell marker; they do not define native slab openings.
No external plan, model, image or test vector was consulted or adapted. It adds
no dependencies or third-party assets. Its two documentation screenshots show
the actual Linux editor; export/editor fonts and platform scaffolding retain
the project's existing notices. Capture details are in [Documentation images](images/README.md).
