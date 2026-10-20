import 'dart:math' as math;
import 'package:domowik_editor/camera_3d.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  _cursorTests();
  test(
    'fit centers the scene and projection puts its target at screen center',
    () {
      final c = Camera3D()..fit([10, 2, -4], 5);
      final m = c.matrix(16 / 9);
      final p = [...c.target, 1.0];
      final clip = List.generate(
        4,
        (i) => List.generate(
          4,
          (j) => m[j * 4 + i] * p[j],
        ).reduce((a, b) => a + b),
      );
      expect(clip[0] / clip[3], closeTo(0, 1e-12));
      expect(clip[1] / clip[3], closeTo(0, 1e-12));
      expect(clip[2] / clip[3], inExclusiveRange(-1, 1));
      expect(c.distance, 11.5);
    },
  );
  test(
    'walk navigation is eye-level and follows yaw; orbit and zoom stay bounded',
    () {
      final c = Camera3D()
        ..fit([0, 1, 0], 5)
        ..enterWalk(3);
      expect(c.eye[1], 4.65);
      final z = c.eye[2];
      c.move(2, 1, 0);
      expect(c.eye[2], z - 2);
      expect(c.eye[0], 1);
      c.yaw = math.pi / 2;
      final x = c.eye[0];
      c.move(2, 0, 0);
      expect(c.eye[0], closeTo(x - 2, 1e-12));
      c.walking = false;
      c.orbit(1e6, 1e6);
      expect(c.pitch, lessThanOrEqualTo(1.45));
      c.zoom(1e-9);
      expect(c.distance, greaterThanOrEqualTo(0.15));
      expect(c.matrix(1).every((v) => v.isFinite), isTrue);
    },
  );
  test(
    'rebasing preserves the world pose and invalid pointer input is ignored',
    () {
      final c = Camera3D()..fit([2, 1, 3], 5);
      final e = c.eye;
      c.rebase(10, -20);
      expect(c.eye[0], closeTo(e[0] + 10, 1e-12));
      expect(c.eye[2], closeTo(e[2] - 20, 1e-12));
      final before = c.matrix(1);
      c.orbit(double.nan, 0);
      c.pan(0, double.infinity);
      c.zoom(double.nan);
      expect(c.matrix(1), before);
    },
  );
}

/// Logical pixel of world point [p] in a [w] × [h] viewport.
(double, double) _pixel(Camera3D c, List<double> p, double w, double h) {
  final m = c.matrix(w / h);
  final clip = List.generate(
    4,
    (i) => m[i] * p[0] + m[4 + i] * p[1] + m[8 + i] * p[2] + m[12 + i] * 1.0,
  );
  return ((clip[0] / clip[3] + 1) / 2 * w, (1 - clip[1] / clip[3]) / 2 * h);
}

List<double> _along((List<double>, List<double>) ray, double t) => [
  for (var i = 0; i < 3; i++) ray.$1[i] + ray.$2[i] * t,
];

