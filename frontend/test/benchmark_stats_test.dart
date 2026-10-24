import 'package:domowik_editor/benchmark_stats.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('percentiles are nearest-rank without interpolation', () {
    final s = Summary.of([for (var i = 1; i <= 100; i++) i])!;
    expect(s.count, 100);
    expect(s.min, 1);
    expect(s.p50, 50);
    expect(s.p90, 90);
    expect(s.p95, 95);
    expect(s.p99, 99);
    expect(s.max, 100);
    expect(s.mean, 50.5);
    final small = Summary.of([30, 10, 20])!;
    expect(small.p50, 20);
    expect(small.p99, 30);
    expect(Summary.of(const []), isNull);
  });

  test('frames are attributed to their vsync window', () {
    const frames = [
      FrameSample(1000, 2000, 3000, 6000),
      FrameSample(2000, 1000, 1000, 20000),
      FrameSample(3000, 1000, 1000, 40000),
      FrameSample(9000, 1000, 1000, 1000),
    ];
    final s = summariseFrames(frames, 1000, 3000);
    expect(s['count'], 3);
    expect(s['over16'], 2);
    expect(s['over33'], 1);
    expect((s['totalMs'] as Map)['max'], 40);
  });

  test('a load that started after a move but showed an older preview '
      'is not credited with it', () {
    // Move B (revision 1) is previewed by load A2. Move C is dispatched at
    // 3000 and carried by revision 2, but load A2 starts at 3500, before
    // C's preview reaches the core, so it still shows revision 1. Only
    // load A3 (revision 2) shows C.
    const moves = [(1000, 1), (3000, 2)];
    const loads = [(3500, 4500, 1), (6000, 7000, 2)];
    expect(inputLatencies(moves, loads), [3.5, 4.0]);
  });

  test('a move no installed scene showed has no latency', () {
    expect(inputLatencies(const [(1000, 3)], const [(2000, 2500, 2)]), [null]);
  });
}
