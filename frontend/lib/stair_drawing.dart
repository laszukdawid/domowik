import 'dart:math' as math;

import 'package:flutter/painting.dart';

/// Quantizes a stair drawing pointer to the nearest absolute 15° ray from
/// [start], in plan millimetres (Y up). Length is the pointer's projection on
/// that ray. This is transient input quantization, not stair geometry: the
/// unchanged Rust builder validates endpoints and derives the assembly.
///
/// Heading/trigonometry use approximate binary64; rounded half steps go away
/// from zero. Cardinal/diagonal rays use exact component directions. Invalid or
/// overflowing arithmetic leaves [end] unchanged for normal core refusal.
/// Pure, deterministic for supplied inputs, with no retained state or ownership.
Offset constrainStairDrawingEnd(Offset start, Offset end) {
  final delta = end - start;
  if (!delta.dx.isFinite || !delta.dy.isFinite || delta == Offset.zero) {
    return end;
  }
  final step = (math.atan2(delta.dy, delta.dx) / (math.pi / 12)).round();
  final direction = switch (step % 24) {
    0 => const Offset(1, 0),
    3 => const Offset(1, 1),
    6 => const Offset(0, 1),
    9 => const Offset(-1, 1),
    12 => const Offset(-1, 0),
    15 => const Offset(-1, -1),
    18 => const Offset(0, -1),
    21 => const Offset(1, -1),
    _ => Offset(math.cos(step * math.pi / 12), math.sin(step * math.pi / 12)),
  };
  final projection =
      (delta.dx * direction.dx + delta.dy * direction.dy) /
      direction.distanceSquared;
  final result = start + direction * projection;
  return result.dx.isFinite && result.dy.isFinite ? result : end;
}
