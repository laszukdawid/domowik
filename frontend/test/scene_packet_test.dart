import 'dart:typed_data';
import 'dart:isolate';

import 'package:domowik_editor/scene_packet.dart';
import 'package:domowik_editor/scene_packet_data.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  Map<String, Object?> scene() => {
    'meshes': {
      'large': List.generate(20000, (i) => i / 10),
      'small': [1, 2, 3],
    },
    'nodes': [
      {
        'mesh': 'large',
        'transparent': false,
        'matrix': List.filled(16, 1),
        'normalMatrix': List.filled(9, 2),
        'color': [1, 0.5, 0, 1],
      },
    ],
  };

  test(
    'large meshes yield within a mesh and produce one complete packet',
    () async {
      var yields = 0;
      final input = scene();
      final packet = await prepareScenePacket(
        input,
        current: () => true,
        workBudget: Duration.zero,
        yieldFrame: () async {
          yields++;
        },
      );
      expect(yields, greaterThan(3));
      expect(packet!.meshes.keys, ['large', 'small']);
      final vertices =
          (packet.meshes['large'] as Map)['vertices'] as Float32List;
      expect(vertices, Float32List.fromList((input['meshes'] as Map)['large']));
      expect(packet.nodes.single['matrix'], isA<Float32List>());
      expect(packet.nodes.single['normalMatrix'], hasLength(9));
      expect(packet.nodes.single['color'], [1, 0.5, 0, 1]);
      expect((input['meshes'] as Map)['large'], isNot(isA<Float32List>()));
    },
  );

  test(
    'one large mesh yields more than just before and after conversion',
    () async {
      var yields = 0;
      final packet = await prepareScenePacket(
        {
          'meshes': {'large': List.filled(20000, 1.0)},
          'nodes': [],
        },
        current: () => true,
        workBudget: Duration.zero,
        yieldFrame: () async {
          yields++;
        },
      );
      expect(packet, isNotNull);
      expect(yields, greaterThan(2));
    },
  );

  test(
    'context changes during conversion discard the partial packet',
    () async {
      var valid = true, yields = 0;
      final packet = await prepareScenePacket(
        scene(),
        current: () => valid,
        workBudget: Duration.zero,
        yieldFrame: () async {
          if (++yields == 2) valid = false;
        },
      );
      expect(packet, isNull);
      expect(yields, 2);
    },
  );

  test(
    'native worker arrays cross isolates and are reused without UI copies',
    () async {
      final input = scene();
      final prepared = await Isolate.run(() => prepareSceneArrays(input));
      final vertices = (prepared['meshes'] as Map)['large'];
      final matrix = (prepared['nodes'] as List).single['matrix'];
      expect(vertices, isA<Float32List>());
      expect(matrix, isA<Float32List>());
      expect((input['meshes'] as Map)['large'], isNot(isA<Float32List>()));
      final packet = await prepareScenePacket(
        prepared,
        current: () => true,
        workBudget: const Duration(days: 1),
        yieldFrame: () async {
          fail('prepared arrays must not need conversion');
        },
      );
      expect(packet!.meshes, same(prepared['presenterMeshes']));
      expect((packet.meshes['large'] as Map)['vertices'], same(vertices));
      expect(packet.nodes.single['matrix'], same(matrix));
    },
  );

  test(
    'instance-only scenes never reinterpret resident meshes as uploads',
    () async {
      final input = scene()..['instanceOnly'] = true;
      input['meshes'] = {'large': null};
      final packet = await prepareScenePacket(
        input,
        current: () => true,
        yieldFrame: () async {},
      );
      expect(packet!.meshes, isEmpty);
      expect(packet.nodes, hasLength(1));
    },
  );
}
