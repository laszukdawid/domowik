import 'package:domowik_editor/camera_3d.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('3D camera CPU path; no scene/model/renderer work', () {
    final c = Camera3D()..fit([0, 1, 0], 20);
    var checksum = 0.0;
    for (var i = 0; i < 10000; i++) {
      c.orbit(0.2, 0);
      checksum += c.matrix(16 / 9)[0];
    }
    const count = 100000;
    final clock = Stopwatch()..start();
    for (var i = 0; i < count; i++) {
      c.orbit(0.2, 0);
      checksum += c.matrix(16 / 9)[0];
    }
    clock.stop();
    // Named scope: Dart JIT arithmetic/allocation only, not a GPU frame budget.
    // ignore: avoid_print
    print(
      '3D orbit + view/projection matrix: ${clock.elapsedMicroseconds * 1000 / count} ns/update, $count updates, checksum=$checksum',
    );
    expect(checksum.isFinite, isTrue);
  });
}
