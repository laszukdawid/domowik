import 'dart:typed_data';

import 'package:flutter/scheduler.dart';

/// Private, uninstalled packet data. Never upload mutable meshes separately.
class PreparedScenePacket {
  const PreparedScenePacket(this.meshes, this.nodes);
  final Map<String, Object?> meshes;
  final List<Map<String, Object?>> nodes;
}

/// Converts full-scene geometry and instance data in short CPU slices. Checks
/// elapsed work at most every 8192 floats and yields to an actual Flutter frame
/// after [workBudget]; even a single very large mesh cannot monopolize a frame.
/// Cancellation discards the partial packet without touching the presenter.
/// Budgets are cooperative, not a hard real-time guarantee. Camera derivation
/// stays with the view, after the final yield, to use current navigation state.
Future<PreparedScenePacket?> prepareScenePacket(
  Map<String, Object?> scene, {
  required bool Function() current,
  Duration workBudget = const Duration(milliseconds: 2),
  Future<void> Function()? yieldFrame,
}) async {
  final clock = Stopwatch()..start();
  Future<bool> checkpoint() async {
    if (!current()) return false;
    if (clock.elapsed >= workBudget) {
      await (yieldFrame?.call() ?? SchedulerBinding.instance.endOfFrame);
      clock.reset();
    }
    return current();
  }

  Future<Float32List?> floats(List values) async {
    if (!await checkpoint()) return null;
    final result = Float32List(values.length);
    for (var offset = 0; offset < values.length;) {
      final end = (offset + 8192).clamp(0, values.length);
      for (; offset < end; offset++) {
        result[offset] = (values[offset] as num).toDouble();
      }
      if (!await checkpoint()) return null;
    }
    return result;
  }

  if (!current()) return null;
  final meshes = <String, Object?>{};
  if (scene['instanceOnly'] != true) {
    for (final entry in (scene['meshes'] as Map<String, Object?>).entries) {
      final vertices = await floats(entry.value as List);
      if (vertices == null) return null;
      meshes[entry.key] = {'vertices': vertices};
    }
  }
  final nodes = <Map<String, Object?>>[];
  for (final item in scene['nodes'] as List) {
    final node = <String, Object?>{
      'mesh': item['mesh'],
      'transparent': item['transparent'],
    };
    for (final key in ['matrix', 'normalMatrix', 'color']) {
      final values = await floats(item[key] as List);
      if (values == null) return null;
      node[key] = values;
    }
    nodes.add(node);
  }
  return current() ? PreparedScenePacket(meshes, nodes) : null;
}