void _cursorTests() {
  test('a cursor ray projects back onto the pixel it was cast through', () {
    for (final walking in [false, true]) {
      final c = Camera3D()..fit([3, 1, -2], 6);
      if (walking) c.enterWalk(0);
      for (final (x, y) in [(400.0, 300.0), (12.0, 590.0), (790.0, 4.0)]) {
        final ray = c.ray(x, y, 800, 600)!;
        expect(ray.$1, c.eye);
        final (px, py) = _pixel(c, _along(ray, 7), 800, 600);
        expect(px, closeTo(x, 1e-6));
        expect(py, closeTo(y, 1e-6));
      }
    }
    expect(Camera3D().ray(1, 1, 0, 600), isNull);
    expect(Camera3D().ray(double.nan, 1, 800, 600), isNull);
  });

  test('zooming toward the cursor keeps the point under it still', () {
    final c = Camera3D()..fit([0, 1, 0], 5);
    const x = 650.0, y = 140.0;
    final ray = c.ray(x, y, 800, 600)!;
    final forward = [for (var i = 0; i < 3; i++) c.target[i] - c.eye[i]];
    final t =
        forward.fold<double>(0, (s, v) => s + v * v) /
        List.generate(3, (i) => ray.$2[i] * forward[i]).reduce((a, b) => a + b);
    final anchor = _along(ray, t);
    final before = c.distance;
    c.zoomToward(0.5, ray.$2);
    expect(c.distance, closeTo(before * 0.5, 1e-9));
    final (px, py) = _pixel(c, anchor, 800, 600);
    expect(px, closeTo(x, 1e-6));
    expect(py, closeTo(y, 1e-6));
    // At the clamp the anchor still holds for the zoom actually applied.
    c.zoomToward(1e-9, c.ray(x, y, 800, 600)!.$2);
    expect(c.distance, 0.15);
    final (qx, qy) = _pixel(c, anchor, 800, 600);
    expect(qx, closeTo(x, 1e-6));
    expect(qy, closeTo(y, 1e-6));
  });

  test('orbiting about a picked point keeps it fixed on screen', () {
    final c = Camera3D()..fit([0, 1, 0], 5);
    final pivot = [2.5, 0.4, -1.0];
    final (x, y) = _pixel(c, pivot, 800, 600);
    final distance = c.distance;
    for (final (dx, dy) in [(40.0, 0.0), (0.0, 25.0), (-70.0, -30.0)]) {
      c.orbitAround(pivot, dx, dy);
      final (px, py) = _pixel(c, pivot, 800, 600);
      expect(px, closeTo(x, 1e-6));
      expect(py, closeTo(y, 1e-6));
    }
    expect(c.distance, closeTo(distance, 1e-9));
    // Pitch stays within the orbit range, as plain orbit keeps it.
    c.orbitAround(pivot, 0, 1e6);
    expect(c.pitch, closeTo(1.45, 1e-12));
  });

  test('framing fits the whole selection in narrow and wide viewports', () {
    // A 4 m × 2.5 m × 0.2 m wall, seen from several angles.
    const low = [-2.0, 0.0, -0.1], high = [2.0, 2.5, 0.1];
    final centre = [for (var i = 0; i < 3; i++) (low[i] + high[i]) / 2];
    final extent = math.sqrt(
      [
        for (var i = 0; i < 3; i++) (high[i] - low[i]) / 2,
      ].fold<double>(0, (s, v) => s + v * v),
    );
    for (final (w, h) in [(300.0, 600.0), (800.0, 600.0), (1200.0, 300.0)]) {
      for (final yaw in [0.0, 0.65, 1.57, 3.0]) {
        final c = Camera3D()
          ..fit([0, 1, 0], 5)
          ..yaw = yaw
          ..frame(centre, extent, aspect: w / h);
        for (var corner = 0; corner < 8; corner++) {
          final p = [
            corner & 1 == 0 ? low[0] : high[0],
            corner & 2 == 0 ? low[1] : high[1],
            corner & 4 == 0 ? low[2] : high[2],
          ];
          final (x, y) = _pixel(c, p, w, h);
          expect(x, inInclusiveRange(0, w), reason: '$w×$h yaw $yaw');
          expect(y, inInclusiveRange(0, h), reason: '$w×$h yaw $yaw');
        }
      }
    }
  });

  test('framing a selection larger than the zoom range still fits it', () {
    final c = Camera3D()..fit([0, 0, 0], 1);
    c.frame([0, 0, 0], 100);
    expect(c.distance, closeTo(230, 1e-9));
    expect(c.farPlane, greaterThan(c.distance + 100));
  });

  test('the visible range matches the clip planes along each ray', () {
    final c = Camera3D()..fit([0, 1, 0], 5);
    final centre = c.ray(400, 300, 800, 600)!.$2;
    expect(c.visibleRange(centre)[0], closeTo(c.nearPlane, 1e-9));
    expect(c.visibleRange(centre)[1], closeTo(c.farPlane, 1e-9));
    final corner = c.visibleRange(c.ray(0, 0, 800, 600)!.$2);
    expect(corner[0], greaterThan(c.nearPlane));
    expect(corner[1], greaterThan(c.farPlane));
  });

  test('framing centres the target and leaves walk mode', () {
    final c = Camera3D()
      ..fit([0, 1, 0], 5)
      ..enterWalk(0)
      ..frame([4, 0.5, -3], 1);
    expect(c.walking, isFalse);
    expect(c.target, [4, 0.5, -3]);
    expect(c.distance, closeTo(2.3, 1e-12));
    final (px, py) = _pixel(c, c.target, 800, 600);
    expect(px, closeTo(400, 1e-6));
    expect(py, closeTo(300, 1e-6));
  });
}
