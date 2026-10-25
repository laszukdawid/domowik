import 'dart:typed_data';

import 'package:flutter/scheduler.dart';

/// Private, uninstalled packet data. Never upload mutable meshes separately.
class PreparedScenePacket {
  const PreparedScenePacket(this.meshes, this.nodes);
  final Map<String, Object?> meshes;
  final List<Map<String, Object?>> nodes;
}

/// Reuses arrays already prepared by the native worker. Other hosts convert
/// geometry and instance data in short CPU slices, without a Future per array.
/// Checks elapsed work every conversion block (at most 8192 floats) and yields
/// to an actual Flutter frame
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
  if (!current()) return null;
  final prepared = scene['presenterMeshes'];
  if (prepared is Map<String, Object?>) {
    return PreparedScenePacket(
      prepared,
      (scene['nodes'] as List).cast<Map<String, Object?>>(),
    );
  }
  final clock = Stopwatch()..start();
  final meshes = <String, Object?>{};
  final nodes = <Map<String, Object?>>[];

  Iterable<void> floats(List values, void Function(Float32List) put) sync* {
    if (values is Float32List) {
      put(values);
      return;
    }
    final result = Float32List(values.length);
    for (var offset = 0; offset < values.length;) {
      final end = (offset + 8192).clamp(0, values.length);
      for (; offset < end; offset++) {
        result[offset] = (values[offset] as num).toDouble();
      }
      yield null;
    }
    put(result);
  }

  Iterable<void> convert() sync* {
    if (scene['instanceOnly'] != true) {
      for (final entry in (scene['meshes'] as Map<String, Object?>).entries) {
        yield* floats(
          entry.value as List,
          (v) => meshes[entry.key] = {'vertices': v},
        );
        yield null;
      }
    }
    for (final item in scene['nodes'] as List) {
      final node = <String, Object?>{
        'mesh': item['mesh'],
        'transparent': item['transparent'],
      };
      for (final key in ['matrix', 'normalMatrix', 'color']) {
        yield* floats(item[key] as List, (v) => node[key] = v);
      }
      nodes.add(node);
      yield null;
    }
  }

  for (final _ in convert()) {
    if (!current()) return null;
    if (clock.elapsed >= workBudget) {
      await (yieldFrame?.call() ?? SchedulerBinding.instance.endOfFrame);
      clock.reset();
    }
  }
  return current() ? PreparedScenePacket(meshes, nodes) : null;
}
