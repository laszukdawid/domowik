# Wall contact admission: issue 0055 progress

Status: axis-admission checkpoint, 2026-09-06. Issue 0055 remains In Progress.
The core rejects coincident and positively overlapping collinear wall axes,
and the CLI and native-backed editor exercise those refusals. This is not
completion evidence for all wall contacts. Subsequent area-overlap work is
recorded in [the warning report](AREA-WARNINGS.md); intersection construction
still has numerical conformance work outstanding.

## Delivered boundary

`domowik-geometry::wall_contact::axis_contact` classifies stored binary64
axes without a tolerance. Full coincidence sorts before positive collinear
overlap. Endpoint-only contact remains legal; different levels do not interact.
The shared edit builder checks refusing contacts before splitting or remapping
openings, resolves crossings for all co-moved walls, and checks the resulting
children and shortened parents again. Split children inherit parent fields,
including unknown fields. Exact zero-length cuts refuse before that wall's
opening remap.

`domowik-document::wall_contact::wall_contact_refusals` checks final affected
axes against final same-level walls. Newly effective batches pass this guard
before publication, including raw operations, external admission, promoted
held batches, undo, redo and compensation. Canonical replay also checks the
whole candidate against the current document: an incoming batch can discard
an older repair indirectly without naming either wall.

The admission rule in BEHAVIOR-SPEC section 3.1 leaves historical replay
unchanged. An unrelated or axis-neutral edit may retain a legacy conflict;
changing a participant must remove its refusing contacts. Repair is allowed,
but undo or compensation cannot restore that conflict. Refused history actions
leave their stacks unchanged. A batch may transiently overlap while applying
its operations if its final state passes. A newly effective bad batch does
not become admissible merely because a later batch would repair it.

The edit builder also recognizes explicitly supplied unchanged endpoints as
axis-neutral. Repeating a property form must not split a legacy crossing.
Unresolved crossings in raw historical data remain unchanged.

## Numerical evidence

The orientation filter retains its floating-point error bound. Uncertain,
underflow-risk and overflow cases use an exact integer determinant over the
stored coordinates. Six products accumulate in positive and negative fixed
66-limb arrays; each finite binary64 coordinate is decoded as an integer
mantissa times a power of two. The fallback allocates no heap memory by
construction; allocation counts were not measured.

The previous fallback misclassified `orientation((0, eta), (1, 0), (eta, eta))`
as collinear, where `eta` is the smallest positive subnormal. Its determinant
is exactly `eta*eta > 0`. A regression first reproduced that failure and now
passes. Geometry tests include mixed-scale underflow, overflowing differences,
finite-exponent randomized comparisons, and an independent integer oracle for
all 360,000 pairs of nonzero directed segments on the 5-by-5 integer grid.
The oracle checks classification; it does not prove rounded intersection
coordinates or arrangement construction exact.

## Named-host performance

Measurements ran on 2026-09-05: Apple M4 Max, 16 physical/logical cores,
48 GiB RAM, macOS 26.6.2 (25G83), arm64; Rust 1.98.0 (`88d9e12ae`,
LLVM 22.1.8), Cargo 1.98.0. Other project builds/tests paused during each
benchmark run. The operating system and unrelated processes were not quiesced.

```sh
cargo bench -p domowik-geometry --bench predicates -- \
  --sample-size 20 --warm-up-time 1 --measurement-time 2 'orientation|wall-contact'
cargo bench -p domowik-document --bench session -- \
  --sample-size 20 --warm-up-time 1 --measurement-time 2 'wall_contact_admission|^commit/'
```

| Predicate | Criterion time estimate | Reported interval |
|---|---:|---:|
| Orientation, filtered | 1.7453 ns | 1.7397 to 1.7512 ns |
| Orientation, exact fallback | 42.771 ns | 42.614 to 42.934 ns |
| Orientation, mixed subnormal | 39.038 ns | 38.902 to 39.187 ns |
| Orientation, overflow | 28.307 ns | 27.670 to 28.772 ns |
| Axis contact, disjoint | 12.080 ns | 12.011 to 12.169 ns |
| Axis contact, proper crossing | 12.639 ns | 12.524 to 12.774 ns |
| Axis contact, endpoint touch | 11.451 ns | 11.356 to 11.553 ns |
| Axis contact, endpoint on interior | 11.741 ns | 11.509 to 11.995 ns |
| Axis contact, collinear overlap | 11.422 ns | 11.228 to 11.585 ns |
| Axis contact, near-collinear fallback | 43.066 ns | 42.901 to 43.228 ns |
| Axis contact, diagonal collinear disjoint | 168.36 ns | 167.86 to 168.86 ns |

