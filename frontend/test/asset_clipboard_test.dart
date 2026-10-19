import 'dart:io';

import 'package:domowik_bridge_plugin/domowik_bridge_plugin.dart';
import 'package:domowik_editor/asset_library.dart';
import 'package:domowik_editor/core/ffi_core.dart';
import 'package:domowik_editor/main.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

/// Real-bridge harness for the issue-0113 clipboard workflows: one plan, one
/// attached model directory, and the same canvas helpers the asset tests use.
class _Harness {
  _Harness(this.tester);

  final WidgetTester tester;

  /// Device 1 is the mouse every tester gesture already uses: a hover event
  /// after a mouse-up updates its position without re-adding the device.
  final TestPointer _mouse = TestPointer(1, PointerDeviceKind.mouse);

  Finder canvas() => find.byWidgetPredicate(
    (w) => w is CustomPaint && w.painter is PlanPainter,
  );

  PlanPainter scene() =>
      tester.widget<CustomPaint>(canvas()).painter! as PlanPainter;

  Offset get topLeft => tester.getTopLeft(canvas());

  Future<void> io() async {
    await tester.runAsync(
      () async => Future<void>.delayed(const Duration(milliseconds: 150)),
    );
    await tester.pumpAndSettle();
  }

  Future<void> tap(Finder finder) async {
    await tester.runAsync(() async {
      await tester.tap(finder);
      await Future<void>.delayed(const Duration(milliseconds: 150));
    });
    await tester.pumpAndSettle();
  }

  Future<void> click(Offset local) async {
    await tester.runAsync(() async {
      final gesture = await tester.startGesture(
        topLeft + local,
        kind: PointerDeviceKind.mouse,
      );
      await gesture.up();
      await Future<void>.delayed(const Duration(milliseconds: 150));
    });
    await io();
  }

  Future<void> rightClick(Offset local) async {
    await tester.runAsync(() async {
      final gesture = await tester.startGesture(
        topLeft + local,
        kind: PointerDeviceKind.mouse,
        buttons: kSecondaryMouseButton,
      );
      await gesture.up();
      await Future<void>.delayed(const Duration(milliseconds: 150));
    });
    await tester.pumpAndSettle();
  }

  /// Moves the mouse over the canvas without pressing, the paste anchor. Each
  /// call sends one hover event for the shared mouse device.
  Future<void> hover(Offset local) async {
    await tester.sendEventToBinding(_mouse.hover(topLeft + local));
    await tester.pumpAndSettle();
  }

  /// Sends one Control-modified shortcut to the focused canvas.
  Future<void> shortcut(LogicalKeyboardKey key, {bool shift = false}) async {
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    if (shift) await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    await tester.sendKeyEvent(key);
    if (shift) await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await io();
  }

  /// Attaches the temporary directory and places `clip sofa` at `local`.
  Future<void> placeClipSofa(Offset local) async {
    await tap(find.byKey(const ValueKey('asset-library')));
    await io();
    await tap(find.byKey(const ValueKey('attach-asset-directory')));
    await io();
    await tap(
      find
          .descendant(
            of: find.byType(AssetLibraryDialog),
            matching: find.widgetWithText(ListTile, 'clip sofa'),
          )
          .first,
    );
    await tap(find.byKey(const ValueKey('choose-asset')));
    await click(local);
  }

  Future<void> floorAction(Finder finder) async {
    await tap(find.byKey(const ValueKey('inspector-floor-tab')));
    await tap(finder);
  }
}

