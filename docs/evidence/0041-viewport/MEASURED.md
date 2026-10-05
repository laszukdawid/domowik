# Viewport CPU baseline — 2026-09-05

Host: Apple M4 Max, 16 logical processors, 48 GiB RAM, macOS 26.6.2
(25G83). Dart 3.11.4, Flutter test engine, JIT. CPU/memory were read from
`sysctl`; runtime/OS are emitted by the harness. Logical canvas: 1200×800.

| Work | Measurement |
|---|---:|
| Camera cycle: anchored zoom ×1.25, pan, inverse zoom, inverse pan, inverse point mapping | 48.76 ns mean |
| Empty-plan grid + ruler display-list recording, p50 | 271.04 µs |
| Same, p95 | 380.42 µs |
| Same, p99 | 501.13 µs |

One run: 10,000 warm-up camera cycles followed by 1,000,000 measured cycles;
100 warm-up paint recordings followed by 1,000 measured recordings. Each
picture and text layout is disposed. A reported checksum consumes camera
results. No timing assertion participates in correctness tests.

These are **isolated JIT CPU measurements**, not frame times, GPU/raster
times, input-to-photon latency, or a comparison with another application.
There is no document geometry in the paint case. The results establish a
repeatable baseline for these small components, not issue 0033's product gate.

Reproduce from `spikes/0024-flutter-rust-bridge/flutter_app`:

```sh
flutter test benchmark/viewport_benchmark.dart
```

The harness runs as its own stage in `scripts/verify.sh` to keep it executable;
its timings are recorded, never used as a pass/fail threshold.

## Work bounded by the viewport

The previous painter recomputed plan bounds and fitted the entire document on
every repaint. Bounds are now computed when document data changes, and only
an explicit fit/open/new consumes them to change the camera. Ordinary paint
uses an immutable camera snapshot. This removes the per-paint bounds scan and
the visible camera jump; it does not remove the renderer's geometry traversal.

Grid spacing is the smallest `{1,2,5} × 10^n` millimetre step occupying at
least 8 logical pixels. At 1200×800, at most 151 vertical and 101 horizontal
grid lines intersect the viewport, irrespective of document extent or zoom.
Only every fifth line gets a ruler label. Unit tests cover both zoom extremes
and the specification's 0.1/0.05 px/mm examples.

Provenance: independently implemented from Domowik BEHAVIOR-SPEC 1.2–1.5.
No third-party algorithm implementation, package, artwork or dataset was used.
