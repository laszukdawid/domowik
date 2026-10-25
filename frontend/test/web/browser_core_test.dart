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

  test('the browser core is the ABI 19 bridge and round-trips a plan', () {
    expect(core.abiVersion(), 19);
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
