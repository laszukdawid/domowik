# Native feature deduplication — 2026-09-05

Issue 0050, implementing Domowik R6.3.2. Original code and fixtures; no new
dependency, external algorithm adaptation or document-format change.

## Correctness and review

The old adjacent-entry predicate compared only class and entity. One short
wall's distinct free endpoints were incorrectly merged when adjacent in the
ranked list, but both survived when another candidate ranked between them.
The corrected predicate requires equality at every native ranking level.
Selection separately projects these raw features to distinct entity IDs
(issue 0049); the native API does not discard feature identity for the UI.

Claude Opus 5 implemented this correction and its tests through Herdr; Codex
reviewed the comparator, raw regression logs and measurements, and requested
the necessary-condition fast guard. The adjacent-endpoint test failed against
the old predicate: expected two endpoints, received one. Interleaved features
and endpoint/document-order invariance already passed before the change;
they are standing guards, not evidence of an insertion-order or winner bug.
The first-ranked winner was not changed by this correction.

The final geometry suite has 118 tests, including a private-index fixture
proving that identical full keys merge while different points/classes remain.
Joined endpoints still resolve to one join node, not member handles.

## Controlled measurements

Apple M4 Max, Mac16,5, 16 logical CPUs, 48 GiB; macOS 26.6.2 (25G83), Darwin
25.6.0 arm64. Rust 1.98.0 (88d9e12ae), Cargo 1.98.0 (797e8a9bc), release
profile. Other project builds/tests were paused during the three arms.

```sh
cargo test -p domowik-geometry --lib \
  hit::tests::both_free_endpoints_of_one_wall_survive_when_they_sort_adjacently
cargo bench -p domowik-geometry --bench spatial -- \
  --sample-size 20 --warm-up-time 0.2 --measurement-time 0.5 \
  'dense-endpoints|tied-regions'
cargo test -p domowik-geometry --lib
```

The same benchmark source and invocation were used in sequence with:

- A: old class/entity-only deduplication (known incorrect).
- B: full ranking-key equality, without a cheap rejection guard.
- C: class/entity equality **and** full ranking-key equality (final code).

For A/B reproduction, change only the final `dedup_by` predicate; do not
change sorting, fixtures or measurement settings. Restore C before any
verification or integration build. Full-key equality implies class/entity
equality, so C's short-circuit cannot change B's answer.

Criterion central estimates (not whole-frame or FFI latency):

| Query | A: incorrect | B: full key | C: guarded full key |
|---|---:|---:|---:|
| 32 tied regions, indexed | 862.66 ns | 975.74 ns | 886.70 ns |
| 220 tied regions, indexed | 4.3052 µs | 4.9057 µs | 4.5260 µs |
| 32 dense endpoints, indexed | 1.9670 µs | 2.0582 µs | 1.9812 µs |
| 220 dense endpoints, indexed | 14.273 µs | 15.295 µs | 14.647 µs |
| 220 tied regions, one-shot | 31.899 µs | 32.400 µs | 32.986 µs |
| 220 dense endpoints, one-shot | 58.188 µs | 61.819 µs | 60.276 µs |
| 220 dense endpoints, build only | 44.149 µs | 44.161 µs | 44.984 µs |

The unguarded correction incurred about 15% on the 220-region indexed query
and 7% on the endpoint-heavy query. C recovers most of that: central estimates
are about 5.1% and 2.6% above A respectively. C versus B change intervals were
−10.09% to −8.17% and −4.64% to −3.32%. This is a correctness fix with a
measured residual, not a claimed speedup over the incorrect implementation.

The endpoint fixture is a fan of unjoined walls, inner endpoints 100 mm and
outer endpoints 5000 mm from the query, with a 240 mm query radius. Both inner
endpoints and regions qualify. The tied-region fixture gives distinct walls
distance zero and the same closest point. Both are stress scenes, not a claim
about typical residential plans.

## Limits and corrected claims

One sequential run per arm on one host, with short windows, does not isolate
all noise or compiler-layout effects. Even the build-only control changed
about 1.9% between A and C although it never runs deduplication. Do not infer a
precise universal overhead from small residuals. Nor does this single control
establish a statistical noise floor.

An initial agent report incorrectly called the unpaired timings regression-
free; the controlled comparison disproved that. A later explanation attributed
C's remaining tied-region overhead to repeated entity IDs; that fixture has
distinct IDs, so that explanation is unsupported and is not adopted here.
Numeric distance exactness remains issue 0012. These are native query costs,
not presented-frame measurements or whole-application performance acceptance.

Raw logs reviewed on the measurement host are in
`/tmp/domowik-plan-w211zo/evidence-0050/` (environment, A red test, A/B/C benches
and B/C green suites). They are temporary artifacts; the reproducible commands,
fixture source, comparison method and measured values above are tracked here.
