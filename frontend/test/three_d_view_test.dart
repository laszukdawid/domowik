import 'dart:convert';
import 'dart:ffi' hide Size;
import 'dart:io';
import 'package:domowik_bridge_plugin/domowik_bridge_plugin.dart';
import 'package:domowik_editor/three_d_view.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets(
    'navigation uploads only cameras; revisions rebuild the same plan scene',
    (tester) async {
      final suffix = Platform.isMacOS ? 'dylib' : 'so';
      final image = File(
        'target/release/libdomowik_bridge.$suffix',
      ).absolute.path;
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
                  imagePath: image,
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
}
