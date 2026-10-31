import 'dart:async';
import 'dart:convert';
import 'dart:ffi' hide Size;
import 'dart:io';
import 'dart:math' as math;
import 'dart:typed_data';

import 'package:domowik_bridge_plugin/domowik_bridge_plugin.dart';
import 'package:domowik_editor/core/ffi_core.dart';
import 'package:domowik_editor/core/domowik_core.dart' show DomowikCore;
import 'package:domowik_editor/editor_shell.dart';
import 'package:domowik_editor/three_d_view.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('save shortcuts escape 3D navigation without moving the camera', (
    tester,
  ) async {
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      ThreeDViewState.channel,
      (call) async =>
          call.method == 'create' ? {'textureId': 71, 'session': 1} : null,
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        ThreeDViewState.channel,
        null,
      ),
    );
    final key = GlobalKey<ThreeDViewState>();
    var saves = 0;
    await tester.pumpWidget(
      MaterialApp(
        home: EditorShell(
          fileName: 'plan.domo',
          onNew: () {},
          onOpen: () {},
          onSave: () => saves++,
          onSaveAs: () {},
          onUndo: null,
          onRedo: null,
          tools: const SizedBox(),
          inspector: const SizedBox(),
          canvas: ThreeDView(
            key: key,
            core: _SceneCore({
              'originMm': [0, 0],
              'center': [0, 0, 0],
              'radius': 1,
              'meshes': <String, Object?>{},
              'requiredModels': <String>[],
              'nodes': <Object?>[],
              'diagnostics': <String>[],
            }),
            handle: 1,
            revision: 1,
            levelIds: const ['level'],
            ghostedIds: const [],
            floorElevationMm: 0,
            active: true,
          ),
          status: 'ready',
          statusIsError: false,
        ),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byType(Texture));
    await tester.tap(find.byKey(const ValueKey('3d-walk')));
    await tester.pumpAndSettle();
    final eye = List.of(key.currentState!.camera.eye);
    for (final modifier in [
      LogicalKeyboardKey.controlLeft,
      LogicalKeyboardKey.metaLeft,
    ]) {
      await tester.sendKeyDownEvent(modifier);
      await tester.sendKeyDownEvent(LogicalKeyboardKey.keyS);
      await tester.pump(const Duration(milliseconds: 16));
      await tester.pump(const Duration(milliseconds: 100));
      await tester.sendKeyUpEvent(LogicalKeyboardKey.keyS);
      await tester.sendKeyUpEvent(modifier);
      await tester.pumpAndSettle();
    }
    expect(saves, 2);
    expect(key.currentState!.camera.eye, eye);
    // Bare S still walks backwards.
    await tester.sendKeyDownEvent(LogicalKeyboardKey.keyS);
    await tester.pump(const Duration(milliseconds: 16));
    await tester.pump(const Duration(milliseconds: 100));
    await tester.sendKeyUpEvent(LogicalKeyboardKey.keyS);
    await tester.pumpAndSettle();
    expect(saves, 2);
    expect(key.currentState!.camera.eye, isNot(eye));
    await tester.pumpWidget(const SizedBox.shrink());
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'a long stair installs rebased geometry and camera in one scene packet',
    (tester) async {
      final suffix = Platform.isMacOS ? 'dylib' : 'so';
      final bridge = Bridge(
        File('target/release/libdomowik_bridge.$suffix').absolute.path,
      );
      final real = FfiCore(bridge);
      final opened = real.createDefault();
      final handle = opened.handle;
      addTearDown(() => real.close(handle));
      final document = jsonDecode(real.snapshot(handle).text!) as Map;
      final lower = document['levels'][0]['id'] as String;
      const upper = '01950000-0000-7000-8000-000000000602';
      expect(
        real
            .execute(
              handle,
              jsonEncode({
                'operations': [
                  {
                    'operationId': '01950000-0000-7000-8000-000000000601',
                    'entityId': upper,
                    'kind': 'createEntity',
                    'entity': {
                      'type': 'level',
                      'id': upper,
                      'name': 'Upper',
                      'elevationMm': 2600,
                      'floorThicknessMm': 200,
                      'defaultWallHeightMm': 2500,
                      'defaultCeilingHeightMm': 2500,
                    },
                  },
                ],
              }),
            )
            .succeeded,
        isTrue,
      );
      void addStair(List<int> end) {
        expect(
          real
              .edit(
                handle,
                jsonEncode({
                  'kind': 'addDescendingStaircase',
                  'request': {
                    'upperLevelId': upper,
                    'startMm': [0, 0],
                    'endMm': end,
                  },
                }),
              )
              .succeeded,
          isTrue,
        );
      }

      Map<String, Object?> scene() => jsonDecode(
        real
            .assets(
              handle,
              jsonEncode({
                'kind': 'scene',
                'levelIds': [lower, upper],
                'ghostedIds': [],
              }),
            )
            .text!,
      ) as Map<String, Object?>;
      addStair([3780, 0]);
      final first = scene();
      final background = await tester.runAsync(
        () => real.assetsInBackground(handle, {
          'kind': 'scene',
          'levelIds': [lower, upper],
          'ghostedIds': [],
        }),
      );
      final prepared = background as Map<String, Object?>;
      expect((prepared['meshes'] as Map).values.first, isA<Float32List>());
      expect((prepared['nodes'] as List).first['matrix'], isA<Float32List>());
      expect(prepared['originMm'], first['originMm']);
      addStair([40000, 20000]);
      final second = scene();
      expect(first['originMm'], isNot(second['originMm']));
      final core = _SceneCore(first);
      final key = GlobalKey<ThreeDViewState>();
      final packets = <Map>[];
      final cameraPackets = <MethodCall>[];
      final commit = Completer<void>();
      var reject = false;
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        ThreeDViewState.channel,
        (call) async {
          if (call.method == 'create') return {'textureId': 77, 'session': 11};
          if (call.method == 'mesh')
            fail('mutable geometry must not be installed piecemeal');
          if (call.method == 'camera') cameraPackets.add(call);
          if (call.method == 'scene') {
            if (reject) throw PlatformException(code: 'invalid-scene');
            packets.add(call.arguments as Map);
            if (packets.length == 2) await commit.future;
          }
          return null;
        },
      );
      addTearDown(
        () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          ThreeDViewState.channel,
          null,
        ),
      );
      Future<void> build(int revision) async {
        await tester.pumpWidget(
          MaterialApp(
            home: Scaffold(
              body: ThreeDView(
                key: key,
                core: core,
                handle: handle,
                revision: revision,
                levelIds: [lower, upper],
                ghostedIds: const [],
                floorElevationMm: 2600,
                active: true,
              ),
            ),
          ),
        );
        await tester.pumpAndSettle();
      }

      await build(1);
      final oldTarget = List.of(key.currentState!.camera.target);
      final oldDistance = key.currentState!.camera.distance;
      core.scene = second;
      await build(2);
      expect(packets, hasLength(2));
      expect(
        key.currentState!.camera.target,
        oldTarget,
        reason: 'do not adopt pending scene metadata',
      );
      final camerasBefore = cameraPackets.length;
      key.currentState!.cameraChanged();
      await tester.pump();
      expect(
        cameraPackets,
        hasLength(camerasBefore),
        reason: 'no old-origin camera may race the scene commit',
      );
      for (var i = 0; i < 2; i++) {
        expect(
          (packets[i]['meshes'] as Map).keys,
          ((i == 0 ? first : second)['meshes'] as Map).keys,
        );
        expect(packets[i]['matrix'], isA<Float64List>());
      }
      // Check projected vertices of every existing stair mesh, not just stored
      // run lengths. They must stay in place even though the scene origin moves.
      List<double> project(List<num> matrix, List<num> vertices, int at) {
        final point = [vertices[at], vertices[at + 1], vertices[at + 2], 1];
        final clip = List.generate(
          4,
          (r) => List.generate(
            4,
            (c) => matrix[c * 4 + r] * point[c],
          ).reduce((a, b) => a + b),
        );
        return [clip[0] / clip[3], clip[1] / clip[3]];
      }

      final oldMeshes = packets[0]['meshes'] as Map,
          newMeshes = packets[1]['meshes'] as Map;
      for (final id in oldMeshes.keys.where((id) => '$id'.contains('stair:'))) {
        final a = oldMeshes[id]['vertices'] as List<num>,
            b = newMeshes[id]['vertices'] as List<num>;
        expect(a.length, b.length);
        for (var j = 0; j < a.length; j += 10) {
          final before = project(packets[0]['matrix'] as List<num>, a, j),
              after = project(packets[1]['matrix'] as List<num>, b, j);
          for (var k = 0; k < 2; k++) {
            expect(after[k], closeTo(before[k], 0.00001));
          }
        }
      }
      expect(oldMeshes.keys.any((id) => '$id'.contains('stair:')), isTrue);
      commit.complete();
      await tester.pumpAndSettle();
      expect(key.currentState!.camera.distance, oldDistance);
      final installedTarget = List.of(key.currentState!.camera.target);
      expect(installedTarget, isNot(oldTarget));
      // Rejection must leave the displayed scene's camera/coordinate system alone.
      reject = true;
      core.scene = first;
      await build(3);
      expect(key.currentState!.camera.target, installedTarget);
      expect(key.currentState!.diagnosticError, contains('invalid-scene'));
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.pumpAndSettle();
      expect(real.outstandingBuffers(), 0);
      expect(tester.takeException(), isNull);
    },
  );
  testWidgets('embedded texture bytes cross the Linux presenter channel once', (
    tester,
  ) async {
    final calls = <MethodCall>[];
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      ThreeDViewState.channel,
      (call) async {
        calls.add(call);
        if (call.method == 'create') return {'textureId': 74, 'session': 6};
        return null;
      },
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        ThreeDViewState.channel,
        null,
      ),
    );
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: ThreeDView(
            core: const _TextureCore(),
            handle: 7,
            revision: 1,
            levelIds: const ['level'],
            ghostedIds: const [],
            floorElevationMm: 0,
            active: true,
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    final mesh = calls.singleWhere((c) => c.method == 'mesh').arguments as Map;
    final texture = mesh['texture'] as Map;
    expect(texture['rgba'], isA<Uint8List>());
    expect(texture['rgba'], [255, 0, 0, 255]);
    expect(texture['coordinates'], isA<Float32List>());
    expect(texture['width'], 1);
    await tester.drag(find.byType(Texture), const Offset(50, 30));
    await tester.pumpAndSettle();
    expect(calls.where((c) => c.method == 'mesh'), hasLength(1));
    await tester.pumpWidget(const SizedBox.shrink());
    await tester.pumpAndSettle();
  });
  for (final code in ['no-webgpu', 'webgpu-disabled']) {
    testWidgets('$code startup refusal notifies once and never retries', (
      tester,
    ) async {
      var creates = 0;
      final failures = <String>[];
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        ThreeDViewState.channel,
        (call) async {
          expect(call.method, 'create');
          creates++;
          throw PlatformException(code: code, message: 'adapter unavailable');
        },
      );
      addTearDown(
        () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          ThreeDViewState.channel,
          null,
        ),
      );
      Future<void> show(int revision, {bool active = true}) async {
        await tester.pumpWidget(
          MaterialApp(
            home: Scaffold(
              body: ThreeDView(
                core: const _UnusedCore(),
                handle: revision,
                revision: revision,
                levelIds: const ['level'],
                ghostedIds: const [],
                floorElevationMm: 0,
                active: active,
                onUnavailable: failures.add,
              ),
            ),
          ),
        );
        await tester.pumpAndSettle();
      }

      await show(1);
      expect(failures, hasLength(1));
      expect(failures.single, contains('adapter unavailable'));
      await show(2);
      await show(3, active: false);
      await show(4);
      expect(creates, 1);
      expect(failures, hasLength(1));
      expect(tester.takeException(), isNull);
    });
  }

  testWidgets('a startup refusal after disposal never notifies the host', (
    tester,
  ) async {
    final pending = Completer<Object?>();
    final failures = <String>[];
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      ThreeDViewState.channel,
      (call) async => pending.future,
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        ThreeDViewState.channel,
        null,
      ),
    );
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: ThreeDView(
            core: const _UnusedCore(),
            handle: 1,
            revision: 1,
            levelIds: const ['level'],
            ghostedIds: const [],
            floorElevationMm: 0,
            active: true,
            onUnavailable: failures.add,
          ),
        ),
      ),
    );
    await tester.pumpWidget(const SizedBox.shrink());
    pending.completeError(PlatformException(code: 'no-webgpu'));
    await tester.pumpAndSettle();
    expect(failures, isEmpty);
    expect(tester.takeException(), isNull);
  });

  for (final failingMethod in ['create', 'loadScene']) {
    testWidgets('$failingMethod errors are selectable and fully copyable', (
      tester,
    ) async {
      final details = List.filled(
        30,
        'No suitable WebGPU graphics adapter.',
      ).join('\n');
      final key = GlobalKey<ThreeDViewState>();
      String? copied;
      var clipboardFails = false;
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        ThreeDViewState.channel,
        (call) async {
          if (call.method == failingMethod) {
            throw PlatformException(code: 'no-webgpu', message: details);
          }
          if (call.method == 'create') {
            return {'textureId': 74, 'session': 6, 'loadScene': true};
          }
          return null;
        },
      );
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        (call) async {
          if (call.method == 'Clipboard.setData') {
            if (clipboardFails)
              throw PlatformException(code: 'clipboard-denied');
            copied = (call.arguments as Map)['text'] as String;
          }
          return null;
        },
      );
      addTearDown(() {
        tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          ThreeDViewState.channel,
          null,
        );
        tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          SystemChannels.platform,
          null,
        );
      });
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: ThreeDView(
              key: key,
              core: const _UnusedCore(),
              handle: 7,
              revision: 1,
              levelIds: const ['level'],
              ghostedIds: const [],
              floorElevationMm: 0,
              active: true,
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      final error = key.currentState!.diagnosticError!;
      expect(error, contains(details));
      expect(find.textContaining(details), findsNothing);
      expect(find.textContaining('PlatformException'), findsNothing);
      expect(
        find.text(
          failingMethod == 'create'
              ? '3D could not start'
              : '3D could not update',
        ),
        findsOneWidget,
      );
      expect(
        find.textContaining('continue editing and saving your plan in 2D'),
        findsOneWidget,
      );
      await tester.tap(find.text('3D diagnostics'));
      await tester.pumpAndSettle();
      final selection = find.byWidgetPredicate(
        (widget) => widget is SelectableText && widget.data == error,
      );
      expect(selection, findsOneWidget);
      final camera = key.currentState!.camera;
      final eye = camera.eye;
      final start = tester.getTopLeft(selection) + const Offset(8, 8);
      final drag = await tester.startGesture(
        start,
        kind: PointerDeviceKind.mouse,
      );
      await drag.moveBy(const Offset(90, 16));
      await drag.up();
      await tester.pumpAndSettle();
      expect(
        camera.eye,
        eye,
        reason: 'selecting text must not navigate the camera',
      );
      final editable = tester.widget<EditableText>(
        find.descendant(of: selection, matching: find.byType(EditableText)),
      );
      expect(
        editable.focusNode.hasFocus,
        isTrue,
        reason: 'camera focus must not steal text selection',
      );
      expect(editable.controller.selection.isCollapsed, isFalse);

      // The complete message and copy action stay reachable even in a small pane.
      await tester.binding.setSurfaceSize(const Size(320, 240));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpAndSettle();
      final copy = find.text('Copy 3D error');
      await tester.ensureVisible(copy);
      await tester.pumpAndSettle();
      await tester.tap(copy);
      await tester.pumpAndSettle();
      expect(
        copied,
        error,
        reason: 'copy includes the full, untruncated diagnostic',
      );
      expect(tester.takeException(), isNull);

      clipboardFails = true;
      await tester.tap(copy);
      await tester.pumpAndSettle();
      expect(
        find.textContaining('Could not copy the 3D error.'),
        findsOneWidget,
      );
      expect(key.currentState!.diagnosticError, error);
      expect(tester.takeException(), isNull);
    });
  }

  testWidgets(
    'navigation uploads only cameras; revisions rebuild the same plan scene',
    (tester) async {
      final suffix = Platform.isMacOS ? 'dylib' : 'so';
      final image = File('target/release/libdomowik_bridge.$suffix')
          .absolute
          .path;
      final bridge = Bridge(image);
      final output = alloc<Uint64>(sizeOf<Uint64>());
      expect(bridge.planCreateDefault(output), ok);
      final handle = output.value;
      release(output);
      addTearDown(() => bridge.planClose(handle));
      final snapshot = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
      expect(bridge.planSnapshot(handle, snapshot), ok);
      final document = jsonDecode(bridge.takeString(snapshot)) as Map;
      release(snapshot);
      final level = (document['levels'] as List).single['id'] as String;
      void wall() {
        final text = jsonEncode({
          'kind': 'addWall',
          'levelId': level,
          'start': [0, 0],
          'end': [4000, 0],
          'thicknessMm': 200,
          'heightMm': 2500,
        });
        final ptr = toNative(text);
        final out = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
        expect(bridge.planEdit(handle, ptr, utf8.encode(text).length, out), ok);
        bridge.takeString(out);
        release(ptr);
        release(out);
      }

      wall();
      final calls = <String>[];
      final heights = <double>[];
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        ThreeDViewState.channel,
        (call) async {
          calls.add(call.method);
          if (call.method == 'scene') {
            final args = call.arguments as Map;
            final nodes = args['nodes'] as List;
            if (nodes.isNotEmpty)
              heights.add((nodes.first['matrix'] as List)[5] as double);
          }
          if (call.method == 'create') return {'textureId': 71, 'session': 1};
          if (call.method == 'stats') return {'frames': 1};
          return null;
        },
      );
      addTearDown(
        () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          ThreeDViewState.channel,
          null,
        ),
      );
      final key = GlobalKey<ThreeDViewState>();
      Future<void> build(int revision) async {
        await tester.runAsync(() async {
          await tester.pumpWidget(
            MaterialApp(
              home: Scaffold(
                body: ThreeDView(
                  key: key,
                  core: FfiCore(bridge),
                  handle: handle,
                  revision: revision,
                  levelIds: [level],
                  ghostedIds: const [],
                  floorElevationMm: 0,
                  active: true,
                ),
              ),
            ),
          );
          await Future<void>.delayed(const Duration(milliseconds: 200));
        });
        await tester.pumpAndSettle();
      }

      await build(1);
      expect(find.byType(Texture), findsOneWidget);
      expect(calls.where((c) => c == 'scene'), hasLength(1));
      final meshes = calls.where((c) => c == 'mesh').length;
      final reads = key.currentState!.sceneRequests;
      await tester.drag(find.byType(Texture), const Offset(80, 35));
      await tester.pumpAndSettle();
      expect(calls.where((c) => c == 'camera').length, greaterThan(1));
      expect(calls.where((c) => c == 'scene'), hasLength(1));
      expect(calls.where((c) => c == 'mesh'), hasLength(meshes));
      expect(key.currentState!.sceneRequests, reads);
      await tester.tap(find.byKey(const ValueKey('3d-walk')));
      await tester.pumpAndSettle();
      expect(key.currentState!.camera.eye[1], 1.65);
      final eye = key.currentState!.camera.eye;
      await tester.sendKeyDownEvent(LogicalKeyboardKey.keyW);
      await tester.pump(const Duration(milliseconds: 16));
      await tester.pump(const Duration(milliseconds: 100));
      await tester.sendKeyUpEvent(LogicalKeyboardKey.keyW);
      await tester.pumpAndSettle();
      expect(key.currentState!.camera.eye, isNot(eye));
      expect(calls.where((c) => c == 'scene'), hasLength(1));
      final current = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
      expect(bridge.planSnapshot(handle, current), ok);
      final wallId =
          (jsonDecode(bridge.takeString(current)) as Map)['walls'][0]['id'];
      release(current);
      final text = jsonEncode({
        'kind': 'setWall',
        'wallId': wallId,
        'heightMm': 3000,
      });
      final ptr = toNative(text);
      final changed = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
      expect(
        bridge.planEdit(handle, ptr, utf8.encode(text).length, changed),
        ok,
      );
      bridge.takeString(changed);
      release(ptr);
      release(changed);
      await build(2);
      expect(heights, [
        2.5,
        3.0,
      ], reason: 'the rendered scene follows the authoritative edit');
      expect(calls.where((c) => c == 'scene'), hasLength(2));
      expect(find.textContaining('Could not build'), findsNothing);
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.pumpAndSettle();
      expect(calls.last, 'destroy');
      expect(bridge.outstandingBuffers(), 0);
    },
  );

  testWidgets(
    'a Rust-loading presenter gets the plan handle, and a load finished while '
    'hidden is repeated when shown',
    (tester) async {
      final loads = <Map>[];
      var pending = Completer<Object?>();
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        ThreeDViewState.channel,
        (call) async {
          if (call.method == 'create') {
            return {'textureId': 72, 'session': 9, 'loadScene': true};
          }
          if (call.method == 'loadScene') {
            loads.add(call.arguments as Map);
            return pending.future;
          }
          if (call.method == 'mesh' || call.method == 'scene') {
            fail('a Rust-loading presenter must not receive pushed geometry');
          }
          return null;
        },
      );
      addTearDown(
        () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          ThreeDViewState.channel,
          null,
        ),
      );
      Future<void> build({required bool active}) => tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: ThreeDView(
              core: _UnusedCore(),
              handle: 41,
              revision: 1,
              levelIds: const ['level'],
              ghostedIds: const [],
              floorElevationMm: 0,
              active: active,
            ),
          ),
        ),
      );
      final scene = {
        'originMm': [0, 0],
        'center': [0, 1, 0],
        'radius': 5,
        'diagnostics': <String>[],
      };
      await build(active: true);
      await tester.pump();
      expect(loads, hasLength(1));
      expect(loads.single['plan'], 41);
      expect(loads.single['generation'], 1);
      await build(active: false);
      pending.complete(scene);
      await tester.pump();
      expect(loads, hasLength(1), reason: 'nothing loads while hidden');
      pending = Completer<Object?>()..complete(scene);
      await build(active: true);
      await tester.pump();
      expect(loads, hasLength(2), reason: 'the discarded load is repeated');
      expect(loads.last['generation'], 2);
    },
  );

  testWidgets(
    'continuous pose input presents in-flight results and coalesces the latest request',
    (tester) async {
      final key = GlobalKey<ThreeDViewState>();
      final loads = <Map>[];
      final pending = <Completer<Object?>>[];
      var cameras = 0;
      final scene = {
        'originMm': [0, 0],
        'center': [0, 1, 0],
        'radius': 5,
        'diagnostics': <String>[],
        'sceneToken': 11,
      };
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        ThreeDViewState.channel,
        (call) async {
          if (call.method == 'create')
            return {'textureId': 74, 'session': 8, 'loadScene': true};
          if (call.method == 'camera') cameras++;
          if (call.method == 'loadScene') {
            loads.add(call.arguments as Map);
            if (loads.length == 1) return scene;
            final result = Completer<Object?>();
            pending.add(result);
            return result.future;
          }
          return null;
        },
      );
      addTearDown(() {
        for (final result in pending) {
          if (!result.isCompleted) result.complete(scene);
        }
        tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          ThreeDViewState.channel,
          null,
        );
      });
      Future<void> build(int pose, {int revision = 1}) => tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: ThreeDView(
              key: key,
              core: const _UnusedCore(),
              handle: 41,
              revision: revision,
              poseRevision: pose,
              levelIds: const ['level'],
              ghostedIds: const [],
              floorElevationMm: 0,
              active: true,
            ),
          ),
        ),
      );
      await build(0);
      await tester.pumpAndSettle();
      final initialCameras = cameras;
      await build(1);
      await tester.pump();
      expect(loads, hasLength(2));
      expect(loads.last['poseOnly'], true);
      expect(loads.last['sceneToken'], 11);
      for (var i = 2; i <= 20; i++) {
        await build(i);
      }
      expect(loads, hasLength(2), reason: 'only one request may be in flight');
      pending.first.complete(scene);
      await tester.pump();
      await tester.pump();
      expect(
        cameras,
        greaterThan(initialCameras),
        reason:
            'continued input must not discard the visible intermediate result',
      );
      expect(
        loads,
        hasLength(3),
        reason: 'one new request delivers the latest pose, not every intermediate angle',
      );
      final beforeDocumentChange = cameras;
      await build(21, revision: 2);
      pending[1].complete(scene);
      await tester.pump();
      await tester.pump();
      expect(
        cameras,
        beforeDocumentChange,
        reason: 'a document change still invalidates stale results',
      );
      expect(loads.last['poseOnly'], false);
      pending.last.complete({...scene, 'sceneToken': 12});
      await tester.pumpAndSettle();
      expect(key.currentState!.loading, false);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'a presenter failure shows at once and clears on recovery without hiding '
    'other errors',
    (tester) async {
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        ThreeDViewState.channel,
        (call) async {
          if (call.method == 'create') {
            return {'textureId': 73, 'session': 5, 'loadScene': true};
          }
          if (call.method == 'loadScene') {
            return {
              'originMm': [0, 0],
              'center': [0, 1, 0],
              'radius': 5,
              'diagnostics': <String>[],
            };
          }
          return null;
        },
      );
      addTearDown(
        () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          ThreeDViewState.channel,
          null,
        ),
      );
      await tester.pumpWidget(
        const MaterialApp(
          home: Scaffold(
            body: ThreeDView(
              core: _UnusedCore(),
              handle: 7,
              revision: 1,
              levelIds: ['level'],
              ghostedIds: [],
              floorElevationMm: 0,
              active: true,
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      Future<void> health(int session, String? error) async {
        await tester.binding.defaultBinaryMessenger.handlePlatformMessage(
          ThreeDViewState.channel.name,
          const StandardMethodCodec().encodeMethodCall(
            MethodCall('health', {'session': session, 'error': error}),
          ),
          (_) {},
        );
        await tester.pump();
      }

      await health(99, 'another session');
      expect(find.textContaining('3D rendering failed'), findsNothing);
      await health(5, 'GPU error: device lost');
      expect(find.text('3D could not update'), findsOneWidget);
      expect(find.textContaining('GPU error: device lost'), findsNothing);
      await tester.tap(find.text('3D diagnostics'));
      await tester.pumpAndSettle();
      expect(
        find.text('3D rendering failed: GPU error: device lost'),
        findsOneWidget,
      );
      await health(5, null);
      expect(find.textContaining('3D rendering failed'), findsNothing);
      expect(find.text('3D could not update'), findsNothing);
    },
  );

  testWidgets(
    'selection tints without meshes; clicks pick, double clicks open, F frames',
    (tester) async {
      final suffix = Platform.isMacOS ? 'dylib' : 'so';
      final bridge = Bridge(
        File('target/release/libdomowik_bridge.$suffix').absolute.path,
      );
      final output = alloc<Uint64>(sizeOf<Uint64>());
      expect(bridge.planCreateDefault(output), ok);
      final handle = output.value;
      release(output);
      addTearDown(() => bridge.planClose(handle));
      String call(String text) {
        final ptr = toNative(text);
        final out = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
        expect(bridge.planEdit(handle, ptr, utf8.encode(text).length, out), ok);
        final result = bridge.takeString(out);
        release(ptr);
        release(out);
        return result;
      }

      Map snapshot() {
        final out = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
        expect(bridge.planSnapshot(handle, out), ok);
        final document = jsonDecode(bridge.takeString(out)) as Map;
        release(out);
        return document;
      }

      final level = (snapshot()['levels'] as List).single['id'] as String;
      call(
        jsonEncode({
          'kind': 'addWall',
          'levelId': level,
          'start': [0, 0],
          'end': [4000, 0],
          'thicknessMm': 200,
          'heightMm': 2500,
        }),
      );
      final wallId = (snapshot()['walls'] as List).single['id'] as String;
      final calls = <MethodCall>[];
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        ThreeDViewState.channel,
        (c) async {
          calls.add(c);
          if (c.method == 'create') return {'textureId': 71, 'session': 1};
          return null;
        },
      );
      addTearDown(
        () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          ThreeDViewState.channel,
          null,
        ),
      );
      final key = GlobalKey<ThreeDViewState>();
      final picks = <ScenePick>[];
      Future<void> build(List<String> selected, {int revision = 1}) async {
        await tester.runAsync(() async {
          await tester.pumpWidget(
            MaterialApp(
              home: Scaffold(
                body: ThreeDView(
                  key: key,
                  core: FfiCore(bridge),
                  handle: handle,
                  revision: revision,
                  levelIds: [level],
                  ghostedIds: const [],
                  floorElevationMm: 0,
                  active: true,
                  selectedIds: selected,
                  onPick: picks.add,
                ),
              ),
            ),
          );
          await Future<void>.delayed(const Duration(milliseconds: 200));
        });
        await tester.pumpAndSettle();
      }

      List<List<double>> colors() => [
        for (final node
            in (calls.lastWhere((c) => c.method == 'scene').arguments
                    as Map)['nodes']
                as List)
          List<double>.from(node['color'] as List),
      ];

      await build(const []);
      final plain = colors();
      final meshes = calls.where((c) => c.method == 'mesh').length;
      await build([wallId]);
      expect(calls.where((c) => c.method == 'scene'), hasLength(2));
      expect(colors(), isNot(plain), reason: 'the selected wall is tinted');
      expect(
        calls.where((c) => c.method == 'mesh'),
        hasLength(meshes),
        reason: 'selection never re-sends geometry',
      );
      await build(const []);
      expect(colors(), plain);

      // Clicks carry explicit times: double clicks are judged by event time.
      var pointer = 10;
      Future<void> click(Offset at, int ms) async {
        final gesture = await tester.createGesture(
          pointer: pointer++,
          kind: PointerDeviceKind.mouse,
        );
        await gesture.down(at, timeStamp: Duration(milliseconds: ms));
        await gesture.up(timeStamp: Duration(milliseconds: ms + 40));
        await tester.pump(const Duration(milliseconds: 20));
      }

      // The fitted camera looks at the wall's centre: a click there picks it.
      final reads = key.currentState!.sceneRequests;
      final centre = tester.getCenter(find.byType(Texture));
      await click(centre, 1000);
      expect(picks.single.entityId, wallId);
      expect(picks.single.levelId, level);
      expect(picks.single.open, isFalse);
      expect(picks.single.planMm!.dx, closeTo(2000, 300));
      expect(picks.single.planMm!.dy.abs(), lessThan(150));
      await click(centre, 3000);
      expect(picks.last.open, isFalse, reason: 'too slow for a double click');
      await click(centre, 3200);
      expect(picks.last.open, isTrue);
      await click(centre, 3400);
      expect(picks.last.open, isFalse, reason: 'a third click starts over');
      // Empty sky picks nothing.
      await click(
        tester.getTopLeft(find.byType(Texture)) + const Offset(5, 5),
        6000,
      );
      expect(picks.last.entityId, isNull);
      expect(key.currentState!.sceneRequests, reads, reason: 'picks are views');

      // Orbiting about the wall under the press keeps the orbit distance.
      final camera = key.currentState!.camera;
      final distance = camera.distance;
      await tester.dragFrom(
        centre,
        const Offset(120, 40),
        kind: PointerDeviceKind.mouse,
      );
      await tester.pumpAndSettle();
      expect(camera.distance, closeTo(distance, 1e-9));
      expect(picks.length, 5, reason: 'a drag is not a click');
      // F frames the selection: the wall's box centre at x=0 (origin-rebased).
      await build([wallId]);
      await click(centre, 9000); // Focuses the view for the key.
      await tester.sendKeyEvent(LogicalKeyboardKey.keyF);
      await tester.pumpAndSettle();
      expect(camera.target[0], closeTo(0, 1e-6));
      expect(camera.target[1], closeTo(1.25, 1e-6));
      expect(camera.target[2], closeTo(0, 1e-6));
      final half = math.sqrt(2 * 2 + 1.25 * 1.25 + 0.1 * 0.1);
      expect(camera.distance, closeTo(half * 2.3, 1e-6));

      // Scrolling zooms toward the cursor, not the screen centre.
      final target = List.of(camera.target);
      final scroll = TestPointer(1, PointerDeviceKind.mouse);
      await tester.sendEventToBinding(
        scroll.hover(centre + const Offset(150, 0)),
      );
      await tester.sendEventToBinding(scroll.scroll(const Offset(0, -200)));
      await tester.pumpAndSettle();
      expect(camera.target, isNot(target));

      // A browser trackpad pinch (scale signal) zooms in.
      final farther = camera.distance;
      await tester.sendEventToBinding(scroll.scale(2));
      await tester.pumpAndSettle();
      expect(camera.distance, lessThan(farther));

      // An edit makes the presented scene stale until it reloads: a click
      // then neither selects nor clears.
      call(
        jsonEncode({
          'kind': 'addWall',
          'levelId': level,
          'start': [0, 10000],
          'end': [4000, 10000],
          'thicknessMm': 200,
          'heightMm': 2500,
        }),
      );
      final before = picks.length;
      await click(centre, 20000);
      expect(picks, hasLength(before), reason: 'a stale pick is not a miss');

      // The reload moves the origin to (2000, 5000) mid-flight; the framing
      // still ends on the selected wall in the new coordinates.
      await tester.sendKeyEvent(LogicalKeyboardKey.keyF);
      await tester.pump(const Duration(milliseconds: 60));
      await build([wallId], revision: 2);
      expect(camera.target[0], closeTo(0, 1e-6));
      expect(camera.target[1], closeTo(1.25, 1e-6));
      expect(camera.target[2], closeTo(5, 1e-6));
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.pumpAndSettle();
      expect(bridge.outstandingBuffers(), 0);
    },
  );

  testWidgets(
    'a selection made during the first load reuses that load as its baseline',
    (tester) async {
      final suffix = Platform.isMacOS ? 'dylib' : 'so';
      final bridge = Bridge(
        File('target/release/libdomowik_bridge.$suffix').absolute.path,
      );
      final output = alloc<Uint64>(sizeOf<Uint64>());
      expect(bridge.planCreateDefault(output), ok);
      final handle = output.value;
      release(output);
      addTearDown(() => bridge.planClose(handle));
      final snapshot = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
      expect(bridge.planSnapshot(handle, snapshot), ok);
      final level =
          ((jsonDecode(bridge.takeString(snapshot)) as Map)['levels'] as List)
                  .single['id']
              as String;
      release(snapshot);
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        ThreeDViewState.channel,
        (c) async =>
            c.method == 'create' ? {'textureId': 71, 'session': 1} : null,
      );
      addTearDown(
        () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          ThreeDViewState.channel,
          null,
        ),
      );
      final core = _SceneRecordingCore(bridge);
      Widget view(List<String> selected) => MaterialApp(
        home: Scaffold(
          body: ThreeDView(
            core: core,
            handle: handle,
            revision: 1,
            levelIds: [level],
            ghostedIds: const [],
            floorElevationMm: 0,
            active: true,
            selectedIds: selected,
          ),
        ),
      );
      await tester.runAsync(() async {
        await tester.pumpWidget(view(const []));
        await tester.pump();
        expect(core.poseOnly, [false], reason: 'the first load is in flight');
        await tester.pumpWidget(view(const ['entity']));
        await Future<void>.delayed(const Duration(milliseconds: 300));
      });
      await tester.pumpAndSettle();
      expect(core.poseOnly, [false, true]);
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.pumpAndSettle();
    },
  );
}

