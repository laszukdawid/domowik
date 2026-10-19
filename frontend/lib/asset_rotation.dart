import 'dart:math' as math;

import 'package:flutter/painting.dart';

import 'asset_library.dart';
import 'plan_viewport.dart';

/// Screen-space affordance outside the core-supplied footprint, in logical
/// pixels. This is UI layout only; Rust still derives the rotated footprint.
Offset assetRotationHandlePosition(AssetFootprint asset, PlanViewport view) {
  final center = view.toScreen(asset.position);
  final angle = asset.rotationDeg * math.pi / 180;
  final edge = asset.corners.length == 4
      ? view.toScreen((asset.corners[2] + asset.corners[3]) / 2) - center
      : Offset.zero;
  final distance = edge.distance;
  final direction = distance > 0
      ? edge / distance
      : Offset(-math.sin(angle), -math.cos(angle));
  return center + direction * math.max(40, distance + 28);
}

/// Transient pointer-to-yaw mapping in plan millimetres (Y up). Angles are
/// continuous across the atan2 branch cut and resolved to tenths of a degree.
/// With [update]'s snap option, output is the nearest absolute multiple of 15°
/// (half steps away from zero). Snapping never changes the accumulated angle,
/// so releasing Shift restores free rotation without drift.
/// Positions within one logical pixel of the center have no usable heading
/// and leave the last angle unchanged. No document geometry is derived here.
class AssetRotationDrag {
  AssetRotationDrag({
    required this.center,
    required Offset start,
    required double angleDeg,
  }) : _heading = (start - center).direction,
       _angleDeg = angleDeg;

  final Offset center;
  double _heading, _angleDeg;

  double update(
    Offset cursor, {
    required double zoom,
    bool snapTo15Degrees = false,
  }) {
    final delta = cursor - center;
    if (delta.distance * zoom >= 1) {
      final heading = delta.direction;
      final turn = heading - _heading;
      _angleDeg += math.atan2(math.sin(turn), math.cos(turn)) * 180 / math.pi;
      _heading = heading;
    }
    return snapTo15Degrees
        ? (_angleDeg / 15).roundToDouble() * 15
        : (_angleDeg * 10).roundToDouble() / 10;
  }
}
