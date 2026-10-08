# 0096: Replace the union with a snap-rounded labelled arrangement

- **Phase:** 1
- **Priority:** High
- **Status:** In Progress
- **Depends on:** None

## Problem

`boolean::union` finds crossings on the original segments, then moves
vertices onto weld representatives without re-noding, classifies fragments
against the ORIGINAL rings, and traces rings that may never close. The three
stages see three different geometries. Reproduced on `4ead4cd`:

- Rectangles A `(10.0006,30.0006)–(20.0006,70.0006)`, B
  `(20.0003,30.0006)–(60.0003,60.0006)`, C `(20.0002,30.0007)–(60.0002,70.0007)`
  union to 799.994 mm²; A+C alone is 1,999.986 mm². `walls::plan` on three
  unjoined walls reproduces it.
- `trace_rings` emits a walk of three or more vertices whether or not it
  closed, so a missing edge becomes a long invented diagonal.
- `classify_point((1/3, 1), [(0,0),(1,3),(0,3)])` returns `Outside`: the ray
  crossing is a rounded division.
- `faces_in_scope` tests face corners only, so a wall touching a room along a
  span of its edge is out of scope.
- `locality::link` compares independently welded areas against a jointly
  welded union and maps `Uncertain` to a link, so R5.1.9 is not exact.

It is also quadratic in ring count for sparse plans, and the benchmarks top
out at 22 walls.

Found by an independent review (Codex GPT-6-Astra, 2026-10-08,
`issue-0030`); every item above was re-run before being recorded.

## Approach

Snap rounding (Hobby, *Practical segment intersection with finite precision
output*, Comput. Geom. 13 (1999) 199–214, doi:10.1016/S0925-7721(99)00021-8).

1. The topology grid becomes binary: cell `k` is `[k − ½, k + ½)` in units of
   2⁻¹⁰ mm (≈ 0.977 µm), so `x · 1024` is exact and every cell boundary is an
   exact binary64 value. Cell of a point is `floor(x·1024 + ½)`, Hobby's
   convention. This replaces the decimal `0.001` quantum, whose boundaries
   were not representable.
2. A cell is HOT when it holds a segment endpoint or an intersection point.
   An intersection is located from its correctly rounded construction; where
   that construction lands exactly on a cell boundary both neighbours are
   marked, which is conservative and keeps every true intersection inside a
   hot cell.
3. Every segment is routed through the centres of ALL hot cells it meets,
   decided exactly, in order along the segment. Hobby's Theorem 4.1: the
   output pieces meet only at common endpoints or coincide.
4. All topology then runs on integer coordinates with `i128` arithmetic:
   angular order, face tracing, signed area, point location. No `atan2`, no
   dyadic allocation, no tolerance.
5. Faces carry winding numbers from the directed ring edges. Occupied means
   winding > 0. Labelled windings (per join-connected component) give
   R5.1.9 overlap as "a face covered by two components" and containment as
   "inside a component's filled outer rings", both exact on the one welded
   embedding of the level.

Stored coordinates are untouched. Derived geometry (occupied boundaries,
rooms, areas) is expressed on cell centres.

## Acceptance criteria

- [x] The four counterexamples above are regression tests and pass.
- [x] `union`, `union_and_complement` and `union_with_sources` run on the
      snap-rounded arrangement; every traced walk closes, or the build
      fails loudly in debug.
- [x] Topology predicates after rounding are exact integer arithmetic.
- [x] `classify_point` decides interior points by exact orientation.
- [x] R5.1.9 overlap and containment are decided exactly on the level's
      welded arrangement; no `Uncertain` link remains.
- [x] `faces_in_scope` detects edge-span contact.
- [x] Broad phase is not quadratic in ring count for sparse plans.
- [x] Benchmarks cover 100, 1,000 and 10,000 walls, with before/after
      numbers recorded here, INCLUDING a near-parallel stress family: long,
      closely spaced walls a few micro-radians off parallel. Snap rounding's
      cost is the number of (segment, hot cell) incidences, which Hobby §2.3
      warns can approach N·(N+K) for nearly collinear input; real houses
      rarely have more than ~100 parallel walls, but the bound must be
      measured, not assumed.
