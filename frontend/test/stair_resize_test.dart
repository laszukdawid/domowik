import 'package:domowik_editor/plan_viewport.dart';
import 'package:domowik_editor/stair_resize.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'stair handles use the supplied edge midpoints and shared view transform',
    () {
      final view = PlanViewport(zoom: 0.2, origin: const Offset(200, 300));
      final handles = stairResizeHandles(const [
        Offset(0, -500),
        Offset(3000, -500),
        Offset(3000, 500),
        Offset(0, 500),
      ], view);
      expect(handles.map((h) => h.edge), ['left', 'right', 'lower', 'upper']);
      expect(handles.map((h) => h.position), [
        view.toScreen(const Offset(1500, 500)),
        view.toScreen(const Offset(1500, -500)),
        view.toScreen(Offset.zero),
        view.toScreen(const Offset(3000, 0)),
      ]);
      expect(handles[0].direction.dx, 0);
      expect(handles[0].direction.dy, lessThan(0));
      expect(stairResizeHandles(const [], view), isEmpty);
    },
  );
}
