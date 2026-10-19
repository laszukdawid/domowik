// Browser-backed editor workflows (issue 0088): the real editor widgets on
// the real WebAssembly core, in Chrome. Run with
// `flutter test --platform chrome test/web` after scripts/build-web.sh has
// placed the core beside these tests (scripts/test-web.sh does both).
@TestOn('browser')
library;

import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:domowik_editor/core/domowik_core.dart';
import 'package:domowik_editor/core/web_core.dart';
import 'package:domowik_editor/file_workflows.dart';
import 'package:domowik_editor/main.dart';
import 'package:domowik_editor/plan_session.dart';
import 'package:domowik_editor/platform/config_storage_web.dart';
import 'package:domowik_editor/platform/model_import.dart';
import 'package:domowik_editor/user_config.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

/// Browser-shaped dialogs: bytes in, a writer out, no paths anywhere.
class _ByteDialogs implements FileDialogs {
  PlanBytes? opening;
  String? suggested;
  int saveCalls = 0;
  final saved = <Uint8List>[];
  Completer<void>? holdWrite;

  /// False plays a download: the bytes leave, storage is not confirmed.
  bool confirmWrites = true;
  bool unsavedGuard = false;

  @override
  Future<PlanSource?> openPlan() async => opening;

  @override
  Future<PlanTarget?> savePlan(String name) async {
    suggested = name;
    saveCalls++;
    return PlanByteTarget('browser plan.domo', (bytes) async {
      await holdWrite?.future;
      saved.add(bytes);
      return confirmWrites;
    });
  }

  @override
  void setCloseHandler(Future<bool> Function()? handler) {}
}

const _cube =
    'v -1 0 -1\nv 1 0 -1\nv 1 1 -1\nv -1 1 -1\nv -1 0 1\nv 1 0 1\nv 1 1 1\n'
    'v -1 1 1\nf 1 4 3 2\nf 5 6 7 8\nf 1 5 8 4\nf 2 3 7 6\nf 4 8 7 3\nf 1 2 6 5\n';

