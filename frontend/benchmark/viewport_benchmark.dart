// Measurement harness, deliberately outside test/. No timing assertions.
// Run explicitly: flutter test benchmark/viewport_benchmark.dart
import 'dart:convert';
import 'dart:io';
import 'dart:ui';

import 'package:domowik_editor/main.dart';
import 'package:domowik_editor/plan_viewport.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('viewport CPU measurement (not a frame-time acceptance gate)', () {
    const size = Size(1200, 800);
    var view = PlanViewport.fit(size, null);
    var checksum = 0.0;
    void transform() {
      view = view
          .zoomAt(1.25, const Offset(271, 163))
          .pan(const Offset(1, -1))
          .zoomAt(0.8, const Offset(271, 163))
          .pan(const Offset(-0.8, 0.8));
      checksum += view.toPlan(const Offset(350, 210)).dx;
    }

    for (var i = 0; i < 10000; i++) {
      transform();
    }
    final timer = Stopwatch()..start();
    for (var i = 0; i < 1000000; i++) {
      transform();
    }
    timer.stop();
    final transformNs = timer.elapsedTicks * 1e9 / timer.frequency / 1000000;

    var pinchView = PlanViewport.fit(size, null);
    void pinchCycle() {
      pinchView = pinchView
          .panZoom(
            previousCentroid: const Offset(400, 300),
            currentCentroid: const Offset(420, 310),
            scale: 1.25,
          )
          .panZoom(
            previousCentroid: const Offset(420, 310),
            currentCentroid: const Offset(400, 300),
            scale: 0.8,
          );
      checksum += pinchView.origin.dx;
    }

    for (var i = 0; i < 10000; i++) {
      pinchCycle();
    }
    timer.reset();
    timer.start();
    for (var i = 0; i < 1000000; i++) {
      pinchCycle();
    }
    timer.stop();
    final pinchNs = timer.elapsedTicks * 1e9 / timer.frequency / 1000000;

    Map<String, double> measurePaint(PlanViewport camera) {
      final samples = <double>[];
      final painter = PlanPainter(
        const [],
        Derived.empty,
        null,
        null,
        view: camera,
      );
      for (var i = 0; i < 1100; i++) {
        final sample = Stopwatch()..start();
        final recorder = PictureRecorder();
        painter.paint(Canvas(recorder), size);
        recorder.endRecording().dispose();
        sample.stop();
        if (i >= 100) samples.add(sample.elapsedTicks * 1e6 / sample.frequency);
      }
      samples.sort();
      return {'p50': samples[499], 'p95': samples[949], 'p99': samples[989]};
    }

    final normalPaint = measurePaint(view);
    final extremePaint = {
      for (final origin in [-1e30, 1e30, -1e308, 1e308])
        '$origin': measurePaint(
          PlanViewport(zoom: 0.0001, origin: Offset(origin, origin)),
        ),
    };
    stdout.writeln(
      jsonEncode({
        'harness': 'viewport-cpu-v3',
        'runtime': Platform.version,
        'os': Platform.operatingSystemVersion,
        'processors': Platform.numberOfProcessors,
        'mode':
            'flutter-test JIT; display-list recording only; no GPU/raster/presentation',
        'resolutionLogicalPx': [size.width, size.height],
        'transformCycleNsMean': transformNs,
        'transformCycles': 1000000,
        'transformWarmupCycles': 10000,
        'panZoomCycleNsMean': pinchNs,
        'panZoomStepsPerCycle': 2,
        'panZoomCycles': 1000000,
        'panZoomWarmupCycles': 10000,
        'gridAndRulersPaintUs': normalPaint,
        'extremeOriginGridAndRulersPaintUs': extremePaint,
        'extremeOriginZoom': 0.0001,
        'paintSamples': 1000,
        'paintWarmupSamples': 100,
        'checksum': checksum,
      }),
    );
  });
}