- [x] NUMERICAL-MODEL §3/§12 and FORMAT-SPEC record the binary quantum and
      snap rounding; an executive decision records the change.
- [x] `./scripts/verify.sh` passes, or any environment-only failure is named.
      (2026-10-08: every Rust, Go-free, JSON, release-validation and
      Flutter test stage passed; it stopped at `frontend Flutter macOS
      application` because local Xcode 27.0 rejects the 10.15 deployment
      target. Not caused by this change; the later tracker and JSON stages
      were run by hand and pass.)

## Progress

### Implementation — 2026-10-08

- `geometry::overlay` (new): binary grid, hot cells via a bucket-grid broad
  phase (axis-aligned crossings exact without construction), exact half-open
  segment–cell routing, integer half-edge arrangement, per-label face
  windings. Ring tracing asserts closure.
- `geometry::boolean`: `union`, `union_and_complement`,
  `union_with_sources` rebuilt on it; public API unchanged.
- `geometry::locality`: one labelled arrangement per level; overlap from
  faces, containment by exact ring enclosure, links computed once.
  `Uncertain` is gone. `faces_in_scope` tests segment contact.
- `arrangement::classify_point`: exact crossing side. `cuts_along` and its
  helpers deleted. `domowik-format::topology_cell` and `JOIN_COINCIDENCE_MM`
  on the new grid.
- Two snap tests moved to grid-representable inputs (`1.125`/`1.875`,
  `0.25`/`0.375`): they assert exact ties and exact-radius admission, which
  now hold on cell centres.
- ED-0110; NUMERICAL-MODEL §3, §12 banner, §14; FORMAT-SPEC L14.

### Measurements — 2026-10-08

Apple M4 Max, criterion, `--warm-up-time 1 --measurement-time 3`, load
average ~2–3 (MDM baseline). Before = `4ead4cd` with the same bench file.

| `benches/scale.rs` | Before | After | Speed-up |
|---|---|---|---|
| isolated/100 | 465.8 µs | 142.1 µs | 3.3× |
| isolated/1000 | 8.31 ms | 1.56 ms | 5.3× |
| isolated/10000 | 391.2 ms | 19.5 ms | 20× |
| tiled/112 | 5.93 ms | 231 µs | 26× |
| tiled/1012 | 63.6 ms | 2.66 ms | 24× |
| tiled/9940 | 939.0 ms | 28.4 ms | 33× |
| bundle/10 | 210.9 µs | 87.6 µs | 2.4× |
| bundle/100 | 2.44 ms | 939 µs | 2.6× |
| bundle/1000 | 29.3 ms | 8.96 ms | 3.3× |
| fan/10 | 4.57 ms | 909 µs | 5.0× |
| fan/30 | 44.6 ms | 8.48 ms | 5.3× |
| fan/100 | 571.8 ms | 95.0 ms | 6.0× |

| `regions` / `locality` | Before | After |
|---|---|---|
| plan/single-room | 313 µs | 19.7 µs |
| plan/apartment | 462 µs | 31.6 µs |
| plan/grid-4x4 | 555 µs | 49.7 µs |
| occupied-region/grid-10x10 | 3.29 ms | 236 µs |
| components/wings-64 | 23.1 ms | 1.02 ms |
| closure/wings-64 | 46.7 ms | 2.07 ms |

Growth is near-linear for isolated, tiled and bundle. `fan` grows about
11× per 3.3× in N, the quadratic crossing count K it is built to produce;
the incidence blow-up Hobby warns of is not visible at N = 100. A first run
showed isolated/10000 at 83 ms: the per-piece winding probe scanned every
other piece's bounds. It is now bucket-indexed (19.5 ms).

**Not yet done.** Routing a long DIAGONAL segment visits every bucket of its
bounding box, quadratic in its length over the bucket size; a DDA walk
would make it linear. No fixture here exercises it. Crossings that are not
axis-aligned still take `intersection_point`'s exact path every time.

## Closure

- **Implementation/spec commits:**
- **Follow-up issues:**
