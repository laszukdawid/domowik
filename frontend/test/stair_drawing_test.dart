import 'dart:math' as math;

import 'package:domowik_editor/stair_drawing.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  const start = Offset(1200, -700);

  test(
    'every 15° ray wins within 4° in either direction and all quadrants',
    () {
      for (var step = 0; step < 24; step++) {
        for (final deviation in [-4.0, 0.0, 4.0]) {
          final angle = step * math.pi / 12;
          final rawAngle = angle + deviation * math.pi / 180;
          final end =
              start + Offset(math.cos(rawAngle), math.sin(rawAngle)) * 2000;
          final result = constrainStairDrawingEnd(start, end);
          final projectedLength = 2000 * math.cos(deviation * math.pi / 180);
          expect(
            result.dx,
            closeTo(start.dx + math.cos(angle) * projectedLength, 1e-9),
          );
          expect(
            result.dy,
            closeTo(start.dy + math.sin(angle) * projectedLength, 1e-9),
          );
        }
      }
    },
  );

  test('axis and diagonal components stay exactly aligned', () {
    expect(
      constrainStairDrawingEnd(start, start + const Offset(20, 400)),
      start + const Offset(0, 400),
    );
    expect(
      constrainStairDrawingEnd(start, start + const Offset(400, 20)),
      start + const Offset(400, 0),
    );
    expect(
      constrainStairDrawingEnd(start, start + const Offset(-400, 20)),
      start + const Offset(-400, 0),
    );
    expect(
      constrainStairDrawingEnd(start, start + const Offset(20, -400)),
      start + const Offset(0, -400),
    );
    expect(
      constrainStairDrawingEnd(start, start + const Offset(380, 420)),
      start + const Offset(400, 400),
    );
    expect(
      constrainStairDrawingEnd(start, start + const Offset(-380, -420)),
      start + const Offset(-400, -400),
    );
  });

  test('nearest-ray switch is at half a 15° step in either direction', () {
    for (final sign in [-1, 1]) {
      Offset end(double degrees) =>
          start +
          Offset(
                math.cos(degrees * math.pi / 180),
                math.sin(degrees * math.pi / 180),
              ) *
              2000;
      expect(constrainStairDrawingEnd(start, end(sign * 7.49)).dy, start.dy);
      final result = constrainStairDrawingEnd(start, end(sign * 7.51)) - start;
      expect(
        math.atan2(result.dy, result.dx) * 180 / math.pi,
        closeTo(sign * 15, 1e-10),
      );
    }
  });

  test(
    'degenerate and unrepresentable input remains available to core validation',
    () {
      expect(constrainStairDrawingEnd(start, start), start);
      const infinite = Offset(double.infinity, 0);
      expect(constrainStairDrawingEnd(start, infinite), infinite);
      const hugeStart = Offset(1e308, 1e308), hugeEnd = Offset(-1e308, -1e308);
      expect(constrainStairDrawingEnd(hugeStart, hugeEnd), hugeEnd);
    },
  );
}
