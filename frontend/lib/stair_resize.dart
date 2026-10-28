import 'package:flutter/painting.dart';

import 'plan_viewport.dart';

/// A screen-space affordance, not a proposed document edit. Rust owns resizing.
class StairResizeHandle {
  const StairResizeHandle(this.edge, this.position, this.direction);
  final String edge;
  final Offset position, direction;
}

/// Places handles at edge midpoints of the Rust-supplied lower-to-upper rectangle:
/// corners 0/3 are lower, 1/2 upper, 2/3 left and 0/1 right. All positions are
/// logical pixels; direction is display-only for cursor orientation. No physical
/// dimensions, endpoint constraints or stair geometry are derived in Dart.
List<StairResizeHandle> stairResizeHandles(
  List<Offset> outline,
  PlanViewport view,
) {
  if (outline.length != 4) return const [];
  final center = view.toScreen((outline[0] + outline[2]) / 2);
  final handles = <StairResizeHandle>[];
  for (final edge in const [
    ('left', 2, 3),
    ('right', 0, 1),
    ('lower', 0, 3),
    ('upper', 1, 2),
  ]) {
    final position = view.toScreen((outline[edge.$2] + outline[edge.$3]) / 2);
    handles.add(StairResizeHandle(edge.$1, position, position - center));
  }
  return handles;
}
