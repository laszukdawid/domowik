// The browser 3D presenter on WebGPU (issue 0088): the shared renderer module
// fed scene frames by the WebAssembly core. Needs a WebGPU adapter, which
// headless Chrome for Testing offers on hosts with a GPU.
@TestOn('browser')
library;

import 'dart:convert';
import 'dart:js_interop';
import 'dart:typed_data';

import 'package:domowik_editor/core/domowik_core.dart';
import 'package:domowik_editor/core/web_core.dart';
import 'package:domowik_editor/platform/scene_presenter_web.dart';
import 'package:domowik_editor/three_d_view.dart';
import 'package:domowik_editor/web_graphics_policy.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart' show PlatformException;
import 'package:flutter_test/flutter_test.dart';

@JS('navigator.gpu')
external JSObject? get _gpu;

void main() {
  late DomowikCore core;
  setUpAll(() async {
    core = await WebCore.instantiate(await fetchBytes('domowik_bridge.wasm'));
  });

  String addWall(int handle, String level, List<num> a, List<num> b) {
    final reply = core.edit(
      handle,
      jsonEncode({
        'kind': 'addWall',
        'levelId': level,
        'start': a,
        'end': b,
        'thicknessMm': 200,
        'heightMm': 2500,
      }),
    );
    expect(reply.code, ok, reason: core.lastError(handle));
    return reply.text!;
  }

  test('safe mode refuses create before any renderer initialization', () async {
    final presenter = WebScenePresenter(
      core,
      graphicsPolicy: const WebGraphicsPolicy.safe2d('Safe 2D test policy'),
    );
    await expectLater(
      presenter.invokeMapMethod<String, Object?>('create'),
      throwsA(
        isA<PlatformException>()
            .having((e) => e.code, 'code', 'webgpu-disabled')
            .having((e) => e.message, 'message', 'Safe 2D test policy'),
      ),
    );
    await expectLater(
      presenter.invokeMethod<void>('stats', {'session': 1}),
      throwsA(isA<PlatformException>()),
    );
  });

  test('scenes load from the core; navigation and pose updates reuse GPU geometry', () async {
    expect(_gpu, isNotNull, reason: 'this browser offers no WebGPU');
    final plan = core.createDefault().handle;
    addTearDown(() => core.close(plan));
    final level =
        ((jsonDecode(core.snapshot(plan).text!) as Map)['levels'] as List)
                .single['id']
            as String;
    addWall(plan, level, [0, 0], [4000, 0]);
    addWall(plan, level, [4000, 0], [4000, 3000]);

    final presenter = WebScenePresenter(core);
    final created = (await presenter.invokeMapMethod<String, Object?>(
      'create',
    ))!;
    expect(created['loadScene'], isTrue);
    final session = created['session'];
    addTearDown(
      () => presenter.invokeMethod<void>('destroy', {'session': session}),
    );

    Future<Map<String, Object?>> load(int generation) async =>
        (await presenter.invokeMapMethod<String, Object?>('loadScene', {
          'session': session,
          'plan': plan,
          'generation': generation,
          'levelIds': [level],
          'ghostedIds': const [],
        }))!;
    Future<Map<String, Object?>> stats() async => (await presenter
        .invokeMapMethod<String, Object?>('stats', {'session': session}))!;
    Future<void> camera(double turn) => presenter.invokeMethod<void>('camera', {
      'session': session,
      'matrix': [
        1.0, 0.0, 0.0, 0.0, //
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, -0.1, -1.0,
        turn, 0.0, -1.0, 4.0,
      ],
      'width': 320,
      'height': 240,
    });

    final scene = await load(1);
    expect(scene['generation'], 1);
    expect(scene['radius'], greaterThan(0));
    expect((scene['originMm'] as List), hasLength(2));
    await camera(0);
    final first = await stats();
    expect(first['triangles'], greaterThan(0));
    expect(first['frames'], 1);
    expect(first['error'], isEmpty);

    for (var i = 1; i <= 20; i++) {
      await camera(i / 20);
    }
    final navigated = await stats();
    expect(navigated['frames'], 21);
    expect(navigated['cameraUpdates'], 21);
    expect(navigated['meshUploads'], first['meshUploads']);
    expect(navigated['instanceUploads'], first['instanceUploads']);
    expect(navigated['sceneUpdates'], first['sceneUpdates']);

    // An edit reloads the scene; unchanged meshes are not uploaded again.
    addWall(plan, level, [0, 3000], [4000, 3000]);
    await load(2);
    await camera(0);
    final edited = await stats();
    expect(edited['sceneUpdates'], (first['sceneUpdates'] as int) + 1);
    expect(edited['triangles'], greaterThan(first['triangles'] as int));
    expect(edited['error'], isEmpty);
    expect(core.outstandingBuffers(), 0);
    // Keep using this real WebGPU session to test pose streaming after a
    // scene edit, rather than allocating another device in headless Chrome.
    final imported = core.importModel(
      plan,
      'tv.obj',
      Uint8List.fromList(utf8.encode('v 0 0 0\nv 2 0 0\nv 0 1 2\nf 1 2 3\n')),
    );
    expect(imported.code, ok);
    final placed = core.assets(
      plan,
      jsonEncode({
        'kind': 'place',
        'digest': jsonDecode(imported.text!)['digest'],
        'levelId': level,
        'positionMm': [2000, 1000],
      }),
    );
    expect(placed.code, ok);
    final id = jsonDecode(placed.text!)['instanceId'];
    var generation = 2;
    Future<Map<String, Object?>> loadPose({Object? token}) async =>
        (await presenter.invokeMapMethod<String, Object?>('loadScene', {
          'session': session,
          'plan': plan,
          'generation': ++generation,
          'levelIds': [level],
          'ghostedIds': <String>[],
          'poseOnly': token != null,
          'sceneToken': token,
        }))!;
    final full = await loadPose();
    await camera(0);
    final before = await stats();
    expect(before['triangles'], greaterThan(0));
    var updates = before['instanceUploads'] as int;
    final timing = Stopwatch()..start();
    for (var i = 1; i <= 20; i++) {
      final angle = i * 5;
      expect(
        core
            .assets(
              plan,
              jsonEncode({
                'kind': 'pose',
                'instanceId': id,
                'positionMm': [2000, 1000, 1500],
                'rotationDeg': [angle, 10, 30],
                'preview': true,
              }),
            )
            .code,
        ok,
      );
      final result = await loadPose(token: full['sceneToken']);
      expect(result['instanceOnly'], true);
      await camera(0);
      final after = await stats();
      expect(after['meshUploads'], before['meshUploads']);
      expect(
        after['triangles'],
        before['triangles'],
        reason: 'names-only frames must not replace resident geometry with empty meshes',
      );
      expect(
        after['instanceUploads'],
        ++updates,
        reason: 'only the model group changes, not wall instances',
      );
      expect(after['error'], isEmpty);
    }
    timing.stop();
    print(
      'POSE browser core+transport+GPU submission: ${timing.elapsedMicroseconds / 20} us/update; 20 warm updates, not GPU/presentation latency',
    );
    expect(
      core.assets(plan, jsonEncode({'kind': 'clearPosePreview'})).code,
      ok,
    );
    await loadPose(token: full['sceneToken']);
    expect((await stats())['meshUploads'], before['meshUploads']);
    expect(core.outstandingBuffers(), 0);
  });

  testWidgets('uploading a missing model reloads the active 3D view', (
    tester,
  ) async {
    late Uint8List bytes;
    await tester.runAsync(() async {
      bytes = await fetchBytes('fixtures/optional-missing-model.domo');
    });
    final plan = core.openBytes(bytes).handle;
    expect(plan, isNonZero);
    Map<String, Object?> state() =>
        jsonDecode(core.state(plan).text!) as Map<String, Object?>;
    final level =
        ((jsonDecode(core.snapshot(plan).text!) as Map)['levels'] as List)
                .single['id']
            as String;
    final key = GlobalKey<ThreeDViewState>();
    await tester.binding.setSurfaceSize(const Size(1000, 800));
    addTearDown(() async {
      await tester.pumpWidget(const SizedBox.shrink());
      core.close(plan);
      await tester.binding.setSurfaceSize(null);
    });
    // As the editor builds it: both revisions straight from the core state.
    Future<void> show() async {
      final s = state();
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: ThreeDView(
              key: key,
              core: core,
              handle: plan,
              revision: s['document_revision'] as int,
              assetRevision: s['asset_revision'] as int,
              levelIds: [level],
              ghostedIds: const [],
              floorElevationMm: 0,
              active: true,
            ),
          ),
        ),
      );
      await tester.pump();
    }

    Future<Map<String, Object?>> settled(int sceneUpdates) async {
      for (var i = 0; i < 100; i++) {
        await tester.runAsync(
          () => Future<void>.delayed(const Duration(milliseconds: 50)),
        );
        await tester.pump();
        final view = key.currentState;
        if (view?.session != null && !view!.loading) {
          final stats = await view.stats();
          if ((stats['sceneUpdates'] as int) >= sceneUpdates &&
              (stats['frames'] as int) > 0) {
            return stats;
          }
        }
      }
      throw StateError('3D not ready: ${key.currentState?.diagnosticError}');
    }

    await show();
    final placeholder = await settled(1);
    final model = Uint8List.fromList(
      utf8.encode('v 0 0 0\nv 2 0 0\nv 0 1 2\nf 1 2 3\n'),
    );
    final revision = state()['document_revision'];
    expect(core.importModel(plan, 'chair.obj', model).code, ok);
    expect(state()['document_revision'], revision, reason: 'no batch');
    await show();
    final restored = await settled(2);
    expect(restored['sceneUpdates'], (placeholder['sceneUpdates'] as int) + 1);
    expect(
      restored['triangles'],
      lessThan(placeholder['triangles'] as int),
      reason: 'the one-triangle model replaces the placeholder box',
    );
    expect(core.outstandingBuffers(), 0);
    expect(tester.takeException(), isNull);
  });

  test('an unknown session and an unknown plan are refused', () async {
    final presenter = WebScenePresenter(core);
    expect(
      () => presenter.invokeMethod<void>('camera', {'session': 999}),
      throwsA(anything),
    );
    final created = (await presenter.invokeMapMethod<String, Object?>(
      'create',
    ))!;
    final session = created['session'];
    await expectLater(
      presenter.invokeMapMethod<String, Object?>('loadScene', {
        'session': session,
        'plan': 424242,
        'generation': 1,
        'levelIds': const <String>[],
        'ghostedIds': const <String>[],
      }),
      throwsA(anything),
    );
    await presenter.invokeMethod<void>('destroy', {'session': session});
  });
}
