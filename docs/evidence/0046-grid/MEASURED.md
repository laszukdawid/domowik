# Bounded grid painting — 2026-09-05

Issue 0046. Independently implemented from Domowik BEHAVIOR-SPEC 1.5 and a
bounded Dart runtime probe; no external code or dependencies.

## Failure and correction

The previous absolute-index loop converted huge finite endpoints to int64.
On Dart 3.11.4, `ceil(1e31/100)` and `floor((1e31+12000)/100)` both return
9223372036854775807. Incrementing the lower value wraps to int64 minimum,
which still satisfies the upper comparison. A three-iteration probe confirmed
this without hanging the application.

Traversal now reduces the screen origin modulo five spacings, retaining the
major-line phase, then advances a zero-based local counter. With spacing ≥8
logical pixels, at most floor(extent/spacing)+1 lines are visited. A separate
16,384-line safety ceiling refuses unsupported synthetic layouts before
integer conversion. This is not a document-coordinate or camera-zoom limit.

Huge finite origins retain finite grid positions even when inverse plan
coordinates overflow. Labels are omitted when lattice indices are nonfinite
or |index|≥2^52, where the half-step rounding neighbourhood is no longer
representable. This avoids falsely precise measurements; normal labels and
origin/major-line alignment are preserved.

Three new camera tests cover normal coordinates, huge signed origins,
inverse overflow and invalid/unsupported layout bounds. One painter test
records actual grid/ruler display lists across 12 extreme camera cases. The
full Flutter suite has 43 tests. Full repository verification additionally
builds and checks the macOS application bundle.

## CPU measurements

Harness `viewport-cpu-v3`: Flutter-test JIT, Dart 3.11.4, macOS 26.6.2
(25G83), Apple M4 Max, 16 logical processors, 48 GiB RAM. Empty 1200×800
logical-pixel canvas, grid and rulers, no concurrent benchmark/native build.
Per scenario: 100 warmup + 1,000 measured display-list recordings.

| Origin on both axes | Zoom | p50 µs | p95 µs | p99 µs |
|---|---:|---:|---:|---:|
| Normal camera | 0.1 | 287 | 394 | 547 |
| -1e30 | 0.0001 | 50 | 71 | 174 |
| +1e30 | 0.0001 | 49 | 69 | 178 |
| -1e308 | 0.0001 | 50 | 71 | 175 |
| +1e308 | 0.0001 | 51 | 70 | 176 |

Extreme scenarios omit unrepresentable text labels, so their timings are not
a like-for-like speedup. Normal painting remains in the same sub-millisecond
CPU range as prior single-run evidence; no statistical speedup/regression
claim is made. Timings are not assertions and do not measure raster/GPU work,
presented frames, input latency or the whole-editor budgets in issue 0033.

Reproduce with `flutter test benchmark/viewport_benchmark.dart` from the
spike's `flutter_app` directory. Correctness remains in `test/`, separate
from this measurement harness.