void main() {
  late WebCore core;
  setUpAll(() async {
    core = await WebCore.instantiate(await fetchBytes('domowik_bridge.wasm'));
  });

  Finder canvas() => find.byWidgetPredicate(
    (w) => w is CustomPaint && w.painter is PlanPainter,
  );
  PlanPainter scene(WidgetTester tester) =>
      tester.widget<CustomPaint>(canvas()).painter! as PlanPainter;

  Future<void> start(
    WidgetTester tester, {
    FileDialogs? dialogs,
    UserConfigStore? config,
    Future<(String, Uint8List)?> Function()? picker,
    Future<PickedModelDirectory?> Function()? directoryPicker,
  }) async {
    await tester.binding.setSurfaceSize(const Size(1400, 900));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      MaterialApp(
        home: PlanPage(
          bridgeLoader: () => core,
          fileDialogs: dialogs ?? _ByteDialogs(),
          configStore: config ?? UserConfigStore.inMemory(),
          modelPicker: picker,
          modelDirectoryPicker: directoryPicker,
          modelUploads: true,
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(core.openPlans(), 1);
    addTearDown(() async {
      await tester.pumpWidget(const SizedBox.shrink());
      expect(core.openPlans(), 0, reason: 'unmount closes the plan');
      expect(core.outstandingBuffers(), 0, reason: 'every buffer released');
    });
  }

  Future<void> drag(WidgetTester tester, Offset from, Offset to) async {
    await tester.tap(find.byKey(const ValueKey('wall-chain-tool')));
    await tester.pumpAndSettle();
    final top = tester.getTopLeft(canvas());
    final gesture = await tester.startGesture(
      top + from,
      kind: PointerDeviceKind.mouse,
    );
    for (var i = 1; i <= 8; i++) {
      await gesture.moveTo(top + Offset.lerp(from, to, i / 8)!);
      await tester.pump();
    }
    await gesture.up();
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(ChoiceChip, 'Select'));
    await tester.pumpAndSettle();
  }

  testWidgets('drawing, undo and redo run on the browser core', (tester) async {
    await start(tester);
    await drag(tester, const Offset(200, 200), const Offset(600, 200));
    // Clear of the first wall: a drag that starts on an endpoint moves it.
    await drag(tester, const Offset(200, 400), const Offset(600, 400));
    expect(scene(tester).walls, hasLength(2));
    expect(find.textContaining('Unsaved'), findsOneWidget);
    await tester.tap(find.byTooltip('Undo'));
    await tester.pumpAndSettle();
    expect(scene(tester).walls, hasLength(1));
    await tester.tap(find.byTooltip('Redo'));
    await tester.pumpAndSettle();
    expect(scene(tester).walls, hasLength(2));
    expect(tester.takeException(), isNull);
  });

  testWidgets('cancelling folder import leaves the plan unchanged', (
    tester,
  ) async {
    await start(tester, directoryPicker: () async => null);
    await tester.tap(find.byKey(const ValueKey('asset-library')));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('upload-asset-directory')));
    await tester.pumpAndSettle();
    expect(find.textContaining('Unsaved'), findsNothing);
    expect(find.text('Plan asset library'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('a plan saves to bytes, stays clean and reopens from them', (
    tester,
  ) async {
    final dialogs = _ByteDialogs();
    await start(tester, dialogs: dialogs);
    await drag(tester, const Offset(200, 300), const Offset(700, 300));
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    expect(dialogs.suggested, 'plan.domo');
    expect(dialogs.saved, hasLength(1));
    expect(find.text('browser plan.domo'), findsOneWidget);
    expect(find.textContaining('Unsaved'), findsNothing);

    // Replace the plan, then open the saved bytes back.
    await tester.tap(find.text('New'));
    await tester.pumpAndSettle();
    expect(scene(tester).walls, isEmpty);
    dialogs.opening = PlanBytes('reopened.domo', dialogs.saved.single);
    await tester.tap(find.byTooltip('Open plan'));
    await tester.pumpAndSettle();
    expect(scene(tester).walls, hasLength(1));
    expect(find.text('reopened.domo'), findsOneWidget);
    expect(find.textContaining('Unsaved'), findsNothing);

    // A file that is not a plan is refused and the open plan survives.
    dialogs.opening = PlanBytes('junk.domo', Uint8List.fromList([1, 2, 3]));
    await tester.tap(find.byTooltip('Open plan'));
    await tester.pumpAndSettle();
    expect(find.textContaining('ould not open'), findsOneWidget);
    expect(scene(tester).walls, hasLength(1));
  });

  testWidgets(
    'a confirmed browser save keeps its writer and a plain Save reuses it',
    (tester) async {
      final dialogs = _ByteDialogs();
      await start(tester, dialogs: dialogs);
      await drag(tester, const Offset(200, 300), const Offset(700, 300));

      // First save asks; the File System Access-style writer is confirmed.
      await tester.tap(find.text('Save'));
      await tester.pumpAndSettle();
      expect(dialogs.saveCalls, 1);
      expect(dialogs.saved, hasLength(1));

      // A second change reaches the same named file, without a new picker.
      await drag(tester, const Offset(700, 300), const Offset(700, 600));
      await tester.tap(find.text('Save'));
      await tester.pumpAndSettle();
      expect(dialogs.saveCalls, 1, reason: 'plain Save does not ask');
      expect(dialogs.saved, hasLength(2));
      expect(
        find.textContaining('Saved to', findRichText: true),
        findsOneWidget,
      );
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('an unconfirmed download keeps the plan and its unsaved mark', (
    tester,
  ) async {
    final dialogs = _ByteDialogs()..confirmWrites = false;
    await start(tester, dialogs: dialogs);
    await drag(tester, const Offset(200, 300), const Offset(700, 300));
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    expect(dialogs.saved, hasLength(1), reason: 'the bytes were offered');
    expect(find.textContaining('Unsaved'), findsOneWidget);
    expect(find.textContaining('cannot confirm'), findsOneWidget);

    // The editor cannot rewrite a download, so plain Save asks again.
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    expect(dialogs.saveCalls, 2);
    expect(dialogs.saved, hasLength(2));
    expect(find.textContaining('Unsaved'), findsOneWidget);

    // Choosing Save while leaving must not let New replace the plan.
    await tester.tap(find.text('New'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Save changes'));
    await tester.pumpAndSettle();
    expect(dialogs.saved, hasLength(3));
    expect(scene(tester).walls, hasLength(1), reason: 'the plan survives');

    // Discard, after the user has checked the download, does replace it.
    await tester.tap(find.text('New'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Discard'));
    await tester.pumpAndSettle();
    expect(scene(tester).walls, isEmpty);
  });

  testWidgets('the saved bytes are a package the core reads like a file', (
    tester,
  ) async {
    final dialogs = _ByteDialogs();
    await start(tester, dialogs: dialogs);
    await drag(tester, const Offset(200, 300), const Offset(700, 300));
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    final bytes = dialogs.saved.single;
    // ZIP local-file-header magic: an ordinary .domo package.
    expect(bytes.sublist(0, 4), [0x50, 0x4b, 0x03, 0x04]);
    final copy = core.openBytes(bytes);
    expect(copy.code, ok);
    final walls =
        (jsonDecode(core.snapshot(copy.handle).text!) as Map)['walls'] as List;
    expect(walls, hasLength(1));
    expect(core.close(copy.handle), ok);
  });

  testWidgets('an uploaded model is embedded, resized and posed in 3D', (
    tester,
  ) async {
    await start(
      tester,
      picker: () async => ('cube.obj', Uint8List.fromList(utf8.encode(_cube))),
    );
    await tester.tap(find.byKey(const ValueKey('asset-library')));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('attach-asset-directory')), findsNothing);
    await tester.tap(find.byKey(const ValueKey('upload-asset-model')));
    await tester.pumpAndSettle();
    expect(find.text('cube'), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('choose-asset')));
    await tester.pumpAndSettle();
    final top = tester.getTopLeft(canvas());
    await tester.tapAt(top + const Offset(400, 400));
    await tester.pumpAndSettle();
    expect(scene(tester).assets, hasLength(1));
    final placed = scene(tester).assets.single;
    expect(placed.name, 'cube');

    // Resize through the dialog: one atomic, undoable core batch.
    final gesture = await tester.startGesture(
      top + scene(tester).toScreen(placed.position),
      kind: PointerDeviceKind.mouse,
      buttons: kSecondaryMouseButton,
    );
    await gesture.up();
    await tester.pumpAndSettle();
    await tester.tap(find.text('Resize…'));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('resize-lock-proportions')));
    await tester.enterText(
      find.byKey(const ValueKey('resize-asset-0')),
      '2400',
    );
    await tester.enterText(find.byKey(const ValueKey('resize-asset-1')), '600');
    await tester.tap(find.byKey(const ValueKey('apply-asset-resize')));
    await tester.pumpAndSettle();
    expect(scene(tester).assets.single.widthMm, 2400);
    expect(scene(tester).assets.single.depthMm, 600);
    await tester.tap(find.byTooltip('Undo'));
    await tester.pumpAndSettle();
    expect(scene(tester).assets.single.widthMm, placed.widthMm);
    final poseButton = find.byKey(const ValueKey('pose-asset'));
    await tester.ensureVisible(poseButton);
    await tester.tap(poseButton);
    await tester.pumpAndSettle();
    final angle = find.byKey(const ValueKey('asset-pose-3'));
    await tester.ensureVisible(angle);
    await tester.enterText(angle, '100');
    await tester.pumpAndSettle();
    expect(
      scene(tester).assets.single.rotationXDeg,
      100,
      reason: 'Wasm live preview',
    );
    final cancel = find.byKey(const ValueKey('cancel-asset-pose'));
    await tester.ensureVisible(cancel);
    await tester.pumpAndSettle();
    await tester.tap(cancel);
    await tester.pumpAndSettle();
    expect(scene(tester).assets.single.rotationXDeg, 0);
    await tester.scrollUntilVisible(
      poseButton,
      -300,
      scrollable: find
          .descendant(
            of: find.byType(ListView),
            matching: find.byType(Scrollable),
          )
          .last,
    );
    await tester.ensureVisible(poseButton);
    await tester.tap(poseButton);
    await tester.pumpAndSettle();
    await tester.ensureVisible(angle);
    await tester.enterText(angle, '100');
    await tester.pumpAndSettle();
    final apply = find.byKey(const ValueKey('apply-asset-pose'));
    await tester.ensureVisible(apply);
    await tester.pumpAndSettle();
    await tester.tap(apply);
    await tester.pumpAndSettle();
    expect(scene(tester).assets.single.rotationXDeg, 100);
    await tester.tap(find.byTooltip('Undo'));
    await tester.pumpAndSettle();
    expect(scene(tester).assets.single.rotationXDeg, 0);
    expect(tester.takeException(), isNull);
  });

  testWidgets('preferences persist in browser storage', (tester) async {
    final key = 'domowik.test.${DateTime.now().microsecondsSinceEpoch}';
    final store = UserConfigStore(LocalConfigStorage(key));
    await store.load();
    expect(store.splitOrientation, SplitOrientation.vertical);
    await start(tester, config: store);
    await tester.tap(find.byTooltip('Preferences'));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('split-orientation')));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Horizontal (top and bottom)').last);
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(FilledButton, 'Save').last);
    await tester.pumpAndSettle();
    final restarted = UserConfigStore(LocalConfigStorage(key));
    await restarted.load();
    expect(restarted.splitOrientation, SplitOrientation.horizontal);
    expect(find.textContaining('Unsaved'), findsNothing);
  });

  testWidgets('a stopped core replaces the editor with a reload notice', (
    tester,
  ) async {
    // Its own instance: the panic ends it for good.
    late WebCore doomed;
    await tester.runAsync(() async {
      doomed = await WebCore.instantiate(
        await fetchBytes('domowik_bridge.wasm'),
      );
    });
    await tester.binding.setSurfaceSize(const Size(1400, 900));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      MaterialApp(
        home: PlanPage(
          bridgeLoader: () => doomed,
          fileDialogs: _ByteDialogs(),
          configStore: UserConfigStore.inMemory(),
        ),
      ),
    );
    await tester.pumpAndSettle();
    await drag(tester, const Offset(200, 300), const Offset(600, 300));
    expect(scene(tester).walls, hasLength(1));
    doomed.debugPanic();
    await tester.tap(find.byTooltip('Undo'));
    await tester.pumpAndSettle();
    expect(find.text('The core stopped.'), findsOneWidget);
    expect(find.textContaining('reload the page'), findsOneWidget);
    expect(find.textContaining('deliberate panic'), findsOneWidget);
    await tester.pumpWidget(const SizedBox.shrink());
  });

  test('a panic ends the browser core and every later call says so', () async {
    // A separate instance: the trap leaves it unusable by design.
    final doomed = await WebCore.instantiate(
      await fetchBytes('domowik_bridge.wasm'),
    );
    final plan = doomed.createDefault();
    expect(plan.code, ok);
    expect(doomed.failure, isNull);
    expect(doomed.debugPanic(), panicContained);
    expect(doomed.failure, contains('deliberate panic'));
    expect(doomed.state(plan.handle).code, panicContained);
    expect(doomed.createDefault().code, panicContained);
    expect(doomed.lastError(plan.handle), contains('deliberate panic'));
  });
}
