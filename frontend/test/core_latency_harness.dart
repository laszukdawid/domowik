// Shared by core_latency_test.dart (native FFI core) and
// web/core_latency_web_test.dart (browser WebAssembly core), so both measure
// the same calls on the same plan (issue 0088 evidence). Timings are printed,
// not asserted: they are host-dependent, like every benchmark here.

import 'dart:convert';

import 'package:domowik_editor/core/domowik_core.dart';

/// Builds a 6×6 grid of rooms (84 walls) and reports the median time per call,
/// in microseconds, for the editor's hot paths. Returns `op -> median µs`.
Map<String, double> measureCoreLatency(DomowikCore core, String label) {
  final handle = core.createDefault().handle;
  final level =
      ((jsonDecode(core.snapshot(handle).text!) as Map)['levels'] as List)
          .single['id'] as String;
  void wall(List<num> a, List<num> b) {
    final reply = core.edit(
      handle,
      jsonEncode({
        'kind': 'addWall',
        'levelId': level,
        'start': a,
        'end': b,
        'thicknessMm': 200,
        'heightMm': 2500,
      }),
    );
    if (!reply.succeeded) throw StateError('${core.lastError(handle)}');
  }

  const n = 6, step = 4000;
  for (var i = 0; i <= n; i++) {
    wall([0, i * step], [n * step, i * step]);
    wall([i * step, 0], [i * step, n * step]);
  }

  // Median over 15 batches of mean time per call. Batches, because a
  // browser coarsens its clock (100 µs in Chrome without cross-origin
  // isolation), which would round a single fast call to zero.
  double median(int perBatch, void Function(int) body) {
    for (var i = 0; i < 10; i++) {
      body(i); // warm-up: JIT and wasm tier-up
    }
    final samples = <double>[];
    final watch = Stopwatch();
    var call = 0;
    for (var batch = 0; batch < 15; batch++) {
      watch
        ..reset()
        ..start();
      for (var i = 0; i < perBatch; i++) {
        body(call++);
      }
      watch.stop();
      samples.add(watch.elapsedMicroseconds / perBatch);
    }
    samples.sort();
    return samples[samples.length ~/ 2];
  }

  final results = <String, double>{
    'hitTest': median(100, (i) {
      core.hitTest(
        handle,
        jsonEncode({
          'levelId': level,
          'pointMm': [1000 + i * 37 % 20000, 1500],
          'radiusMm': 120,
        }),
      );
    }),
    'snap': median(50, (i) {
      core.snap(
        handle,
        jsonEncode({
          'levelId': level,
          'pointMm': [1010 + i * 41 % 20000, 3990],
          'radiusMm': 120,
          'visibleRectMm': [-1000, -1000, 25000, 25000],
          'gridSpacingMm': 100,
          'gridEnabled': true,
          'objectEnabled': true,
          'angleEnabled': true,
          'altDisabled': false,
        }),
      );
    }),
    'derived': median(20, (_) => core.derived(handle, level)),
    'state': median(200, (_) => core.state(handle)),
    'snapshot': median(20, (_) => core.snapshot(handle)),
    'snapshot+setWall+undo': median(10, (i) {
      final walls =
          (jsonDecode(core.snapshot(handle).text!) as Map)['walls'] as List;
      final w = walls[i % walls.length] as Map;
      core.edit(
        handle,
        jsonEncode({
          'kind': 'setWall',
          'wallId': w['id'],
          'heightMm': 2600 + i,
        }),
      );
      core.undo(handle);
    }),
  };
  core.close(handle);
  for (final MapEntry(:key, :value) in results.entries) {
    print('LATENCY $label $key ${value.toStringAsFixed(1)} us');
  }
  return results;
}
