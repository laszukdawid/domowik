import 'dart:math' as math;
import 'dart:typed_data';

/// Local camera state in renderer metres. No document or mesh dependency.
class Camera3D {
  Camera3D();

  /// Snapshot for a pending scene's coordinate system, without moving the
  /// camera used to navigate the scene that is still on screen.
  Camera3D.copy(Camera3D other)
    : target = List.of(other.target),
      walkEye = List.of(other.walkEye),
      yaw = other.yaw,
      pitch = other.pitch,
      distance = other.distance,
      radius = other.radius,
      walking = other.walking;

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

  /// Orbit zoom that keeps the point under [direction] (a [ray] from the eye)
  /// fixed on screen. It slides along the plane through [target] facing the
  /// camera, so it needs no scene query. Walking zooms as [zoom] does.
  void zoomToward(double factor, List<double> direction) {
    if (!factor.isFinite || factor <= 0) return;
    if (walking || direction.length != 3) return zoom(factor);
    final e = eye;
    final forward = [for (var i = 0; i < 3; i++) target[i] - e[i]];
    final along = _dot(direction, forward);
    if (!along.isFinite || along <= 1e-9) return zoom(factor);
    final t = _dot(forward, forward) / along;
    final anchor = [for (var i = 0; i < 3; i++) e[i] + direction[i] * t];
    final before = distance;
    zoom(factor);
    final applied = distance / before;
    for (var i = 0; i < 3; i++) {
      target[i] += (anchor[i] - target[i]) * (1 - applied);
    }
  }

  /// [orbit] about [pivot] instead of [target]: eye and target turn rigidly
  /// around it, so the pivot keeps its place on screen. Walk mode and
  /// non-finite pivots fall back to [orbit].
  void orbitAround(List<double> pivot, double dx, double dy) {
    if (!dx.isFinite || !dy.isFinite) return;
    if (walking || pivot.length != 3 || pivot.any((v) => !v.isFinite)) {
      return orbit(dx, dy);
    }
    final yawStep = -dx * 0.006;
    final pitchStep = (pitch + dy * 0.006).clamp(0.05, 1.45) - pitch;
    var v = [for (var i = 0; i < 3; i++) target[i] - pivot[i]];
    // Pitch turns about the camera's right axis; -pitchStep raises the view.
    final r = [math.cos(yaw), 0.0, -math.sin(yaw)];
    final c = math.cos(-pitchStep), s = math.sin(-pitchStep);
    final rv = _dot(r, v);
    final cross = [
      r[1] * v[2] - r[2] * v[1],
      r[2] * v[0] - r[0] * v[2],
      r[0] * v[1] - r[1] * v[0],
    ];
    v = [
      for (var i = 0; i < 3; i++) v[i] * c + cross[i] * s + r[i] * rv * (1 - c),
    ];
    // Yaw turns about the vertical through the pivot.
    final cy = math.cos(yawStep), sy = math.sin(yawStep);
    v = [v[0] * cy + v[2] * sy, v[1], -v[0] * sy + v[2] * cy];
    target = [for (var i = 0; i < 3; i++) pivot[i] + v[i]];
    yaw += yawStep;
    pitch += pitchStep;
  }

  /// The orbit distance that fits a sphere of [extent] metres in a viewport
  /// of [aspect] (width / height) from any angle, with 15% margin. The
  /// narrower of the vertical and horizontal fields of view decides.
  double framingDistance(double extent, double aspect) {
    final half = math.pi / 6;
    final narrow = aspect.isFinite && aspect > 0 && aspect < 1
        ? math.atan(math.tan(half) * aspect)
        : half;
    // Not capped by the zoom range: an explicit framing must fit.
    return math.max(0.15, math.max(0.25, extent) * 1.15 / math.sin(narrow));
  }

  /// Orbit framing of a sphere, keeping the current viewing angle.
  void frame(List<double> center, double extent, {double aspect = 1}) {
    if (center.length != 3 || center.any((v) => !v.isFinite)) return;
    if (!extent.isFinite) return;
    walking = false;
    target = List.of(center);
    // Clipping and zoom limits must cover what is framed.
    radius = math.max(radius, extent);
    distance = framingDistance(extent, aspect);
  }

  /// Clip-plane depths along the view axis, metres.
  double get nearPlane => walking ? 0.025 : math.max(0.025, distance * 0.0005);
  double get farPlane => math.max(100, distance + radius * 10).toDouble();

  /// Distances along unit [direction] from the eye that the projection keeps:
  /// `[near, far]` in metres, for picking only what is drawn.
  List<double> visibleRange(List<double> direction) {
    final (_, _, _, back) = _basis();
    final along = -_dot(direction, back);
    if (!(along > 1e-9)) return [0, 0];
    return [nearPlane / along, farPlane / along];
  }

  /// A unit ray from the eye through pixel ([x], [y]) of a [width] × [height]
  /// logical viewport, in scene metres: `(origin, direction)`. Null for an
  /// empty viewport or non-finite position.
  (List<double>, List<double>)? ray(
    double x,
    double y,
    double width,
    double height,
  ) {
    if (!(width > 0) || !(height > 0) || !x.isFinite || !y.isFinite) {
      return null;
    }
    final (e, right, up, back) = _basis();
    final tanHalf = math.tan(math.pi / 6);
    final nx = (2 * x / width - 1) * tanHalf * (width / height);
    final ny = (1 - 2 * y / height) * tanHalf;
    final d = [
      for (var i = 0; i < 3; i++) right[i] * nx + up[i] * ny - back[i],
    ];
    final l = math.sqrt(_dot(d, d));
    return (e, [for (final v in d) v / l]);
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

  static double _dot(List<double> a, List<double> b) =>
      a[0] * b[0] + a[1] * b[1] + a[2] * b[2];

  /// Eye and its right/up/back unit axes, shared by [matrix] and [ray].
  (List<double>, List<double>, List<double>, List<double>) _basis() {
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
    final z = norm([for (var i = 0; i < 3; i++) e[i] - aim[i]]);
    final x = norm(cross([0, 1, 0], z));
    return (e, x, cross(z, x), z);
  }

  Float64List matrix(double aspect) {
    final (e, x, y, z) = _basis();
    final dot = _dot;
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
    final near = nearPlane, far = farPlane;
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
