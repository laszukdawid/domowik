# Editor input lifecycle and continuous navigation — 2026-09-05

Scope: issues 0043 and 0044, macOS/arm64 Flutter prototype. Implemented from
Domowik BEHAVIOR-SPEC 1.4, 6.5 and 8.7; no comparable-application source,
external code, assets or new dependencies. Claude Opus 5 independently audited
commits d80d8f0/c9efb80 and contributed the pure panZoom primitive and tests;
Codex reviewed it, implemented input integration and ran verification.

## Correctness evidence

- The Flutter suite has 39 tests: 10 camera, 3 shell and 26 real-library editor
  scenarios. Only native file dialogs are mocked; document edits, history,
  saving and ownership use the real Rust library.
- Six new regression scenarios were also run against the previous committed
  main.dart (6d573e8), with the fixes immediately restored afterwards. All six
  failed: release-position commit, mid-drag wheel preview, click jitter,
  selection/lifecycle cancellation, zero-size Fit and tool-switch cancellation.
- Trackpad tests exercise two cumulative-scale updates, nonzero canvas layout
  translation, invalid scale, Escape and retained selection. The centroid is
  computed by transforming the global position-plus-pan endpoint once: on the
  pinned SDK, adding localPosition and localPan double-counts translation.
- Touch tests cover second-touch cancellation before pinch, combined scale and
  translation, ignored third touch, cancel restoration, and lifting one finger
  without converting the remaining contact into an edit. Stale and nonfinite
  release tests drive the Listener contract directly because engine hit-testing
  rejects NaN before widget delivery.

These are automated event-injection tests, not physical trackpad/touchscreen
validation or proof of platform pointer-capture delivery. Linux/Android and
reference-device validation remain issues 0024/0033.

## Isolated CPU measurements

Same M4 Max host as issue 0041: 16 logical processors, 48 GiB RAM,
macOS 26.6.2 (25G83), Dart 3.11.4, Flutter-test JIT. No concurrent native build
or benchmark. Harness: `flutter test benchmark/viewport_benchmark.dart`,
`viewport-cpu-v2`, 1200×800 logical pixels, empty content with grid/rulers.

| Work | Observation |
|---|---:|
| Existing transform cycle | 49.48 ns mean |
| Two-step panZoom cycle | 37.77 ns mean |
| Grid/ruler display-list recording | p50 266 µs, p95 381 µs, p99 510 µs |

Each camera measurement uses 10,000 warmup and 1,000,000 measured cycles;
painting uses 100 warmup and 1,000 measured samples. A checksum consumes the
results. No timing assertion. These are single-run isolated CPU measurements,
not GPU, raster, presented-frame, input-latency or whole-editor budgets.

## Review triage and residuals

- Fixed: zoom-dependent click/drag thresholds (normative >5 logical pixels,
  not an approximate 3-pixel threshold), preview/cancel defects and zero-size
  Fit. Continuous navigation now consumes Flutter's event families.
- Retained normative behavior: shift-wheel uses dx; nonfinite events are
  discarded whole. Suggested fallbacks changing those rules were not applied.
- Remaining targeting disagreement is issue 0045. A bounded Dart numeric probe
  confirmed grid-loop integer saturation/wrap, tracked as issue 0046; ordinary
  viewport timing does not establish a bound for those synthetic inputs.
- Native close-latch fallback concerns were not treated as proven reachable
  data loss; existing guards still fail closed. No AppKit menu automation or
  forced engine-loss recovery is claimed.
