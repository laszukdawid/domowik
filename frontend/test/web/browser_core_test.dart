@TestOn('browser')
library;

import 'dart:convert';

import 'package:domowik_editor/core/domowik_core.dart';
import 'package:domowik_editor/core/web_core.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  late DomowikCore core;
  setUpAll(() async {
    core = await WebCore.instantiate(await fetchBytes('domowik_bridge.wasm'));
  });

  test('the browser core is the ABI 21 bridge and round-trips a plan', () {
    expect(core.abiVersion(), 21);
    final plan = core.createDefault();
    expect(plan.code, ok);
    final snapshot = jsonDecode(core.snapshot(plan.handle).text!) as Map;
    final level = (snapshot['levels'] as List).single['id'];
    final edit = core.edit(
      plan.handle,
      jsonEncode({
        'kind': 'addWall',
        'levelId': level,
        'start': [0, 0],
        'end': [4000, 0],
        'thicknessMm': 200,
        'heightMm': 2500,
      }),
    );
    expect(edit.code, ok, reason: core.lastError(plan.handle));
    final encoded = core.encode(plan.handle);
    expect(encoded.code, ok);
    final reopened = core.openBytes(encoded.bytes!);
    expect(reopened.code, ok);
    final walls =
        (jsonDecode(core.snapshot(reopened.handle).text!) as Map)['walls']
            as List;
    expect(walls, hasLength(1));
    expect(core.close(plan.handle), ok);
    expect(core.close(reopened.handle), ok);
    expect(core.outstandingBuffers(), 0);
    expect(core.openPlans(), 0);
  });

  test(
    'door color and glass style use one undoable browser-core edit and scene',
    () {
      final plan = core.createDefault();
      expect(plan.code, ok);
      try {
        Map snapshot() => jsonDecode(core.snapshot(plan.handle).text!) as Map;
        void edit(Map<String, Object?> request) => expect(
          core.edit(plan.handle, jsonEncode(request)).code,
          ok,
          reason: core.lastError(plan.handle),
        );
        final level = (snapshot()['levels'] as List).single['id'];
        edit({
          'kind': 'addWall',
          'levelId': level,
          'start': [0, 0],
          'end': [4000, 0],
          'thicknessMm': 200,
          'heightMm': 2500,
        });
        final wall = (snapshot()['walls'] as List).single['id'];
        edit({
          'kind': 'addOpening',
          'hostWallId': wall,
          'opening': 'door',
          'u': 0.5,
          'widthMm': 900,
          'heightMm': 2100,
        });
        final before = snapshot();
        final id = (before['openings'] as List).single['id'];
        edit({
          'kind': 'setDoorAppearance',
          'openingId': id,
          'color': '#dcc8b4',
          'doorStyle': 'glass',
        });
        final after = snapshot();
        final opening = (after['openings'] as List).single;
        expect(opening['color'], '#DCC8B4');
        expect(opening['doorStyle'], 'glass');
        final scene = jsonDecode(
          core
              .assets(
                plan.handle,
                jsonEncode({
                  'kind': 'scene',
                  'levelIds': [level],
                }),
              )
              .text!,
        ) as Map;
        final leaf = (scene['nodes'] as List).singleWhere(
          (n) => n['id'] == 'door:$id',
        );
        expect(leaf['transparent'], isTrue);
        expect((leaf['color'] as List)[3], 0.25);
        expect(core.undo(plan.handle).code, ok);
        expect(snapshot(), before);
        expect(core.redo(plan.handle).code, ok);
        expect(snapshot(), after);
      } finally {
        expect(core.close(plan.handle), ok);
      }
      expect(core.outstandingBuffers(), 0);
    },
  );

  test('bulk wall dimensions use one undoable browser-core edit', () {
    final plan = core.createDefault();
    expect(plan.code, ok);
    Map snapshot() => jsonDecode(core.snapshot(plan.handle).text!) as Map;
    final level = (snapshot()['levels'] as List).single['id'];
    for (final y in [0, 1000]) {
      expect(
        core
            .edit(
              plan.handle,
              jsonEncode({
                'kind': 'addWall',
                'levelId': level,
                'start': [0, y],
                'end': [4000, y],
                'thicknessMm': 200,
                'heightMm': y == 0 ? 2500 : 2800,
              }),
            )
            .code,
        ok,
      );
    }
    final before = snapshot();
    final ids = (before['walls'] as List).map((w) => w['id']).toList();
    expect(
      core
          .edit(
            plan.handle,
            jsonEncode({
              'kind': 'setWalls',
              'wallIds': ids,
              'heightMm': 3000,
              'thicknessMm': 180,
            }),
          )
          .code,
      ok,
      reason: core.lastError(plan.handle),
    );
    final after = snapshot();
    for (final wall in after['walls'] as List) {
      expect(wall['heightMm'], 3000);
      expect(wall['thicknessMm'], 180);
    }
    expect(core.undo(plan.handle).code, ok);
    expect(snapshot(), before);
    expect(core.redo(plan.handle).code, ok);
    expect(snapshot(), after);
    expect(core.close(plan.handle), ok);
    expect(core.outstandingBuffers(), 0);
    expect(core.openPlans(), 0);
  });
}
