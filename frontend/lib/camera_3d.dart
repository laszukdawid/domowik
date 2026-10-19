import 'dart:math' as math;
import 'dart:typed_data';

/// Local camera state in renderer metres. No document or mesh dependency.
class Camera3D {
  List<double> target = [0, 1, 0];
  List<double> walkEye = [0, 1.65, 10];
  double yaw = 0.65, pitch = 0.9, distance = 15, radius = 5;
  bool walking = false;
  List<double> get eye => walking
      ? List.of(walkEye)
      : [
          target[0] + distance * math.cos(pitch) * math.sin(yaw),
          target[1] + distance * math.sin(pitch),
          target[2] + distance * math.cos(pitch) * math.cos(yaw),
        ];

  void fit(List<double> center, double extent) {
    target = List.of(center);
    radius = math.max(1, extent);
    distance = radius * 2.3;
    yaw = 0.65;
    pitch = 0.9;
    walking = false;
  }

  void enterWalk(double elevation) {
    walkEye = [target[0], elevation + 1.65, target[2] + radius * 1.25];
    walking = true;
    yaw = 0;
    pitch = 0;
  }

  void orbit(double dx, double dy) {
    if (!dx.isFinite || !dy.isFinite) return;
    yaw -= dx * 0.006;
    pitch = (pitch + dy * 0.006).clamp(walking ? -1.4 : 0.05, 1.45);
  }

  void zoom(double factor) {
    if (!factor.isFinite || factor <= 0) return;
    if (walking) {
      move((1 - factor) * math.max(radius, 1), 0, 0);
    } else {
      distance = (distance * factor).clamp(0.15, math.max(100, radius * 40));
    }
  }

  void pan(double dx, double dy) {
    if (!dx.isFinite || !dy.isFinite) return;
    final scale = distance * 0.002;
    final right = [math.cos(yaw), 0.0, -math.sin(yaw)];
    final up = [
      -math.sin(yaw) * math.sin(pitch),
      math.cos(pitch),
      -math.cos(yaw) * math.sin(pitch),
    ];
    for (var i = 0; i < 3; i++) {
      final d = (-dx * right[i] + dy * up[i]) * scale;
      target[i] += d;
      if (walking) walkEye[i] += d;
    }
  }

  void move(double forward, double strafe, double vertical) {
    walkEye[0] += -math.sin(yaw) * forward + math.cos(yaw) * strafe;
    walkEye[2] += -math.cos(yaw) * forward - math.sin(yaw) * strafe;
    walkEye[1] += vertical;
  }

  void rebase(double x, double z) {
    target[0] += x;
    target[2] += z;
    walkEye[0] += x;
    walkEye[2] += z;
  }

  Float64List matrix(double aspect) {
    final e = eye;
    final aim = walking
        ? [
            e[0] - math.sin(yaw) * math.cos(pitch),
            e[1] - math.sin(pitch),
            e[2] - math.cos(yaw) * math.cos(pitch),
          ]
        : target;
    List<double> norm(List<double> v) {
      final l = math.sqrt(v.fold<double>(0, (s, v) => s + v * v));
      return v.map((v) => v / l).toList();
    }

    List<double> cross(List<double> a, List<double> b) => [
      a[1] * b[2] - a[2] * b[1],
      a[2] * b[0] - a[0] * b[2],
      a[0] * b[1] - a[1] * b[0],
    ];
    double dot(List<double> a, List<double> b) =>
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    final z = norm([for (var i = 0; i < 3; i++) e[i] - aim[i]]);
    final x = norm(cross([0, 1, 0], z));
    final y = cross(z, x);
    final view = [
      x[0],
      y[0],
      z[0],
      0.0,
      x[1],
      y[1],
      z[1],
      0.0,
      x[2],
      y[2],
      z[2],
      0.0,
      -dot(x, e),
      -dot(y, e),
      -dot(z, e),
      1.0,
    ];
    final f = 1 / math.tan(math.pi / 6);
    final near = walking ? 0.025 : math.max(0.025, distance * 0.0005);
    final far = math.max(100, distance + radius * 10);
    final projection = [
      f / aspect,
      0.0,
      0.0,
      0.0,
      0.0,
      f,
      0.0,
      0.0,
      0.0,
      0.0,
      (far + near) / (near - far),
      -1.0,
      0.0,
      0.0,
      2 * far * near / (near - far),
      0.0,
    ];
    return Float64List.fromList(
      List.generate(
        16,
        (i) => List.generate(
          4,
          (k) => projection[k * 4 + i % 4] * view[(i ~/ 4) * 4 + k],
        ).fold<double>(0, (a, b) => a + b),
      ),
    );
  }
}