/// Records whether each scene request asked for a warm, transform-only update.
class _SceneRecordingCore extends FfiCore {
  _SceneRecordingCore(super.bridge);
  final List<bool> poseOnly = [];
  @override
  Future<Object?> assetsInBackground(int handle, Map<String, Object?> request) {
    if (request['kind'] == 'scene') poseOnly.add(request['poseOnly'] == true);
    return super.assetsInBackground(handle, request);
  }
}

/// For presenter tests whose scene requests never reach a core.
class _UnusedCore implements DomowikCore {
  const _UnusedCore();
  @override
  dynamic noSuchMethod(Invocation invocation) =>
      throw UnimplementedError('this test has no core');
}

class _TextureCore extends _UnusedCore {
  const _TextureCore();
  @override
  Future<Object?> assetsInBackground(
    int handle,
    Map<String, Object?> request,
  ) async {
    if (request['kind'] == 'mesh')
      return {
        'vertices': List<double>.filled(30, 0),
        'texture': {
          'width': 1,
          'height': 1,
          'layers': 1,
          'rgba': [255, 0, 0, 255],
          'coordinates': [
            0.5,
            0.5,
            0,
            1,
            1,
            0.5,
            0.5,
            0,
            1,
            1,
            0.5,
            0.5,
            0,
            1,
            1,
          ],
        },
      };
    return {
      'originMm': [0, 0],
      'center': [0, 0, 0],
      'radius': 1,
      'meshes': <String, Object?>{},
      'requiredModels': ['model'],
      'nodes': <Object?>[],
      'diagnostics': <String>[],
    };
  }
}

class _SceneCore extends _UnusedCore {
  _SceneCore(this.scene);
  Map<String, Object?> scene;
  @override
  Future<Object?> assetsInBackground(
    int handle,
    Map<String, Object?> request,
  ) async => scene;
}