Admission fixtures contain short disjoint axes with one changed wall.
Changed-axis discovery, result allocation and drop are timed; fixture clones
are not. For `n` final walls, `k` changed axes and `r` refusals, the guard does
O(n + k*n + r*log(r)) work and uses O(n + r) auxiliary storage. It avoids
an all-pairs pass for a one-wall edit, but a batch changing every wall can
still require quadratic work.

| 2,000-wall operation | Criterion time estimate | Reported interval |
|---|---:|---:|
| Height-only admission | 29.523 microseconds | 29.395 to 29.656 microseconds |
| Clear one-wall move admission | 65.636 microseconds | 65.289 to 65.926 microseconds |
| Refused overlap admission | 68.079 microseconds | 67.532 to 68.592 microseconds |
| One additional session commit | 261.04 microseconds | 258.93 to 263.01 microseconds |

The commit benchmark includes session destruction, with fixture cloning outside
the timed closure. It does not include the gesture builder, geometry derivation,
file I/O or FFI. Neither table measures a GUI frame, GPU work or device-wide
responsiveness. No median/p95 or controlled before/after claim follows from
these runs. Adding admission checks has a cost; the commit estimate remains
below this harness's 1 ms regression alarm on this host.

Raw local logs: `/tmp/domowik-0055-z24mHX/predicates-rerun.txt` and
`/private/tmp/domowik-0055-admission-bench.log`. Later edits to the measured
geometry and admission paths were comments/tests only; the edit builder has
changed since measurement and is not included in these timings.

## Verification and remaining work

The tracked CLI process tests reject creation, movement and raw `apply`
(including `--dry-run`) without changing saved bytes. They also accept a
crossing move that yields four children, then undo it in one step. Native
bridge tests assert unchanged snapshots, revision, dirty state and history
after refusals. The GUI test performs both refusing drags, checks disabled
history actions and unchanged saved bytes, then makes and undoes a legal move.
Its camera uses exactly 0.5 px/mm: an arbitrary fit scale can place a dragged
axis a tiny nonzero distance away, which is not mathematical coincidence.

Full repository verification passed at implementation commit `f12380f` on
2026-09-06, including the unchanged-endpoint fix:

```sh
PATH="/Users/dawidlaszuk/develop/flutter/bin:$PATH" ./scripts/verify.sh
```

The process exited 0 and ended with `=== VERIFICATION PASSED === all stages`.
The local log is `/private/tmp/domowik-0055-verify.log`; no stage was skipped.
The run covered Rust build/format/Clippy/tests and benchmark compilation, Go
hygiene and tracker checks, native C/Dart harnesses, Flutter analysis, 152
Flutter tests, viewport measurement harnesses and the macOS release application
checks. It includes 118 document tests, 19 edit tests, 138 geometry unit tests
plus two integration tests, and 53 native bridge tests. These results verify
this checkpoint, not completion of the remaining contact rules or the app.

A repeat verifier run at the documentation-only commit `f0f70dc` failed in the
macOS build with duplicate system Swift module definitions. An unchanged clean
release-build retry then passed. The failure and its unresolved cause are
tracked in [issue 0056](../../../issues/0056-macos-sdk-module-redefinition.md);
the successful first run must not be read as repeated-build reliability.

At this checkpoint, work still required by 0055 included `wall-area-overlap` warnings for parallel
and nonparallel disjoint axes, warning refresh after thickness edits, and
end-to-end warning tests. Rounded proper-crossing construction, the provisional
join/cut quantum, and the raw-operation unresolved-crossing boundary prevent
a full R3.1.2 conformance claim. In particular, an exact contact class does not
make its constructed intersection exact. Whole-editor performance and other
platforms remain separate open gates.

## Review and provenance

Claude Opus 5 implemented the axis classifier and integer orientation fallback;
Claude Sonnet 5 implemented shared edit resolution. Codex implemented session,
CLI and bridge integration, independent oracle/regression tests and the GUI
refusal test. Root review found the mixed-subnormal determinant failure and
the unchanged-endpoint incidental split. Opus reviewed the final resolver and
admission logic. Tests, algorithms and fixtures derive from Domowik's own
specification and independently developed mathematics. No comparable-app source,
fixtures or assets were consulted. The document crate adds only a dependency
on Domowik's existing geometry crate; no external dependency was added.
