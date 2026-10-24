import 'dart:math' as math;
import 'dart:ui' show FrameTiming, FramePhase;

/// Order statistics over one metric's samples.
///
/// Percentiles are nearest-rank, as in docs/BENCHMARK-INTERACTION.md: samples
/// sorted ascending, the p-th percentile is the sample at index
/// `ceil(p/100 × n) − 1`. No interpolation and no outlier rejection.
class Summary {
  Summary._(
    this.count,
    this.min,
    this.mean,
    this.stdev,
    this.p50,
    this.p90,
    this.p95,
    this.p99,
    this.max,
  );

  final int count;
  final double min, mean, stdev, p50, p90, p95, p99, max;

  static Summary? of(Iterable<num> samples) {
    final sorted = samples.map((v) => v.toDouble()).toList()..sort();
    if (sorted.isEmpty) return null;
    double rank(double p) =>
        sorted[math.max(0, (p / 100 * sorted.length).ceil() - 1)];
    final mean = sorted.reduce((a, b) => a + b) / sorted.length;
    final variance =
        sorted.fold<double>(0, (s, v) => s + (v - mean) * (v - mean)) /
        sorted.length;
    return Summary._(
      sorted.length,
      sorted.first,
      mean,
      math.sqrt(variance),
      rank(50),
      rank(90),
      rank(95),
      rank(99),
      sorted.last,
    );
  }

  Map<String, Object> toJson() => {
    'count': count,
    'min': min,
    'mean': mean,
    'stdev': stdev,
    'p50': p50,
    'p90': p90,
    'p95': p95,
    'p99': p99,
    'max': max,
  };
}

/// One Flutter frame's phases in microseconds, keyed by its vsync start on
/// the engine clock (the clock `SchedulerBinding.currentSystemFrameTimeStamp`
/// reports), so frames can be attributed to the window that produced them.
class FrameSample {
  const FrameSample(
    this.vsyncMicros,
    this.buildMicros,
    this.rasterMicros,
    this.totalMicros,
  );

  factory FrameSample.of(FrameTiming t) => FrameSample(
    t.timestampInMicroseconds(FramePhase.vsyncStart),
    t.buildDuration.inMicroseconds,
    t.rasterDuration.inMicroseconds,
    t.totalSpan.inMicroseconds,
  );

  final int vsyncMicros, buildMicros, rasterMicros, totalMicros;

  List<int> toJson() => [vsyncMicros, buildMicros, rasterMicros, totalMicros];
}

/// Frames whose vsync falls in `[fromMicros, toMicros]`, summarised. Times
/// are milliseconds; `over16` and `over33` count frames whose total span
/// missed a 60 Hz or 30 Hz deadline.
Map<String, Object?> summariseFrames(
  List<FrameSample> frames,
  int fromMicros,
  int toMicros,
) {
  final inWindow = [
    for (final f in frames)
      if (f.vsyncMicros >= fromMicros && f.vsyncMicros <= toMicros) f,
  ];
  double ms(int micros) => micros / 1000;
  final seconds = (toMicros - fromMicros) / 1e6;
  return {
    'count': inWindow.length,
    'windowSeconds': seconds,
    'fps': seconds > 0 ? inWindow.length / seconds : null,
    'buildMs': Summary.of(inWindow.map((f) => ms(f.buildMicros)))?.toJson(),
    'rasterMs': Summary.of(inWindow.map((f) => ms(f.rasterMicros)))?.toJson(),
    'totalMs': Summary.of(inWindow.map((f) => ms(f.totalMicros)))?.toJson(),
    'over16': inWindow.where((f) => f.totalMicros > 16667).length,
    'over33': inWindow.where((f) => f.totalMicros > 33333).length,
    'raw': [for (final f in inWindow) f.toJson()],
  };
}

/// Input -> displayed-scene latencies in milliseconds for pointer [moves], or
/// null where no installed scene ever showed the move.
///
/// A move is `(sentMicros, revision)`: when it was dispatched, and the pose
/// revision of the first preview that carried it. A scene load is
/// `(startMicros, endMicros, revision)`, its revision being the one current
/// when it started. A move is shown by the first load, in start order, whose
/// revision includes the move's; a load that started later but still showed
/// an older preview is never credited with it.
List<double?> inputLatencies(
  List<(int, int)> moves,
  List<(int, int, int)> loads,
) {
  final ordered = [...loads]..sort((a, b) => a.$1.compareTo(b.$1));
  return [
    for (final (sent, revision) in moves)
      ordered
          .where((l) => l.$3 >= revision && l.$1 >= sent)
          .map((l) => (l.$2 - sent) / 1000)
          .firstOrNull,
  ];
}
