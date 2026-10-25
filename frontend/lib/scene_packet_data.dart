import 'dart:typed_data';

/// Converts presentation data only, never authoritative geometry. Used in the
/// native scene worker before returning data to the UI isolate. Float32 arrays
/// replace the decoded number lists, avoiding a second large UI-side copy.
Map<String, Object?> prepareSceneArrays(Map<String, Object?> scene) {
  final meshes = <String, Object?>{
    if (scene['instanceOnly'] != true)
      for (final entry in (scene['meshes'] as Map<String, Object?>).entries)
        entry.key: sceneFloats(entry.value as List),
  };
  final nodes = [
    for (final item in scene['nodes'] as List)
      <String, Object?>{
        'mesh': item['mesh'],
        'transparent': item['transparent'],
        for (final key in ['matrix', 'normalMatrix', 'color'])
          key: sceneFloats(item[key] as List),
      },
  ];
  return {
    ...scene,
    if (scene['instanceOnly'] != true) 'meshes': meshes,
    'nodes': nodes,
    // The UI can submit these dictionaries directly. Do not traverse thousands
    // of already-prepared arrays again merely to wrap each in a vertices map.
    'presenterMeshes': {
      for (final entry in meshes.entries) entry.key: {'vertices': entry.value},
    },
  };
}

/// Owned Float32 presentation values, or the already-prepared array unchanged.
Float32List sceneFloats(List values) {
  if (values is Float32List) return values;
  final result = Float32List(values.length);
  for (var i = 0; i < values.length; i++) {
    result[i] = (values[i] as num).toDouble();
  }
  return result;
}
