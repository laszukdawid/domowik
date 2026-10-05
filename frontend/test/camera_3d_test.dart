import 'dart:math' as math;
import 'package:domowik_editor/camera_3d.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
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
