import 'dart:ui';

import 'package:domowik_editor/main.dart';
import 'package:domowik_editor/plan_viewport.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('the painter records grids when inverse coordinates overflow', () {
    for (final value in [-1e30, 1e30, -1e308, 1e308]) {
      for (final zoom in [0.0001, 0.1, 100.0]) {
        final recorder = PictureRecorder();
        final painter = PlanPainter(
          const [],
          Derived.empty,
          null,
          null,
          view: PlanViewport(zoom: zoom, origin: Offset(value, value)),
        );
        painter.paint(Canvas(recorder), const Size(1200, 800));
        recorder.endRecording().dispose();
      }
    }
  });
}