void main() {
  Future<_Harness> start(WidgetTester tester) async {
    final root = Directory.systemTemp.createTempSync('domowik-clipboard-');
    File('${root.path}/clip sofa.obj').writeAsStringSync(
      'v 0 1 0\nv 3 1 0\nv 3 1 1\nv 1 1 1\nv 1 1 3\nv 0 1 3\nf 1 2 3 4 5 6\n',
    );
    final output = File('${root.path}.domo');
    addTearDown(() {
      if (root.existsSync()) root.deleteSync(recursive: true);
      if (output.existsSync()) output.deleteSync();
    });
    final suffix = Platform.isMacOS ? 'dylib' : 'so';
    final bridge = Bridge(
      File('target/release/libdomowik_bridge.$suffix').absolute.path,
    );
    const files = MethodChannel('invalid.domowik-spike/files');
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(files, (
      call,
    ) async {
      if (call.method == 'savePlan' || call.method == 'openPlan')
        return output.path;
      if (call.method == 'pickAssetDirectory') return root.path;
      return null;
    });
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        files,
        null,
      ),
    );
    await tester.binding.setSurfaceSize(const Size(1400, 900));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.runAsync(() async {
      await tester.pumpWidget(
        MaterialApp(home: PlanPage(bridgeLoader: () => FfiCore(bridge))),
      );
      await Future<void>.delayed(const Duration(milliseconds: 150));
    });
    await tester.pumpAndSettle();
    addTearDown(() async {
      await tester.pumpWidget(const SizedBox.shrink());
      expect(bridge.openPlans(), 0);
      expect(bridge.outstandingBuffers(), 0);
    });
    return _Harness(tester);
  }

  testWidgets(
    'copy and paste duplicate a model at the pointer and across levels',
    (tester) async {
      final h = await start(tester);

      // An empty clipboard is inert and opens no menu on empty space.
      await h.click(const Offset(900, 700));
      await h.shortcut(LogicalKeyboardKey.keyV);
      expect(find.textContaining('The clipboard is empty'), findsOneWidget);
      await h.rightClick(const Offset(900, 700));
      expect(find.text('Paste here'), findsNothing);

      await h.placeClipSofa(const Offset(320, 360));
      expect(h.scene().assets, hasLength(1));
      final original = h.scene().assets.single;
      await h.click(h.scene().toScreen(original.position));
      expect(h.scene().selected, original.id);
      await h.shortcut(LogicalKeyboardKey.keyC);
      expect(find.textContaining('Copied 1 model'), findsOneWidget);
      expect(h.scene().assets, hasLength(1), reason: 'copy does not duplicate');

      // Paste at the pointer: the copy sits exactly under the cursor and
      // keeps the original's identity payload.
      await h.hover(const Offset(620, 460));
      final expected = h.scene().view.toPlan(const Offset(620, 460));
      await h.shortcut(LogicalKeyboardKey.keyV);
      expect(find.textContaining('Pasted 1 model'), findsOneWidget);
      expect(h.scene().assets, hasLength(2));
      final pasted = h.scene().assets.where((a) => a.id != original.id).single;
      expect(pasted.position.dx, closeTo(expected.dx, 1e-6));
      expect(pasted.position.dy, closeTo(expected.dy, 1e-6));
      expect(pasted.levelId, original.levelId);
      expect(pasted.digest, original.digest);
      expect(pasted.name, original.name);
      expect(pasted.widthMm, original.widthMm);
      expect(pasted.depthMm, original.depthMm);
      expect(pasted.heightMm, original.heightMm);
      expect(pasted.rotationDeg, original.rotationDeg);
      expect(h.scene().selected, pasted.id, reason: 'selection follows it');

      // The paste was ONE history entry and undo does not clear the clipboard.
      await h.shortcut(LogicalKeyboardKey.keyZ);
      expect(h.scene().assets, hasLength(1));
      await h.shortcut(LogicalKeyboardKey.keyZ, shift: true);
      expect(h.scene().assets, hasLength(2));

      // A new level becomes active: nothing of Level 1 shows, and the
      // clipboard still pastes there.
      await h.floorAction(find.byTooltip('Add a storey above'));
      expect(h.scene().assets, isEmpty);
      await h.hover(const Offset(700, 500));
      final target = h.scene().view.toPlan(const Offset(700, 500));
      await h.shortcut(LogicalKeyboardKey.keyV);
      expect(h.scene().assets, hasLength(1));
      final upstairs = h.scene().assets.single;
      expect(upstairs.levelId, isNot(original.levelId));
      expect(upstairs.digest, original.digest);
      expect(upstairs.position.dx, closeTo(target.dx, 1e-6));
      expect(upstairs.position.dy, closeTo(target.dy, 1e-6));

      // Back on Level 1 the two copies are still there and the menu offers
      // the whole vocabulary for a model, Paste on empty space.
      await h.floorAction(find.text('Level 1'));
      expect(h.scene().assets, hasLength(2));
      await h.rightClick(h.scene().toScreen(original.position));
      expect(find.text('Copy'), findsOneWidget);
      expect(find.text('Cut'), findsOneWidget);
      expect(find.text('Paste here'), findsOneWidget);
      await h.tap(find.text('Copy').last);
      await h.rightClick(const Offset(900, 700));
      expect(find.text('Copy'), findsNothing);
      expect(find.text('Paste here'), findsOneWidget);
      final menuTarget = h.scene().view.toPlan(const Offset(900, 700));
      await h.tap(find.text('Paste here').last);
      expect(h.scene().assets, hasLength(3));
      final fromMenu = h
          .scene()
          .assets
          .where(
            (a) =>
                a.id != original.id &&
                (a.position - menuTarget).distance < 1e-6,
          )
          .single;
      expect(fromMenu.digest, original.digest);
      await h.shortcut(LogicalKeyboardKey.keyZ);
      expect(
        h.scene().assets,
        hasLength(2),
        reason: 'the menu paste was one history entry',
      );
    },
  );

  testWidgets('cut is one undoable batch and the clipboard survives undo', (
    tester,
  ) async {
    final h = await start(tester);
    await h.placeClipSofa(const Offset(320, 360));
    final original = h.scene().assets.single;
    await h.click(h.scene().toScreen(original.position));
    expect(h.scene().selected, original.id);

    await h.shortcut(LogicalKeyboardKey.keyX);
    expect(find.textContaining('Cut 1 model'), findsOneWidget);
    expect(h.scene().assets, isEmpty);

    // ONE undo returns the model; the captured copy is still pastable.
    await h.shortcut(LogicalKeyboardKey.keyZ);
    expect(h.scene().assets, hasLength(1));
    expect(h.scene().assets.single.id, original.id);
    await h.hover(const Offset(620, 460));
    final expected = h.scene().view.toPlan(const Offset(620, 460));
    await h.shortcut(LogicalKeyboardKey.keyV);
    expect(h.scene().assets, hasLength(2));
    final pasted = h.scene().assets.where((a) => a.id != original.id).single;
    expect(pasted.position.dx, closeTo(expected.dx, 1e-6));
    expect(pasted.position.dy, closeTo(expected.dy, 1e-6));

    // Cut on Level 1, paste on Level 2: the moved model keeps its payload.
    await h.floorAction(find.byTooltip('Add a storey above'));
    expect(h.scene().assets, isEmpty);
    await h.rightClick(const Offset(600, 500));
    expect(find.text('Paste here'), findsOneWidget);
    final target = h.scene().view.toPlan(const Offset(600, 500));
    await h.tap(find.text('Paste here').last);
    expect(h.scene().assets, hasLength(1));
    final upstairs = h.scene().assets.single;
    expect(upstairs.levelId, isNot(original.levelId));
    expect(upstairs.digest, original.digest);
    expect(upstairs.name, original.name);
    expect(upstairs.widthMm, original.widthMm);
    expect(upstairs.depthMm, original.depthMm);
    expect(upstairs.heightMm, original.heightMm);
    expect(upstairs.rotationDeg, original.rotationDeg);
    expect(upstairs.position.dx, closeTo(target.dx, 1e-6));
    expect(upstairs.position.dy, closeTo(target.dy, 1e-6));

    // One undo removes the pasted model here without touching Level 1.
    await h.shortcut(LogicalKeyboardKey.keyZ);
    expect(h.scene().assets, isEmpty);
    await h.floorAction(find.text('Level 1'));
    expect(h.scene().assets, hasLength(2));
  });
}
